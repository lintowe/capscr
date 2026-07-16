// whether a status-notifier tray actually exists on this session. two-step
// on purpose: gnome ships a StatusNotifierWatcher-shaped service in some
// configurations while no host renders items (no appindicator extension),
// and TrayIconBuilder succeeds either way, so the only truthful signal is
// the watcher's IsStatusNotifierHostRegistered property.

pub fn status_notifier_present() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let watcher_owned = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &("org.kde.StatusNotifierWatcher",),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<bool>().ok())
        .unwrap_or(false);
    if !watcher_owned {
        return false;
    }
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    ) else {
        return false;
    };
    proxy
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .unwrap_or(false)
}
