//! `lattice update`, and the check for a newer release as Lattice starts.
//!
//! Both ask GitHub where its "latest release" link leads, which names the newest tag, through
//! curl (which Windows has had since 10) rather than a library of Lattice's own. The check runs
//! on a thread of its own, at most once a day, remembering the answer in the state folder in
//! between; `update-check = false` in the config leaves it out. Updating runs the installer
//! again, which picks the build, checks its checksum and puts it in place of this one.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};

use crate::log::log;

const REPO: &str = "mochreach/lattice";
/// How long the answer to a check is good for.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// What the check found, until the editor shows it.
static NEWS: Mutex<Option<String>> = Mutex::new(None);

/// The newest release's version, such as `0.2.1`.
fn latest() -> Result<String> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let url = format!("https://github.com/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args([
            "-sS",
            "-I",
            "--max-time",
            "10",
            "-o",
            null,
            "-w",
            "%{redirect_url}",
        ])
        .arg(&url)
        .output()
        .map_err(|e| anyhow!("couldn't run curl to ask GitHub: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        bail!("couldn't reach GitHub: {}", why.trim());
    }
    let to = String::from_utf8_lossy(&out.stdout);
    tag(&to).ok_or_else(|| anyhow!("GitHub didn't say which release is the latest"))
}

/// The version in a release's link: `…/releases/tag/v0.2.1` is `0.2.1`.
fn tag(link: &str) -> Option<String> {
    let (_, tag) = link.trim().rsplit_once("/tag/")?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    parse(version).map(|_| version.to_string())
}

fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|p| p.parse().ok());
    let triple = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(triple)
}

/// Whether `latest` is a later version than `current`.
fn newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Whether this Lattice came from Nix, which updates it rather than the installer.
fn from_nix(exe: &Path) -> bool {
    exe.starts_with("/nix/store")
}

/// Where the last check's answer is kept: when it was asked, and the version.
fn remembered() -> Option<PathBuf> {
    Some(crate::log::path()?.with_file_name("latest-release"))
}

fn now() -> u64 {
    // A clock before 1970 is wrong enough that checking again is right.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The newest version, from today's answer if there is one, else from GitHub.
fn latest_today() -> Result<String> {
    let path = remembered();
    let kept = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    if let Some((when, version)) = kept.as_deref().and_then(|k| k.trim().split_once(' '))
        && let Ok(when) = when.parse::<u64>()
        && now().saturating_sub(when) < DAY.as_secs()
        && parse(version).is_some()
    {
        return Ok(version.to_string());
    }
    let version = latest()?;
    if let Some(path) = path {
        // Without it, the next start asks again, which is all.
        let _ = std::fs::write(path, format!("{} {version}\n", now()));
    }
    Ok(version)
}

/// Checks for a newer release on a thread of its own, for [`news`] to pass on.
pub fn check() {
    let spawned = std::thread::Builder::new()
        .name("update check".into())
        .spawn(|| match latest_today() {
            Ok(latest) if newer(&latest, CURRENT) => {
                log!("lattice {latest} is out; this is {CURRENT}");
                let nix = std::env::current_exe().is_ok_and(|exe| from_nix(&exe));
                let how = if nix {
                    "update it through Nix"
                } else {
                    "quit and run lattice update"
                };
                let news = format!("Lattice {latest} is out (this is {CURRENT}): {how}");
                *NEWS.lock().unwrap_or_else(|e| e.into_inner()) = Some(news);
            }
            Ok(_) => {}
            // Offline, say: not worth interrupting anyone for.
            Err(e) => log!("update check: {e}"),
        });
    if let Err(e) = spawned {
        log!("update check: {e}");
    }
}

/// What the check found, once.
pub fn news() -> Option<String> {
    NEWS.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// `lattice update`: installs the newest release in place of this one.
pub fn run() -> Result<()> {
    let exe = std::env::current_exe()?;
    if from_nix(&exe) {
        bail!(
            "this Lattice came from Nix, so update it through Nix: nix run github:{REPO}, or \
             nix flake update where it's an input"
        );
    }
    let latest = latest()?;
    if !newer(&latest, CURRENT) {
        println!("lattice {CURRENT} is the latest");
        return Ok(());
    }
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("can't tell which folder {} is in", exe.display()))?;
    println!("Updating lattice {CURRENT} to {latest}");
    install(&exe, dir, &latest)?;
    // Asked again next time, rather than remembering this as out of date.
    if let Some(path) = remembered() {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Runs the installer for `version`, putting it in `dir`.
#[cfg(not(windows))]
fn install(_exe: &Path, dir: &Path, version: &str) -> Result<()> {
    let script = format!("https://raw.githubusercontent.com/{REPO}/main/install.sh");
    let status = Command::new("sh")
        .arg("-c")
        .arg(r#"{ curl -fsSL "$0" || wget -qO- "$0"; } | sh"#)
        .arg(script)
        .env("LATTICE_VERSION", format!("v{version}"))
        .env("LATTICE_INSTALL_DIR", dir)
        .status()?;
    if !status.success() {
        bail!("the installer didn't finish; the Lattice you had is still there");
    }
    Ok(())
}

/// Runs the installer for `version`, putting it in `dir`. Windows won't write over a program
/// that's running, as this one is, but will rename it, so it's moved aside first, and put back
/// if the installer fails. What's left is cleared away by [`tidy`].
#[cfg(windows)]
fn install(exe: &Path, dir: &Path, version: &str) -> Result<()> {
    let aside = exe.with_extension("old.exe");
    let _ = std::fs::remove_file(&aside);
    std::fs::rename(exe, &aside)?;
    let script = format!("https://raw.githubusercontent.com/{REPO}/main/install.ps1");
    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(format!("irm {script} | iex"))
        .env("LATTICE_VERSION", format!("v{version}"))
        .env("LATTICE_INSTALL_DIR", dir)
        .status();
    if !status.as_ref().is_ok_and(|s| s.success()) {
        if !exe.exists() {
            let _ = std::fs::rename(&aside, exe);
        }
        bail!("the installer didn't finish; the Lattice you had is still there");
    }
    Ok(())
}

/// Clears away what an update on Windows moved aside, which it couldn't remove while it ran.
pub fn tidy() {
    if cfg!(windows)
        && let Ok(exe) = std::env::current_exe()
    {
        // Usually not there at all.
        let _ = std::fs::remove_file(exe.with_extension("old.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_the_latest_releases_link() {
        let link = "https://github.com/mochreach/lattice/releases/tag/v0.2.1\n";
        assert_eq!(tag(link).as_deref(), Some("0.2.1"));
        assert_eq!(tag("https://github.com/mochreach/lattice/releases"), None);
        assert_eq!(tag(".../tag/nightly"), None);
    }

    #[test]
    fn compares_versions_by_number() {
        assert!(newer("0.2.1", "0.2.0"));
        assert!(newer("0.10.0", "0.9.3"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.9", "0.2.0"));
        assert!(!newer("rubbish", "0.2.0"));
    }

    #[test]
    fn knows_a_nix_build() {
        assert!(from_nix(Path::new(
            "/nix/store/abc-lattice-0.2.0/bin/lattice"
        )));
        assert!(!from_nix(Path::new("/home/me/.local/bin/lattice")));
    }
}
