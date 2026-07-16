// how this linux build was installed, so the updater can act correctly:
// an AppImage self-updates in place, a deb/rpm install is owned by the
// package manager and gets a deep link to the matching asset, and anything
// else falls back to the releases page. package-ownership of the actual
// running binary is the honest signal (a Fedora user can run the AppImage,
// which /etc/os-release would misclassify), so query dpkg/rpm for the exe.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    AppImage,
    Deb,
    Rpm,
    Other,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallKind::AppImage => "in-place",
            InstallKind::Deb => "deb",
            InstallKind::Rpm => "rpm",
            InstallKind::Other => "external",
        }
    }
}

pub fn detect() -> InstallKind {
    if std::env::var_os("APPIMAGE").is_some() {
        return InstallKind::AppImage;
    }
    let Ok(exe) = std::env::current_exe() else {
        return InstallKind::Other;
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    if owns(&["dpkg", "-S"], &exe) {
        return InstallKind::Deb;
    }
    if owns(&["rpm", "-qf"], &exe) {
        return InstallKind::Rpm;
    }
    InstallKind::Other
}

// true when `tool <exe>` reports the file as owned by an installed package.
// both dpkg -S and rpm -qf exit non-zero and/or print a "not owned" line for
// unowned paths, so success is exit 0 with no such marker
fn owns(tool: &[&str], exe: &std::path::Path) -> bool {
    use std::process::Command;
    let Some((bin, args)) = tool.split_first() else {
        return false;
    };
    let output = Command::new(bin).args(args).arg(exe).output();
    match output {
        Ok(out) => {
            if !out.status.success() {
                return false;
            }
            let text = String::from_utf8_lossy(&out.stdout);
            !text.contains("not owned") && !text.contains("no package")
        }
        Err(_) => false,
    }
}
