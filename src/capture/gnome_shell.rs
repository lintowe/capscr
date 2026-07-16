// client for the capscr companion gnome-shell extension
// (linux/gnome-extension), which exports the window list, per-window pixels,
// keep-above, and positioning that mutter withholds from ordinary wayland
// clients. everything here degrades to the pre-extension behaviour when the
// extension isn't installed: the portal picker for window mode, a floating
// bar and pins that mutter places wherever it likes.

use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;

const BUS: &str = "org.gnome.Shell";
const PATH: &str = "/org/gnome/Shell/Extensions/Capscr";
const IFACE: &str = "org.gnome.Shell.Extensions.Capscr";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GnomeWindow {
    pub id: u64,
    pub pid: i32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

fn proxy() -> Result<zbus::blocking::Proxy<'static>> {
    let conn = zbus::blocking::Connection::session()?;
    Ok(zbus::blocking::Proxy::new(&conn, BUS, PATH, IFACE)?)
}

pub fn available() -> bool {
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| {
        proxy()
            .and_then(|p| p.get_property::<u32>("Version").map_err(Into::into))
            .is_ok()
    })
}

// frame rects in global logical coordinates, topmost first, current
// workspace only — the same shape kwin's list_windows delivers
pub fn list_windows() -> Result<Vec<GnomeWindow>> {
    let raw: String = proxy()?.call("ListWindows", &())?;
    let windows: Vec<GnomeWindow> =
        serde_json::from_str(&raw).context("companion extension sent malformed window list")?;
    let own_pid = std::process::id() as i32;
    Ok(windows
        .into_iter()
        .filter(|w| w.pid != own_pid && w.width > 5 && w.height > 5)
        .collect())
}

// the extension paints the window's own actor offscreen, so the pixels are
// unoccluded like a windows PrintWindow capture, and hands back a png path
pub fn capture_window(id: u64) -> Result<RgbaImage> {
    let path: String = proxy()?.call("CaptureWindow", &(id,))?;
    let loaded = image::open(&path);
    let _ = std::fs::remove_file(&path);
    Ok(loaded
        .with_context(|| format!("companion capture unreadable at {path}"))?
        .to_rgba8())
}

// keep-above plus placement for a capscr window carrying `title_token` in
// its caption; false means the window hadn't mapped yet and the extension
// queued the placement for it
pub fn place_above(title_token: &str, x: i32, y: i32) -> Result<bool> {
    proxy()?
        .call("PlaceAbove", &(title_token, x, y))
        .map_err(|e| anyhow!("companion PlaceAbove failed: {e}"))
}
