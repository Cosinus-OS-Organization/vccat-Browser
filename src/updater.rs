use serde::Deserialize;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str =
    "https://api.github.com/repos/Cosinus-OS-Organization/vccat-Browser/releases/latest";

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
}

pub fn check_update() -> Option<UpdateInfo> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("vccat-browser/updater")
        .timeout(std::time::Duration::from_secs(8))
        .build().ok()?;
    let release: GhRelease = client.get(RELEASES_API).send().ok()?.json().ok()?;
    let remote_str = release.tag_name.trim_start_matches('v');
    let remote = semver::Version::parse(remote_str).ok()?;
    let current = semver::Version::parse(CURRENT_VERSION).ok()?;
    if remote <= current { return None; }
    let asset = release.assets.iter().find(|a| {
        let n = a.name.to_lowercase();
        n.contains("linux") || n == "vccat_browser" || n == "vccat-browser"
    })?;
    Some(UpdateInfo { version: release.tag_name.clone(), download_url: asset.browser_download_url.clone() })
}

pub fn apply_update(info: &UpdateInfo) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp = exe.with_extension("update_tmp");
    let client = reqwest::blocking::Client::builder()
        .user_agent("vccat-browser/updater")
        .timeout(std::time::Duration::from_secs(120))
        .build().map_err(|e| e.to_string())?;
    let bytes = client.get(&info.download_url).send()
        .map_err(|e| e.to_string())?.bytes().map_err(|e| e.to_string())?;
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &exe).map_err(|e| e.to_string())?;
    std::process::Command::new(&exe).spawn().map_err(|e| e.to_string())?;
    std::process::exit(0);
}