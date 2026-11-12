//! Finds the installed Claude Desktop MSIX package at runtime (its version folder changes on update).

use crate::error::{AppError, AppResult, ErrorCode};
use serde::Deserialize;
use std::path::PathBuf;

pub const FALLBACK_FAMILY: &str = "Claude_pzs8sxrjxfjjc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudePkg {
    pub family: String,
    pub install_dir: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AppxEntry {
    name: String,
    package_family_name: String,
    install_location: String,
}

fn missing() -> AppError {
    AppError::new(ErrorCode::ClaudeMissing, "err.claude_missing")
}

pub fn parse_appx_json(s: &str) -> AppResult<ClaudePkg> {
    let s = s.trim().trim_start_matches('\u{feff}');
    if s.is_empty() {
        return Err(missing());
    }
    let entries: Vec<AppxEntry> = if s.starts_with('[') {
        serde_json::from_str(s).map_err(|_| missing())?
    } else {
        vec![serde_json::from_str(s).map_err(|_| missing())?]
    };
    entries
        .into_iter()
        .find(|e| e.name == "Claude")
        .map(|e| ClaudePkg { family: e.package_family_name, install_dir: PathBuf::from(e.install_location) })
        .ok_or_else(missing)
}

pub fn detect() -> AppResult<ClaudePkg> {
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command",
               "Get-AppxPackage -Name Claude | Select-Object Name,PackageFamilyName,InstallLocation | ConvertTo-Json -Compress"])
        .output()?;
    parse_appx_json(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn parses_appx_json_any_version() {
        let j = r#"{"Name":"Claude","PackageFamilyName":"Claude_pzs8sxrjxfjjc","InstallLocation":"C:\\Program Files\\WindowsApps\\Claude_9.9.9.0_x64__pzs8sxrjxfjjc"}"#;
        let p = parse_appx_json(j).unwrap();
        assert_eq!(p.family, "Claude_pzs8sxrjxfjjc");
        assert!(p.install_dir.ends_with("Claude_9.9.9.0_x64__pzs8sxrjxfjjc"));
    }
    #[test]
    fn parses_array_and_picks_claude() {
        let j = r#"[{"Name":"Other","PackageFamilyName":"O_1","InstallLocation":"C:\\o"},{"Name":"Claude","PackageFamilyName":"Claude_x","InstallLocation":"C:\\c"}]"#;
        assert_eq!(parse_appx_json(j).unwrap().family, "Claude_x");
    }
    #[test]
    fn missing_package_is_claude_missing() {
        assert_eq!(parse_appx_json("").unwrap_err().code, ErrorCode::ClaudeMissing);
        assert_eq!(parse_appx_json("[]").unwrap_err().code, ErrorCode::ClaudeMissing);
    }
    #[test]
    fn detects_installed_claude() {
        let p = detect().unwrap();
        assert!(p.family.starts_with("Claude_"));
        assert!(p.install_dir.join("app").join("Claude.exe").exists());
    }
}
