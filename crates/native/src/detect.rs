//! Finding Craft Apps that are already installed, however they got there.
//!
//! - macOS: `<App>.app` in `/Applications` or `~/Applications`, read from its `Info.plist`.
//! - Windows: the Windows Installer's uninstall entries (`Learning Machines LLC`), and portable
//!   copies the launcher unpacked.
//! - Linux: AppImages in `~/Applications`, Debian/RPM packages, Flatpaks, and tarball installs.
//! - FreeBSD: tarball installs and `/usr/local/bin/<app>`.

use artcraft_launcher_core::{CraftApp, InstallSource, Installed, version};

use crate::manifest::Manifest;

/// One look at the system, shared by every app's detection (package lists are read once).
#[derive(Default)]
pub struct Snapshot {
    #[allow(dead_code)]
    packages: Vec<(String, String, artcraft_launcher_core::InstallSource)>,
}

impl Snapshot {
    pub fn take() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self { packages: linux::packages() }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::default()
        }
    }
}

/// What's installed of `app`, best source first.
pub fn detect(app: &CraftApp, manifest: &Manifest, snapshot: &Snapshot) -> Option<Installed> {
    let _ = snapshot;
    if let Some(found) = from_manifest(app, manifest) {
        return Some(found);
    }
    #[cfg(target_os = "macos")]
    return macos::detect(app);
    #[cfg(windows)]
    return windows::detect(app);
    #[cfg(target_os = "linux")]
    return linux::detect(app, snapshot);
    #[cfg(target_os = "freebsd")]
    return freebsd::detect(app);
    #[allow(unreachable_code)]
    None
}

fn from_manifest(app: &CraftApp, manifest: &Manifest) -> Option<Installed> {
    let rec = manifest.apps.get(app.id)?;
    if !rec.path.exists() {
        return None;
    }
    // On macOS the bundle itself is the truth (the app may have updated itself).
    #[cfg(target_os = "macos")]
    if let Some(found) = macos::read_bundle(app, &rec.path) {
        return Some(found);
    }
    Some(Installed { version: version::parse(&rec.version), source: InstallSource::Launcher, location: rec.path.display().to_string() })
}

#[cfg(target_os = "macos")]
pub mod macos {
    use std::path::{Path, PathBuf};

    use super::*;

    /// Where a `.app` might be: the system and the user's Applications folders.
    pub fn candidates(app: &CraftApp) -> Vec<PathBuf> {
        let bundle = format!("{}.app", app.name);
        let mut out = vec![Path::new("/Applications").join(&bundle)];
        if let Some(home) = dirs::home_dir() {
            out.push(home.join("Applications").join(&bundle));
        }
        out
    }

    pub fn detect(app: &CraftApp) -> Option<Installed> {
        candidates(app).iter().find_map(|p| read_bundle(app, p))
    }

    /// Reads a bundle's `Info.plist`; `None` unless it's really this app.
    pub fn read_bundle(app: &CraftApp, path: &Path) -> Option<Installed> {
        let plist = plist::Value::from_file(path.join("Contents/Info.plist")).ok()?;
        let dict = plist.as_dictionary()?;
        if dict.get("CFBundleIdentifier")?.as_string()? != app.bundle_id() {
            return None;
        }
        // `<App>Version` carries the full version including a pre-release suffix;
        // CFBundleShortVersionString only has the numbers.
        let full_key = format!("{}Version", app.name);
        let text =
            [full_key.as_str(), "CFBundleShortVersionString", "CFBundleVersion"].iter().find_map(|k| dict.get(k).and_then(plist::Value::as_string));
        Some(Installed { version: text.and_then(version::parse), source: InstallSource::MacApp, location: path.display().to_string() })
    }
}

#[cfg(windows)]
pub mod windows {
    use std::path::PathBuf;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    use super::*;

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const PUBLISHER: &str = "Learning Machines LLC";

    /// An MSI install found in the registry.
    pub struct MsiInstall {
        pub product_code: String,
        pub version: Option<artcraft_launcher_core::version::Version>,
        pub exe: Option<PathBuf>,
    }

    /// Every Windows Installer entry for `app`, newest first.
    pub fn msi_installs(app: &CraftApp) -> Vec<MsiInstall> {
        let mut out = Vec::new();
        for (hive, flags) in [(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY), (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY), (HKEY_CURRENT_USER, 0)] {
            let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL, KEY_READ | flags) else { continue };
            for code in root.enum_keys().flatten() {
                let Ok(key) = root.open_subkey_with_flags(&code, KEY_READ | flags) else { continue };
                let name: String = key.get_value("DisplayName").unwrap_or_default();
                let publisher: String = key.get_value("Publisher").unwrap_or_default();
                if name != app.name || publisher != PUBLISHER || out.iter().any(|m: &MsiInstall| m.product_code == code) {
                    continue;
                }
                let version = key.get_value::<String, _>("DisplayVersion").ok().and_then(|v| version::parse(&v));
                let location: String = key.get_value("InstallLocation").unwrap_or_default();
                let exe = exe_path(app, (!location.is_empty()).then(|| PathBuf::from(location)));
                out.push(MsiInstall { product_code: code, version, exe });
            }
        }
        out.sort_by(|a, b| match (&b.version, &a.version) {
            (Some(x), Some(y)) => version::compare(x, y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        out
    }

    /// The app's exe: from `App Paths`, the install folder, or Program Files.
    fn exe_path(app: &CraftApp, location: Option<PathBuf>) -> Option<PathBuf> {
        let exe = format!("{}.exe", app.id);
        let app_paths = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
        for flags in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(&app_paths, KEY_READ | flags)
                && let Ok(path) = key.get_value::<String, _>("")
            {
                let p = PathBuf::from(path.trim_matches('"'));
                if p.exists() {
                    return Some(p);
                }
            }
        }
        let mut dirs: Vec<PathBuf> = location.into_iter().collect();
        for var in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                dirs.push(PathBuf::from(pf).join(app.name));
            }
        }
        dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.exists())
    }

    pub fn detect(app: &CraftApp) -> Option<Installed> {
        let m = msi_installs(app).into_iter().next()?;
        Some(Installed {
            version: m.version,
            source: InstallSource::Msi,
            location: m.exe.map_or_else(|| app.name.to_string(), |p| p.display().to_string()),
        })
    }
}

#[cfg(target_os = "linux")]
pub mod linux {
    use std::path::PathBuf;
    use std::time::Duration;

    use artcraft_launcher_core::platform::parse_asset_name;

    use super::*;
    use crate::process::capture;

    /// Installed Craft App packages: (name, version, source). Read once per detection pass.
    pub fn packages() -> Vec<(String, String, InstallSource)> {
        let wanted = |name: &str| artcraft_launcher_core::SUITE.iter().any(|a| a.id == name);
        let mut out = Vec::new();
        let t = Duration::from_secs(10);
        if let Some(list) = capture("dpkg-query", &["-W", "-f", "${Package}|${db:Status-Status}|${Version}\n"], t) {
            for line in list.lines() {
                let mut f = line.split('|');
                if let (Some(name), Some("installed"), Some(ver)) = (f.next(), f.next(), f.next())
                    && wanted(name)
                {
                    out.push((name.to_string(), ver.to_string(), InstallSource::Deb));
                }
            }
        }
        if let Some(list) = capture("rpm", &["-qa", "--qf", "%{NAME}|%{VERSION}\n"], t) {
            for line in list.lines() {
                if let Some((name, ver)) = line.split_once('|')
                    && wanted(name)
                {
                    out.push((name.to_string(), ver.to_string(), InstallSource::Rpm));
                }
            }
        }
        if let Some(list) = capture("flatpak", &["list", "--app", "--columns=application,version"], t) {
            for line in list.lines() {
                let mut f = line.split('\t');
                if let (Some(id), Some(ver)) = (f.next(), f.next())
                    && let Some(name) = id.strip_prefix("ai.storyteller.")
                    && wanted(name)
                {
                    out.push((name.to_string(), ver.to_string(), InstallSource::Flatpak));
                }
            }
        }
        out
    }

    /// `~/Applications`, where the launcher puts AppImages (the AppImageLauncher convention).
    pub fn appimage_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Applications"))
    }

    /// AppImages of `app` in `~/Applications`, newest first.
    pub fn appimages(app: &CraftApp) -> Vec<(PathBuf, Option<version::Version>)> {
        let Some(dir) = appimage_dir() else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
        let stems: Vec<&str> = app.stems().collect();
        let mut out: Vec<(PathBuf, Option<version::Version>)> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let parsed = parse_asset_name(&name, &stems)?;
                (parsed.kind == artcraft_launcher_core::PackageKind::AppImage).then(|| (e.path(), version::parse(&parsed.version)))
            })
            .collect();
        out.sort_by(|a, b| match (&b.1, &a.1) {
            (Some(x), Some(y)) => version::compare(x, y),
            _ => std::cmp::Ordering::Equal,
        });
        out
    }

    pub fn detect(app: &CraftApp, snapshot: &Snapshot) -> Option<Installed> {
        if let Some((path, version)) = appimages(app).into_iter().next() {
            return Some(Installed { version, source: InstallSource::AppImage, location: path.display().to_string() });
        }
        snapshot.packages.iter().find(|(name, _, _)| name == app.id).map(|(name, ver, source)| Installed {
            version: version::parse(ver),
            source: *source,
            location: if *source == InstallSource::Flatpak { app.bundle_id() } else { format!("/usr/bin/{name}") },
        })
    }
}

#[cfg(target_os = "freebsd")]
pub mod freebsd {
    use std::path::Path;
    use std::time::Duration;

    use super::*;

    pub fn detect(app: &CraftApp) -> Option<Installed> {
        let bin = Path::new("/usr/local/bin").join(app.id);
        if !bin.exists() {
            return None;
        }
        Some(Installed { version: binary_version(&bin, app.id), source: InstallSource::Binary, location: bin.display().to_string() })
    }

    /// `<app> --version` prints `<app> 0.5.0 (sha, date)`.
    fn binary_version(bin: &Path, id: &str) -> Option<version::Version> {
        let out = crate::process::capture(bin.to_str()?, &["--version"], Duration::from_secs(5))?;
        let rest = out.trim().strip_prefix(id)?.trim_start();
        version::parse(rest.split_whitespace().next()?)
    }
}
