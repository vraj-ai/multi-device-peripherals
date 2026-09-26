//! Start at login: a `HKCU\...\Run` value on Windows, a LaunchAgent plist on
//! macOS. Both launch `mdp ui`.

use std::io;
use std::path::Path;

/// Turn start-at-login on or off for the running executable.
pub fn set(enabled: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    return windows::set_at(windows::RUN_KEY, "mdp", &exe, enabled);
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        let plist = Path::new(&home).join("Library/LaunchAgents/com.mdp.plist");
        macos::set_at(&plist, &exe, enabled)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (exe, enabled);
        Err(io::Error::new(io::ErrorKind::Unsupported, "start at login"))
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::process::Command;

    pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    fn reg(args: &[&str]) -> io::Result<bool> {
        Ok(Command::new("reg").args(args).output()?.status.success())
    }

    /// Write or remove the `name` value under `key` (a `reg.exe` key path).
    pub fn set_at(key: &str, name: &str, exe: &Path, enabled: bool) -> io::Result<()> {
        if enabled {
            let command = format!("\"{}\" ui", exe.display());
            let ok = reg(&["add", key, "/v", name, "/t", "REG_SZ", "/d", &command, "/f"])?;
            if !ok {
                return Err(io::Error::other(format!("reg add {key} failed")));
            }
        } else if is_set(key, name)? && !reg(&["delete", key, "/v", name, "/f"])? {
            return Err(io::Error::other(format!("reg delete {key} failed")));
        }
        Ok(())
    }

    pub fn is_set(key: &str, name: &str) -> io::Result<bool> {
        reg(&["query", key, "/v", name])
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn autostart_run_value_is_written_and_removed() {
            let key = r"HKCU\Software\mdp-autostart-test";
            let exe = Path::new(r"C:\Program Files\mdp\mdp.exe");
            set_at(key, "mdp", exe, true).expect("enable");
            assert!(is_set(key, "mdp").expect("query"));
            set_at(key, "mdp", exe, false).expect("disable");
            assert!(!is_set(key, "mdp").expect("query"));
            set_at(key, "mdp", exe, false).expect("disable twice is fine");
            let _ = reg(&["delete", key, "/f"]);
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn plist(exe: &Path) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.mdp</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>ui</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            exe.display()
        )
    }

    /// Write or remove the LaunchAgent at `path`.
    pub fn set_at(path: &Path, exe: &Path, enabled: bool) -> io::Result<()> {
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, plist(exe))
        } else {
            match std::fs::remove_file(path) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn autostart_launch_agent_is_written_and_removed() {
            let dir = std::env::temp_dir().join(format!("mdp-autostart-{}", std::process::id()));
            let path = dir.join("LaunchAgents/com.mdp.plist");
            let exe = Path::new("/Applications/mdp");
            set_at(&path, exe, true).expect("enable");
            let text = std::fs::read_to_string(&path).expect("plist");
            assert!(text.contains("<string>/Applications/mdp</string><string>ui</string>"));
            assert!(text.contains("<key>RunAtLoad</key><true/>"));
            set_at(&path, exe, false).expect("disable");
            assert!(!path.exists());
            set_at(&path, exe, false).expect("disable twice is fine");
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
