// bridges gtk's live wayland connection so kde's plasma-shell role can be
// attached to a tauri webview window's surface. guest mode throughout: gtk's
// display is never owned or closed here, and every object created lives on a
// private event queue, so gtk's own dispatching is untouched.
use std::ffi::c_void;

use anyhow::{anyhow, Context, Result};
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::{GtkWindowExt, WidgetExt};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_surface::WlSurface};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_plasma::plasma_shell::client::{
    org_kde_plasma_shell, org_kde_plasma_surface,
};

unsafe extern "C" {
    // both exported by libgdk-3, which the gtk crate already links
    fn gdk_wayland_display_get_wl_display(
        display: *mut gtk::gdk::ffi::GdkDisplay,
    ) -> *mut c_void;
    fn gdk_wayland_window_get_wl_surface(window: *mut gtk::gdk::ffi::GdkWindow) -> *mut c_void;
}

pub struct PlasmaGrant {
    pub plasma_surface: org_kde_plasma_surface::OrgKdePlasmaSurface,
    _shell: org_kde_plasma_shell::OrgKdePlasmaShell,
    _connection: Connection,
}

struct Guest;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Guest {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Guest: ignore org_kde_plasma_shell::OrgKdePlasmaShell);
delegate_noop!(Guest: ignore org_kde_plasma_surface::OrgKdePlasmaSurface);

// pin a realized (ideally not yet mapped) gtk window at global coordinates in
// a layer above fullscreen windows. main thread only: gtk objects.
pub fn pin_gtk_window(gtk_window: &gtk::Window, x: i32, y: i32) -> Result<PlasmaGrant> {
    if !gtk_window.is_realized() {
        gtk_window.realize();
    }
    let display = gtk_window.display();
    let display_ptr: *mut gtk::gdk::ffi::GdkDisplay = display.to_glib_none().0;
    let gdk_window = WidgetExt::window(gtk_window).context("gtk window has no gdk window")?;
    let window_ptr: *mut gtk::gdk::ffi::GdkWindow = gdk_window.to_glib_none().0;
    let (wl_display, wl_surface_ptr) = unsafe {
        (
            gdk_wayland_display_get_wl_display(display_ptr),
            gdk_wayland_window_get_wl_surface(window_ptr),
        )
    };
    if wl_display.is_null() || wl_surface_ptr.is_null() {
        return Err(anyhow!("not a wayland gdk window"));
    }
    // guest backend: creates its own wl_event_queue on gtk's display
    let backend =
        unsafe { wayland_client::backend::Backend::from_foreign_display(wl_display.cast()) };
    let connection = Connection::from_backend(backend);
    let (globals, mut event_queue) =
        registry_queue_init::<Guest>(&connection).context("enumerate guest globals")?;
    let queue = event_queue.handle();
    let shell = globals
        .bind::<org_kde_plasma_shell::OrgKdePlasmaShell, _, _>(&queue, 1..=8, ())
        .context("bind org_kde_plasma_shell")?;
    let surface_id = unsafe {
        wayland_client::backend::ObjectId::from_ptr(WlSurface::interface(), wl_surface_ptr.cast())
    }
    .context("wrap gtk wl_surface")?;
    let surface = WlSurface::from_id(&connection, surface_id).context("adopt gtk wl_surface")?;
    let plasma_surface = shell.get_surface(&surface, &queue, ());
    // criticalnotification stacks above active fullscreen windows; the
    // pre-v6 notification role is still above everything but fullscreen
    let role = if plasma_surface.version() >= 6 {
        org_kde_plasma_surface::Role::Criticalnotification
    } else {
        org_kde_plasma_surface::Role::Notification
    };
    plasma_surface.set_role(role as u32);
    plasma_surface.set_position(x, y);
    if plasma_surface.version() >= 2 {
        plasma_surface.set_skip_taskbar(1);
    }
    if plasma_surface.version() >= 5 {
        plasma_surface.set_skip_switcher(1);
    }
    let mut guest = Guest;
    event_queue
        .roundtrip(&mut guest)
        .context("guest roundtrip")?;
    Ok(PlasmaGrant {
        plasma_surface,
        _shell: shell,
        _connection: connection,
    })
}
