//! wayland-capable global hotkeys via raw evdev reads.
//!
//! the global-hotkey crate only grabs on x11, so under a wayland session the
//! shortcuts never reach us reliably because the compositor owns input. read
//! key and button edges straight off /dev/input/event* instead: that sits below
//! the display server and works regardless of which application has focus.

use crate::hotkeys::{MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager};

// linux/input-event-codes.h — event types and the codes we react to
const EV_KEY: u16 = 0x01;
const BTN_SIDE: u16 = 0x113; // mouse4 (back)
const BTN_EXTRA: u16 = 0x114; // mouse5 (forward)
const BTN_FORWARD: u16 = 0x115;
const BTN_BACK: u16 = 0x116;

// modifier keycodes, tracked so bindings can carry ctrl/alt/shift/win
const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;

// one input_event record on 64-bit linux: timeval(16) + type(2) + code(2) + value(4)
const EVENT_SIZE: usize = 24;

static MODS: AtomicU8 = AtomicU8::new(0);
// (modifier mask, evdev button code) -> capture task id
static BINDINGS: Mutex<Option<HashMap<(u8, u16), String>>> = Mutex::new(None);

fn button_code(name_upper: &str) -> Option<u16> {
    match name_upper {
        "MOUSE4" | "XBUTTON1" => Some(BTN_SIDE),
        "MOUSE5" | "XBUTTON2" => Some(BTN_EXTRA),
        _ => None,
    }
}

fn keyboard_code(name_upper: &str) -> Option<u16> {
    if name_upper.len() == 1 {
        let byte = name_upper.as_bytes()[0];
        if byte.is_ascii_uppercase() {
            const LETTERS: [u16; 26] = [
                30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22,
                47, 17, 45, 21, 44,
            ];
            return Some(LETTERS[(byte - b'A') as usize]);
        }
        if byte.is_ascii_digit() {
            const DIGITS: [u16; 10] = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            return Some(DIGITS[(byte - b'0') as usize]);
        }
    }
    if let Some(number) = name_upper
        .strip_prefix('F')
        .and_then(|number| number.parse::<u16>().ok())
    {
        return match number {
            1..=10 => Some(58 + number),
            11 => Some(87),
            12 => Some(88),
            13..=24 => Some(170 + number),
            _ => None,
        };
    }
    match name_upper {
        "SPACE" => Some(57),
        "ENTER" | "RETURN" => Some(28),
        "TAB" => Some(15),
        "ESCAPE" | "ESC" => Some(1),
        "BACKSPACE" => Some(14),
        "DELETE" | "DEL" => Some(111),
        "INSERT" | "INS" => Some(110),
        "HOME" => Some(102),
        "END" => Some(107),
        "PAGEUP" | "PGUP" => Some(104),
        "PAGEDOWN" | "PGDN" => Some(109),
        "UP" => Some(103),
        "DOWN" => Some(108),
        "LEFT" => Some(105),
        "RIGHT" => Some(106),
        "PRINTSCREEN" | "PRTSC" | "PRINT" => Some(99),
        "PAUSE" | "PAUSEBREAK" | "BREAK" => Some(119),
        "SCROLLLOCK" | "SCROLL" => Some(70),
        "NUMPAD0" | "NUM0" | "KP0" => Some(82),
        "NUMPAD1" | "NUM1" | "KP1" => Some(79),
        "NUMPAD2" | "NUM2" | "KP2" => Some(80),
        "NUMPAD3" | "NUM3" | "KP3" => Some(81),
        "NUMPAD4" | "NUM4" | "KP4" => Some(75),
        "NUMPAD5" | "NUM5" | "KP5" => Some(76),
        "NUMPAD6" | "NUM6" | "KP6" => Some(77),
        "NUMPAD7" | "NUM7" | "KP7" => Some(71),
        "NUMPAD8" | "NUM8" | "KP8" => Some(72),
        "NUMPAD9" | "NUM9" | "KP9" => Some(73),
        "NUMPADADD" | "NUMADD" | "KPADD" => Some(78),
        "NUMPADSUB" | "NUMSUB" | "KPSUB" | "NUMPADSUBTRACT" => Some(74),
        "NUMPADMUL" | "NUMMUL" | "KPMUL" | "NUMPADMULTIPLY" => Some(55),
        "NUMPADDIV" | "NUMDIV" | "KPDIV" | "NUMPADDIVIDE" => Some(98),
        "NUMPADDOT" | "NUMDOT" | "KPDOT" | "NUMPADDECIMAL" => Some(83),
        "NUMPADENTER" | "KPENTER" => Some(96),
        _ => None,
    }
}

fn normalize_button(code: u16) -> Option<u16> {
    match code {
        BTN_SIDE | BTN_BACK => Some(BTN_SIDE),
        BTN_EXTRA | BTN_FORWARD => Some(BTN_EXTRA),
        _ => None,
    }
}

pub fn parse_evdev_binding(s: &str) -> Option<(u8, u16)> {
    let mut mods = 0u8;
    let mut code = None;
    for part in s.split('+').map(str::trim) {
        let up = part.to_ascii_uppercase();
        match up.as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CTRL,
            "ALT" => mods |= MOD_ALT,
            "SHIFT" => mods |= MOD_SHIFT,
            "SUPER" | "WIN" | "META" | "CMD" => mods |= MOD_WIN,
            other if code.is_none() => code = button_code(other).or_else(|| keyboard_code(other)),
            _ => return None,
        }
        if code.is_none()
            && !matches!(
                up.as_str(),
                "CTRL" | "CONTROL" | "ALT" | "SHIFT" | "SUPER" | "WIN" | "META" | "CMD"
            )
        {
            return None;
        }
    }
    code.map(|c| (mods, c))
}

/// parse a mouse binding for the x11 path, where keyboard shortcuts retain
/// their compositor-level grab
pub fn parse_mouse_binding(s: &str) -> Option<(u8, u16)> {
    parse_evdev_binding(s).filter(|(_, code)| matches!(*code, BTN_SIDE | BTN_EXTRA))
}

pub fn set_bindings(map: HashMap<(u8, u16), String>) {
    tracing::info!("evdev: loaded {} global hotkey binding(s)", map.len());
    *BINDINGS.lock().unwrap() = Some(map);
}

fn mod_bit(code: u16) -> Option<u8> {
    Some(match code {
        KEY_LEFTCTRL | KEY_RIGHTCTRL => MOD_CTRL,
        KEY_LEFTSHIFT | KEY_RIGHTSHIFT => MOD_SHIFT,
        KEY_LEFTALT | KEY_RIGHTALT => MOD_ALT,
        KEY_LEFTMETA | KEY_RIGHTMETA => MOD_WIN,
        _ => return None,
    })
}

/// start one reader thread per readable input device. safe to call once at
/// startup; bindings are read live from BINDINGS so a later config reload just
/// updates the map without restarting the threads.
pub fn start(app: AppHandle) {
    let devices = readable_devices();
    if devices.is_empty() {
        tracing::warn!(
            "evdev: no readable input devices — global hotkeys need read \
             access to /dev/input. add yourself to the 'input' group \
             (sudo usermod -aG input $USER) and re-login."
        );
        return;
    }
    tracing::info!(
        "evdev: watching {} input device(s) for global hotkeys",
        devices.len()
    );
    for path in devices {
        let app = app.clone();
        std::thread::Builder::new()
            .name("capscr-evdev".into())
            .spawn(move || read_device(path, app))
            .ok();
    }
}

fn readable_devices() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_event = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("event"))
            .unwrap_or(false);
        if is_event && File::open(&path).is_ok() {
            out.push(path);
        }
    }
    out
}

fn read_device(path: PathBuf, app: AppHandle) {
    let Ok(mut file) = File::open(&path) else {
        return;
    };
    let mut buf = [0u8; EVENT_SIZE];
    // per-thread dedupe so a fast double-report of one press fires once
    let mut last_fire: HashMap<String, Instant> = HashMap::new();
    loop {
        // evdev delivers whole 24-byte records; read_exact stays aligned
        if file.read_exact(&mut buf).is_err() {
            return; // device unplugged or vanished
        }
        let etype = u16::from_ne_bytes([buf[16], buf[17]]);
        if etype != EV_KEY {
            continue;
        }
        let code = u16::from_ne_bytes([buf[18], buf[19]]);
        let value = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);
        if let Some(bit) = mod_bit(code) {
            match value {
                1 => {
                    MODS.fetch_or(bit, Ordering::SeqCst);
                }
                0 => {
                    MODS.fetch_and(!bit, Ordering::SeqCst);
                }
                _ => {} // autorepeat: modifier already held
            }
            continue;
        }

        if value == 1 {
            dispatch(&app, normalize_button(code).unwrap_or(code), &mut last_fire);
        }
    }
}

fn dispatch(app: &AppHandle, code: u16, last_fire: &mut HashMap<String, Instant>) {
    if app
        .state::<crate::state::AppState>()
        .hotkeys_disabled
        .load(Ordering::SeqCst)
    {
        return;
    }
    let mods = MODS.load(Ordering::SeqCst);
    let task_id = {
        let guard = BINDINGS.lock().unwrap();
        guard.as_ref().and_then(|m| m.get(&(mods, code)).cloned())
    };
    let Some(task_id) = task_id else {
        return;
    };
    let now = Instant::now();
    let recent = last_fire
        .get(&task_id)
        .map(|t| now.duration_since(*t).as_millis() <= 250)
        .unwrap_or(false);
    if recent {
        return;
    }
    last_fire.insert(task_id.clone(), now);
    tracing::debug!("evdev: triggering task '{task_id}'");
    crate::commands::trigger_task(app, &task_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_mouse_buttons() {
        assert_eq!(parse_mouse_binding("Mouse4"), Some((0, BTN_SIDE)));
        assert_eq!(parse_mouse_binding("Mouse5"), Some((0, BTN_EXTRA)));
    }

    #[test]
    fn parses_modified_mouse_button() {
        assert_eq!(
            parse_mouse_binding("Ctrl+Mouse5"),
            Some((MOD_CTRL, BTN_EXTRA))
        );
    }

    #[test]
    fn normalizes_back_and_forward_button_codes() {
        assert_eq!(normalize_button(BTN_SIDE), Some(BTN_SIDE));
        assert_eq!(normalize_button(BTN_BACK), Some(BTN_SIDE));
        assert_eq!(normalize_button(BTN_EXTRA), Some(BTN_EXTRA));
        assert_eq!(normalize_button(BTN_FORWARD), Some(BTN_EXTRA));
    }

    #[test]
    fn ignores_keyboard_hotkeys() {
        assert_eq!(parse_mouse_binding("Ctrl+Shift+S"), None);
        assert_eq!(parse_mouse_binding("F12"), None);
    }

    #[test]
    fn parses_wayland_keyboard_bindings() {
        assert_eq!(
            parse_evdev_binding("Ctrl+Shift+G"),
            Some((MOD_CTRL | MOD_SHIFT, 34))
        );
        assert_eq!(parse_evdev_binding("F12"), Some((0, 88)));
        assert_eq!(parse_evdev_binding("PrintScreen"), Some((0, 99)));
        assert_eq!(parse_evdev_binding("NumpadEnter"), Some((0, 96)));
    }

    #[test]
    fn rejects_unknown_or_multiple_keys() {
        assert_eq!(parse_evdev_binding("Ctrl+Nope"), None);
        assert_eq!(parse_evdev_binding("Ctrl+G+H"), None);
    }
}
