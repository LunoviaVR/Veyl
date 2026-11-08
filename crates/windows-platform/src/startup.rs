//! Read-only list of startup applications: registry Run keys and Startup
//! folders, with the enabled/disabled state Task Manager stores under
//! `Explorer\StartupApproved`. Nothing here changes startup configuration.

use serde::Serialize;

use crate::registry::{
    enum_values, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use windows::Win32::System::Registry::{HKEY, REG_SAM_FLAGS};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub executable: Option<String>,
    pub location: String,
    pub scope: &'static str,
    /// From StartupApproved; `None` when Windows has no record (treated as enabled by Windows).
    pub enabled: Option<bool>,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// Extracts the executable from a command line: the quoted part, or text up to `.exe`.
pub fn executable_from_command(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        return rest.split_once('"').map(|(p, _)| p.to_owned());
    }
    let lower = cmd.to_ascii_lowercase();
    lower
        .find(".exe")
        .map(|i| cmd[..i + 4].to_owned())
        .or_else(|| cmd.split_whitespace().next().map(str::to_owned))
}

/// StartupApproved binary values: first byte even (0x02, 0x06) = enabled, odd (0x03, 0x07) = disabled.
pub fn approved_state(data: &[u8]) -> Option<bool> {
    data.first().map(|b| b % 2 == 0)
}

fn approvals(root: HKEY, sub: &str) -> Vec<(String, bool)> {
    enum_values(root, &format!(r"{APPROVED}\{sub}"), REG_SAM_FLAGS(0))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| approved_state(&v.data).map(|s| (v.name.to_lowercase(), s)))
        .collect()
}

fn from_key(
    out: &mut Vec<StartupItem>,
    root: HKEY,
    key: &str,
    view: REG_SAM_FLAGS,
    location: &str,
    scope: &'static str,
    approved: &[(String, bool)],
) {
    for v in enum_values(root, key, view).unwrap_or_default() {
        // REG_SZ (1) and REG_EXPAND_SZ (2) only.
        if v.kind != 1 && v.kind != 2 {
            continue;
        }
        let command = v.as_string();
        let enabled = approved
            .iter()
            .find(|(n, _)| *n == v.name.to_lowercase())
            .map(|(_, s)| *s);
        out.push(StartupItem {
            executable: executable_from_command(&command),
            name: v.name,
            command,
            location: location.into(),
            scope,
            enabled,
        });
    }
}

fn from_folder(
    out: &mut Vec<StartupItem>,
    dir: Option<std::path::PathBuf>,
    location: &str,
    scope: &'static str,
    approved: &[(String, bool)],
) {
    let Some(dir) = dir else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let enabled = approved
            .iter()
            .find(|(n, _)| *n == name.to_lowercase())
            .map(|(_, s)| *s);
        out.push(StartupItem {
            command: e.path().to_string_lossy().to_string(),
            executable: None,
            name,
            location: location.into(),
            scope,
            enabled,
        });
    }
}

pub fn list() -> Vec<StartupItem> {
    let mut out = Vec::new();
    let user_run = approvals(HKEY_CURRENT_USER, "Run");
    let machine_run = approvals(HKEY_LOCAL_MACHINE, "Run");
    let machine_run32 = approvals(HKEY_LOCAL_MACHINE, "Run32");
    let user_folder = approvals(HKEY_CURRENT_USER, "StartupFolder");
    let machine_folder = approvals(HKEY_LOCAL_MACHINE, "StartupFolder");
    from_key(
        &mut out,
        HKEY_CURRENT_USER,
        RUN,
        REG_SAM_FLAGS(0),
        r"HKCU\…\Run",
        "user",
        &user_run,
    );
    from_key(
        &mut out,
        HKEY_CURRENT_USER,
        RUN_ONCE,
        REG_SAM_FLAGS(0),
        r"HKCU\…\RunOnce",
        "user",
        &[],
    );
    from_key(
        &mut out,
        HKEY_LOCAL_MACHINE,
        RUN,
        KEY_WOW64_64KEY,
        r"HKLM\…\Run",
        "machine",
        &machine_run,
    );
    from_key(
        &mut out,
        HKEY_LOCAL_MACHINE,
        RUN,
        KEY_WOW64_32KEY,
        r"HKLM\…\Run (32-bit)",
        "machine",
        &machine_run32,
    );
    from_key(
        &mut out,
        HKEY_LOCAL_MACHINE,
        RUN_ONCE,
        KEY_WOW64_64KEY,
        r"HKLM\…\RunOnce",
        "machine",
        &[],
    );
    let appdata = std::env::var_os("APPDATA").map(|p| {
        std::path::PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs\Startup")
    });
    let programdata = std::env::var_os("ProgramData").map(|p| {
        std::path::PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs\StartUp")
    });
    from_folder(
        &mut out,
        appdata,
        "Startup folder (user)",
        "user",
        &user_folder,
    );
    from_folder(
        &mut out,
        programdata,
        "Startup folder (all users)",
        "machine",
        &machine_folder,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_parsing() {
        assert_eq!(
            executable_from_command(r#""C:\Program Files\App\app.exe" --min"#).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        assert_eq!(
            executable_from_command(r"C:\Tools\x.EXE /background").as_deref(),
            Some(r"C:\Tools\x.EXE")
        );
        assert_eq!(
            executable_from_command("rundll32 shell32.dll").as_deref(),
            Some("rundll32")
        );
        assert_eq!(executable_from_command("\"unterminated"), None);
        assert_eq!(executable_from_command(""), None);
    }

    #[test]
    fn approved_bytes() {
        assert_eq!(approved_state(&[0x02, 0, 0]), Some(true));
        assert_eq!(approved_state(&[0x06]), Some(true));
        assert_eq!(approved_state(&[0x03, 1]), Some(false));
        assert_eq!(approved_state(&[]), None);
    }

    #[test]
    fn listing_does_not_fail() {
        let _ = list();
    }
}
