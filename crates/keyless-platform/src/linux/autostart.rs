//! Start at sign-in through an XDG autostart entry (`~/.config/autostart`), which GNOME,
//! KDE, Xfce, Cinnamon, MATE and most other desktops read. No root needed.

use std::path::PathBuf;

use crate::Error;

fn entry() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    // Named after the app, so a dev build has its own.
    Some(config.join("autostart").join(format!("{}.desktop", super::app_id())))
}

/// The file to start. An AppImage runs from a temporary mount, so use the image itself.
fn executable() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok())
}

/// Quotes an argument for the `Exec` key of a desktop entry.
fn quote(argument: &str) -> String {
    let mut quoted = String::from("\"");
    for c in argument.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    // The whole value is a desktop entry string, where a backslash is escaped once more.
    quoted.replace('\\', "\\\\")
}

fn exec_line() -> Option<String> {
    Some(format!("Exec={} --background", quote(&executable()?.to_string_lossy())))
}

/// True when the sign-in entry starts this executable (not an older copy elsewhere).
pub fn is_enabled() -> bool {
    let (Some(entry), Some(exec)) = (entry(), exec_line()) else { return false };
    std::fs::read_to_string(entry).is_ok_and(|text| {
        text.lines().any(|line| line.trim() == exec) && !text.lines().any(|line| line.trim() == "Hidden=true")
    })
}

/// Enabling points the entry at the running executable, started in the background.
pub fn set_enabled(enabled: bool) -> Result<(), Error> {
    let os = |e: std::io::Error| Error::Os(e.to_string());
    let entry = entry().ok_or_else(|| Error::Os("no home folder".into()))?;
    if !enabled {
        return match std::fs::remove_file(&entry) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(os(e)),
            _ => Ok(()),
        };
    }
    let exec = exec_line().ok_or_else(|| Error::Os("unknown executable path".into()))?;
    let name = keyless_core::APP_NAME;
    let text = format!(
        "[Desktop Entry]\nType=Application\nName={name}\n{exec}\nIcon=keyless\nTerminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    if let Some(dir) = entry.parent() {
        std::fs::create_dir_all(dir).map_err(os)?;
    }
    std::fs::write(&entry, text).map_err(os)
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quoting() {
        assert_eq!(quote("/usr/bin/keyless"), "\"/usr/bin/keyless\"");
        assert_eq!(quote("/home/a b/Keyless.AppImage"), "\"/home/a b/Keyless.AppImage\"");
        assert_eq!(quote("/opt/$x"), "\"/opt/\\\\$x\"");
    }
}
