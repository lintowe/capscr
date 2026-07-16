# platform limits

capscr aims for the same behaviour on Linux as on Windows, and reaches it
everywhere the platform allows. a handful of differences remain that no amount
of capscr code can close: they are boundaries in the OS or the Wayland
compositor, not missing features. this file is the honest list, why each one
exists, and what would have to change upstream to close it.

run `capscr --wayland-diag` on any Linux session for a live readout of which
of these apply to that machine.

## HDR-preserved capture (Windows only)

Windows captures HDR displays through Windows.Graphics.Capture in FP16 and
tonemaps to SDR (or writes an HDR-preserved PNG). No Linux compositor hands a
capture client HDR pixels as of mid-2026:

- KWin's `org.kde.KWin.ScreenShot2` returns 8-bit `QImage` frames with no
  colour metadata (checked against KWin 6.7.2).
- KDE declined the `ext-image-copy-capture` staging protocol that could carry
  deep buffers ([bug 513785], "resolved intentional", portals-first policy).
- The screenshot/screencast portals advertise 8-bit formats only.

So Linux captures are SDR. The tonemap and cICP-PNG pipeline
(`src/capture/tonemapping.rs`, `src/capture/hdr_png.rs`) is cross-platform and
already exercised on synthetic frames; `capscr --wayland-diag` reports each
output's colour signal and whether any capture source offers a >8-bit format.
The day a compositor exposes deep buffers, that readout flips and the backend
seam in `src/capture/hdr.rs` (`is_hdr_at_point` / `capture_raw` /
`capture_with_hdr_at`) is where the source plugs in.

**closes when:** KWin or Mutter exposes HDR pixels to a capture client (e.g. a
colour-managed `ext-image-copy-capture` frame).

## GNOME window picking (closed by the companion extension)

On X11, KDE, and wlroots, clicking a window in capscr's own overlay picks it.
Mutter gives ordinary apps no window list or per-window capture API, so plain
GNOME routes window-mode capture through the screenshot portal's interactive
mode: GNOME draws its own picker, capscr receives the chosen pixels.

The bundled companion extension (`linux/gnome-extension`, installable from
Settings → general on a GNOME session) closes this: extension code runs
inside the shell where the window list, stacking order, and window actors are
all reachable, and it hands capscr the same window rects and per-window
pixels KWin's ScreenShot2 provides. With it active, window mode uses capscr's
own overlay exactly like everywhere else.

**closes without the extension when:** Mutter offers a sanctioned
window-enumeration or window-capture API to unsandboxed clients.

## GNOME keep-above (closed by the companion extension)

The recording bar and pinned screenshots stay above other windows, including
fullscreen ones, on X11 (always-on-top), KDE (plasma-shell / KWin scripting),
and wlroots (layer-shell overlay). Mutter exposes no layer-shell to regular
clients and ignores client positioning entirely, so on plain GNOME these fall
back to a normal window: visible, but wherever Mutter puts it and not
guaranteed above a fullscreen surface.

The companion extension closes this too: it sets keep-above (Mutter stacks
those in a layer above fullscreen windows) and moves the bar and pins to the
spots capscr asks for.

**closes without the extension when:** Mutter supports layer-shell for
applications (long-declined upstream).

## recording bar visible in an everything-covering recording (closed on Plasma 6.7+)

Windows excludes the recording control bar from capture outright
(`SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`). Plasma 6.7 grew
the compositor-side equivalent: a per-window `excludeFromCapture` property
that KWin honours in screenshots and screencasts alike (6.6 introduced it for
screencasts only). capscr sets it on the bar through a KWin script, so on
Plasma 6.7+ the bar places exactly like on Windows, sitting inside the region
without appearing in the frames.

Elsewhere the boundary stands. Mutter has nothing comparable. Hyprland 0.50's
`no_screen_share` rule censors the window to a black box rather than removing
it, which looks worse in a recording than the bar itself, so capscr doesn't
use it. On those desktops (and X11, where root-window capture reads final
composited pixels) the bar keeps to the outside placement: below, above, or
beside the region, spilling onto a second monitor when the region fills the
first, inside it only when the region covers every monitor.

**closes elsewhere when:** the `ext-surface-capture-control` protocol
proposal (wayland-protocols MR 450) or an equivalent lands in the remaining
compositors.

Related: KWin ≥ 6.6.1 hides *all* of a caller's windows from its own
ScreenShot2 grabs by default, which would silently drop pinned screenshots
from user captures — they are ordinary windows on Windows and belong in the
shot. capscr passes `hide-caller-windows: false` and excludes only the bar.

## GNOME system tray (closed by the companion extension)

capscr is tray-first. GNOME ships no StatusNotifier host by default, so the
tray icon only appears if the user installs the AppIndicator extension. The
companion extension adds its own top-bar button carrying the capture menu, a
native stand-in for the tray (deliberately not a bundled StatusNotifier host,
which would fight the AppIndicator extension over the watcher name). Without
either extension, capscr detects the missing host at startup and surfaces its
hub with a one-time explanation; global hotkeys, the desktop-file capture
actions, and relaunching to reopen the hub all keep it reachable.

**closes without an extension when:** GNOME ships a StatusNotifier host.

## implementation differences that are NOT behaviour differences

These differ under the hood but produce the same result, so they aren't gaps:

- **pixel source** — Windows uses WGC/DXGI/GDI; Linux picks per compositor
  (KWin ScreenShot2, `ext-image-copy-capture`, wlr-screencopy, or the
  portal), ordered at runtime by `src/capture/wayland_chain.rs`.
- **recording audio** — WASAPI loopback on Windows, the PulseAudio/PipeWire
  monitor on Linux.
- **global hotkeys** — a low-level hook on Windows; X11 grabs, the
  GlobalShortcuts portal, or opt-in evdev on Linux.
- **credential vault** — DPAPI on Windows, the freedesktop Secret Service on
  Linux.
- **OCR** — the built-in Windows OCR engine, `tesseract` on Linux.
- **native menu theming** — Windows needs a nudge to render the tray and
  context menus dark (`win_darkmode.rs`); GTK menus follow the system theme
  on their own.

[bug 513785]: https://bugs.kde.org/show_bug.cgi?id=513785
