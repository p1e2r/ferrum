// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Checks GitHub for a newer Ferrum release, and installs it in one click:
//! the package for this computer is downloaded, the new version is put in
//! place of the running one, and Ferrum restarts (the old version keeps
//! running until then; operating systems allow replacing a running program
//! by renaming it out of the way).
//!
//! - macOS: the `.dmg` for this processor is mounted and `Ferrum.app` copied
//!   over the current app bundle.
//! - Windows: the `.zip` is unpacked and `ferrum.exe` replaced.
//! - Linux: the `.tar.gz` is unpacked and the `ferrum` binary replaced.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RELEASES_API: &str = "https://api.github.com/repos/p1e2r/ferrum/releases?per_page=30";
pub const RELEASES_PAGE: &str = "https://github.com/p1e2r/ferrum/releases";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    /// `url` is the release page; `asset` the package for this computer, if there is one.
    /// `sums` is the release's SHA256SUMS file, to check the download against.
    Available { version: String, url: String, asset: Option<String>, sums: Option<String> },
    Downloading { version: String, received: u64, total: Option<u64> },
    Installing { version: String },
    /// Installed; restart with this program.
    Installed { version: String, program: PathBuf },
    Failed(String),
}

/// The part of a package name that matches this computer.
fn package_suffix() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-apple-silicon.dmg"),
        ("macos", "x86_64") => Some("macos-intel.dmg"),
        ("windows", "x86_64") => Some("windows-x86_64.zip"),
        ("linux", "x86_64") => Some("linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Some("linux-arm64.tar.gz"),
        _ => None,
    }
}

/// `"v0.6.2"` → `(0, 6, 2)`.
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// A release on GitHub this computer can update to.
struct Found {
    version: String,
    page: String,
    package: Option<String>,
    sums: Option<String>,
}

/// The newest release in a GitHub releases list (drafts and pre-releases skipped).
fn newest(json: &serde_json::Value) -> Option<Found> {
    json.as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            let assets = r["assets"].as_array();
            let find = |pred: &dyn Fn(&str) -> bool| {
                assets?.iter().find_map(|a| {
                    let name = a["name"].as_str()?;
                    pred(name).then(|| a["browser_download_url"].as_str().map(str::to_owned)).flatten()
                })
            };
            let found = Found {
                version: tag.trim_start_matches('v').to_string(),
                page: r["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
                package: package_suffix().and_then(|suffix| find(&|n: &str| n.ends_with(suffix))),
                sums: find(&|n: &str| n == "SHA256SUMS"),
            };
            Some((parse_version(tag)?, found))
        })
        .max_by_key(|(v, _)| *v)
        .map(|(_, f)| f)
}

/// Start a check; the result appears in the returned slot. `notify` wakes the UI.
pub fn check(current: &str, notify: Arc<dyn Fn() + Send + Sync>) -> Arc<Mutex<UpdateStatus>> {
    let slot = Arc::new(Mutex::new(UpdateStatus::Checking));
    let out = slot.clone();
    let current = current.to_string();
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let status = match fetch() {
            Ok(json) => match newest(&json) {
                Some(f) if parse_version(&f.version) > parse_version(&current) => {
                    UpdateStatus::Available { version: f.version, url: f.page, asset: f.package, sums: f.sums }
                },
                _ => UpdateStatus::UpToDate,
            },
            Err(e) => UpdateStatus::Failed(e),
        };
        if let Ok(mut s) = out.lock() {
            *s = status;
        }
        notify();
    });
    slot
}

/// Download and install `asset` on a background thread, reporting progress in `slot`.
pub fn install(version: String, asset: String, sums: Option<String>, slot: Arc<Mutex<UpdateStatus>>, notify: Arc<dyn Fn() + Send + Sync>) {
    let _ = std::thread::Builder::new().name("update-install".into()).spawn(move || {
        let set = |s: UpdateStatus| {
            if let Ok(mut slot) = slot.lock() {
                *slot = s;
            }
            notify();
        };
        set(UpdateStatus::Downloading { version: version.clone(), received: 0, total: None });
        let work = std::env::temp_dir().join(format!("ferrum-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let result = std::fs::create_dir_all(&work)
            .map_err(|e| e.to_string())
            .and_then(|_| download(&asset, &work, |received, total| set(UpdateStatus::Downloading { version: version.clone(), received, total })))
            .and_then(|package| {
                set(UpdateStatus::Installing { version: version.clone() });
                if let Some(sums) = &sums {
                    verify(&package, sums)?;
                }
                put_in_place(&package, &work)
            });
        let _ = std::fs::remove_dir_all(&work);
        match result {
            Ok(program) => set(UpdateStatus::Installed { version, program }),
            Err(e) => {
                log::error!("Update failed: {e}");
                set(UpdateStatus::Failed(e))
            },
        }
    });
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(crate::tls_config())
        .timeout_connect(Some(Duration::from_secs(20)))
        .user_agent(format!("Ferrum/{}", crate::VERSION))
        .build()
        .into()
}

/// Check a downloaded package against the release's SHA256SUMS
/// (lines of `<hex digest>  <file name>`).
fn verify(package: &Path, sums_url: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let name = package.file_name().and_then(|n| n.to_str()).ok_or("bad file name")?;
    let text = agent()
        .get(sums_url)
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| format!("Couldn't check the download: {e}"))?;
    let expected = expected_digest(&text, name).ok_or("The download isn't listed in the release's checksums")?;
    let mut file = std::fs::File::open(package).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual == expected {
        Ok(())
    } else {
        Err("The download is damaged (its checksum doesn't match). Try again later.".into())
    }
}

fn expected_digest(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

fn download(url: &str, dir: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<PathBuf, String> {
    let mut response = agent().get(url).call().map_err(|e| format!("Download failed: {e}"))?;
    let total = response.body().content_length();
    let name = url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("package");
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    let mut last = Instant::now();
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        received += n as u64;
        if last.elapsed() > Duration::from_millis(200) {
            progress(received, total);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    if total.is_some_and(|t| t != received) {
        return Err("The download was incomplete".into());
    }
    Ok(path)
}

fn run(cmd: &mut std::process::Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{:?} failed: {}", cmd.get_program(), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Where a file can't be written, say so in words.
fn no_permission(e: std::io::Error, place: &Path) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!("Ferrum can't write to {}. Move Ferrum to a folder you own, or install the update by hand.", place.display())
    } else {
        e.to_string()
    }
}

/// Swap `new` in for `current`, keeping the old one at `old` until it's gone.
fn swap(current: &Path, new: &Path, old: &Path) -> Result<(), String> {
    remove_any(old);
    std::fs::rename(current, old).map_err(|e| no_permission(e, current.parent().unwrap_or(current)))?;
    if let Err(e) = std::fs::rename(new, current) {
        // Put the old one back.
        let _ = std::fs::rename(old, current);
        return Err(no_permission(e, current.parent().unwrap_or(current)));
    }
    Ok(())
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// The app bundle Ferrum runs from on macOS (`…/Ferrum.app`).
#[cfg(target_os = "macos")]
fn app_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = exe.parent().and_then(Path::parent).and_then(Path::parent).ok_or("not in an app bundle")?;
    if bundle.extension().is_some_and(|e| e == "app") {
        Ok(bundle.to_path_buf())
    } else {
        Err("This copy of Ferrum isn't in an app bundle, so it can't update itself.".into())
    }
}

#[cfg(target_os = "macos")]
fn put_in_place(package: &Path, work: &Path) -> Result<PathBuf, String> {
    use std::process::Command;
    let bundle = app_bundle()?;
    let parent = bundle.parent().ok_or("no folder")?;
    let mount = work.join("mnt");
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    run(Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"]).arg(&mount).arg(package))?;
    // Copy next to the current app, so the swap is a rename on the same disk.
    let staged = parent.join(".Ferrum-update.app");
    remove_any(&staged);
    let copied = run(Command::new("ditto").arg(mount.join("Ferrum.app")).arg(&staged));
    let _ = run(Command::new("hdiutil").args(["detach", "-quiet", "-force"]).arg(&mount));
    copied.map_err(|e| if e.contains("ermission") { no_permission(std::io::ErrorKind::PermissionDenied.into(), parent) } else { e })?;
    let old = parent.join(".Ferrum-old.app");
    swap(&bundle, &staged, &old)?;
    remove_any(&old);
    Ok(bundle)
}

#[cfg(target_os = "windows")]
fn put_in_place(package: &Path, work: &Path) -> Result<PathBuf, String> {
    use std::process::Command;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = work.join("unpacked");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // tar.exe ships with Windows 10 and 11 and reads .zip files.
    run(Command::new("tar").arg("-xf").arg(package).arg("-C").arg(&out))?;
    let new = out.join("Ferrum").join("ferrum.exe");
    if !new.is_file() {
        return Err("The update package has no ferrum.exe".into());
    }
    let dir = exe.parent().ok_or("no folder")?;
    let staged = dir.join("ferrum.new.exe");
    std::fs::copy(&new, &staged).map_err(|e| no_permission(e, dir))?;
    // A running .exe can be renamed but not deleted; the old one is removed at the next start.
    swap(&exe, &staged, &dir.join("ferrum.old.exe"))?;
    Ok(exe)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn put_in_place(package: &Path, work: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = work.join("unpacked");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    run(Command::new("tar").arg("-xzf").arg(package).arg("-C").arg(&out))?;
    let new = out.join("ferrum").join("ferrum");
    if !new.is_file() {
        return Err("The update package has no ferrum program".into());
    }
    let dir = exe.parent().ok_or("no folder")?;
    let staged = dir.join(".ferrum.new");
    std::fs::copy(&new, &staged).map_err(|e| no_permission(e, dir))?;
    let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    let old = dir.join(".ferrum.old");
    swap(&exe, &staged, &old)?;
    remove_any(&old);
    Ok(exe)
}

/// Start `program` once this process has quit.
pub fn relaunch_after_exit(program: &Path) -> Result<(), String> {
    let pid = std::process::id();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open \"$0\""))
        .arg(program)
        .spawn();
    #[cfg(target_os = "windows")]
    let result = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command"])
            .arg(format!("Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}'", program.display().to_string().replace('\'', "''")))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; exec \"$0\" >/dev/null 2>&1"))
        .arg(program)
        .spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Remove what an earlier update left behind (Windows can't delete a running .exe).
pub fn clean_up_after_update() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent() {
            for name in ["ferrum.old.exe", "ferrum.new.exe", ".ferrum.old", ".ferrum.new"] {
                let _ = std::fs::remove_file(dir.join(name));
            }
        }
}

fn fetch() -> Result<serde_json::Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(crate::tls_config())
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(RELEASES_API)
        .header("User-Agent", "Ferrum-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let text = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.6.2"), Some((0, 6, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.5.0-nightly"), Some((0, 5, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn picks_highest_version() {
        let json: serde_json::Value = serde_json::from_str(
            r#"[{"tag_name":"v0.4.0","html_url":"a"},{"tag_name":"v0.10.1","html_url":"b"},{"tag_name":"nightly","html_url":"c"},{"tag_name":"v0.9.0","html_url":"d","draft":true}]"#,
        )
        .unwrap();
        assert_eq!(newest(&json).map(|f| (f.version, f.page)), Some(("0.10.1".into(), "b".into())));
    }

    #[test]
    fn skips_pre_releases_and_reads_checksums() {
        let json: serde_json::Value = serde_json::from_str(
            r#"[{"tag_name":"v1.0.2","html_url":"pre","prerelease":true},{"tag_name":"v1.0.1","html_url":"ok"}]"#,
        )
        .unwrap();
        assert_eq!(newest(&json).map(|f| f.page), Some("ok".into()));
        let sums = "abc123  Ferrum-1.0.0-macos-intel.dmg\nDEF456 *ferrum-1.0.0-windows-x86_64.zip\n";
        assert_eq!(expected_digest(sums, "Ferrum-1.0.0-macos-intel.dmg").as_deref(), Some("abc123"));
        assert_eq!(expected_digest(sums, "ferrum-1.0.0-windows-x86_64.zip").as_deref(), Some("def456"));
        assert_eq!(expected_digest(sums, "other"), None);
    }

    #[test]
    fn finds_this_computers_package() {
        let Some(suffix) = package_suffix() else { return };
        let json: serde_json::Value = serde_json::json!([{
            "tag_name": "v1.0.0", "html_url": "p",
            "assets": [
                {"name": "ferrum-1.0.0-something-else.bin", "browser_download_url": "x"},
                {"name": format!("ferrum-1.0.0-{suffix}"), "browser_download_url": "right"}
            ]
        }]);
        assert_eq!(newest(&json).and_then(|f| f.package), Some("right".into()));
    }
}
