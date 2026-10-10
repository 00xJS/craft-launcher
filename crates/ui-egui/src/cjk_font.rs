//! A Chinese font from the operating system, for the Simplified Chinese interface. The bundled
//! faces have no CJK glyphs, and a CJK font is too large to ship (10 MB or more), so the desktop
//! build borrows the one every system that can show Chinese already has. Desktop only.

use std::path::{Path, PathBuf};

use egui::epaint::text::VariationCoords;
use egui::{FontData, FontTweak};
use skrifa::{FontRef, MetadataProvider, string::StringId};

/// Larger files aren't read: PingFang, the largest candidate, is about 80 MB.
const MAX_FONT_BYTES: u64 = 100 * 1024 * 1024;
/// Faces looked at in one collection file (PingFang has 24).
const MAX_FACES: u32 = 64;

/// A font file that may be installed, and the family of its Simplified Chinese face.
struct Candidate {
    path: PathBuf,
    family: &'static str,
}

/// The system's Simplified Chinese font, or `None` (logged) when it has none this knows about.
/// Reads the file (once per process: the caller caches the answer).
pub(crate) fn load() -> Option<FontData> {
    for Candidate { path, family } in candidates() {
        let Some(bytes) = read(&path) else { continue };
        let Some(index) = face_index(&bytes, family) else {
            log::debug!("no {family} face in {}", path.display());
            continue;
        };
        log::info!("Chinese text uses {family} from {} (face {index})", path.display());
        // Regular weight, in case the face is variable (Noto Sans CJK VF).
        let tweak = FontTweak { coords: VariationCoords::new([(*b"wght", 400.0)]), ..Default::default() };
        let mut font = FontData::from_owned(bytes).tweak(tweak);
        font.index = index;
        return Some(font);
    }
    log::warn!(
        "no Chinese font found on this system (looked for PingFang, Hiragino Sans GB, Heiti, Microsoft YaHei, SimSun, Noto Sans CJK and WenQuanYi)"
    );
    None
}

fn read(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    if meta.len() > MAX_FONT_BYTES {
        log::warn!("{} is too large to load as a font ({} bytes)", path.display(), meta.len());
        return None;
    }
    std::fs::read(path).map_err(|e| log::warn!("couldn't read {}: {e}", path.display())).ok()
}

/// The face of `family` in a font file or collection: its Regular face, else the first face of
/// that family. Matches the (typographic) family name in any language the file gives it in.
pub(crate) fn face_index(data: &[u8], family: &str) -> Option<u32> {
    let names = |font: &FontRef<'_>, ids: [StringId; 2]| -> Vec<String> {
        ids.into_iter().flat_map(|id| font.localized_strings(id).map(|s| s.to_string())).collect()
    };
    let mut first = None;
    for index in 0..MAX_FACES {
        let Ok(font) = FontRef::from_index(data, index) else { break };
        let families = names(&font, [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]);
        if !families.iter().any(|f| f.eq_ignore_ascii_case(family)) {
            continue;
        }
        let styles = names(&font, [StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME]);
        if styles.iter().any(|s| s.eq_ignore_ascii_case("Regular")) {
            return Some(index);
        }
        first.get_or_insert(index);
    }
    first
}

/// Where each system keeps a Simplified Chinese font, best first.
fn candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut add = |path: PathBuf, family: &'static str| out.push(Candidate { path, family });
    #[cfg(target_os = "macos")]
    {
        // PingFang moved from the system fonts into a downloaded asset in later macOS versions.
        add("/System/Library/Fonts/PingFang.ttc".into(), "PingFang SC");
        for path in mac_font_assets("PingFang.ttc") {
            add(path, "PingFang SC");
        }
        add("/System/Library/Fonts/Hiragino Sans GB.ttc".into(), "Hiragino Sans GB W3");
        add("/System/Library/Fonts/STHeiti Medium.ttc".into(), "Heiti SC");
    }
    #[cfg(target_os = "windows")]
    {
        let root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        add(root.join("Fonts").join("msyh.ttc"), "Microsoft YaHei");
        add(root.join("Fonts").join("simsun.ttc"), "SimSun");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Debian/Ubuntu, Arch, Fedora (static and variable) and FreeBSD's ports.
        const NOTO_DIRS: [&str; 8] = [
            "/usr/share/fonts/opentype/noto",
            "/usr/share/fonts/noto-cjk",
            "/usr/share/fonts/google-noto-cjk",
            "/usr/share/fonts/google-noto-sans-cjk-fonts",
            "/usr/share/fonts/google-noto-sans-cjk-vf-fonts",
            "/usr/share/fonts/noto",
            "/usr/local/share/fonts/noto",
            "/usr/local/share/fonts/noto-cjk",
        ];
        for file in ["NotoSansCJK-Regular.ttc", "NotoSansCJKsc-Regular.otf", "NotoSansCJK-VF.ttc"] {
            for dir in NOTO_DIRS {
                add(Path::new(dir).join(file), "Noto Sans CJK SC");
            }
        }
        const WQY_DIRS: [&str; 5] = [
            "/usr/share/fonts/truetype/wqy",
            "/usr/share/fonts/wenquanyi/wqy-microhei",
            "/usr/share/fonts/wqy-microhei",
            "/usr/share/fonts/wqy-zenhei",
            "/usr/local/share/fonts/wqy",
        ];
        for (file, family) in [("wqy-microhei.ttc", "WenQuanYi Micro Hei"), ("wqy-zenhei.ttc", "WenQuanYi Zen Hei")] {
            for dir in WQY_DIRS {
                add(Path::new(dir).join(file), family);
            }
        }
    }
    out
}

/// `file` in macOS's downloaded font assets:
/// `/System/Library/AssetsV2/com_apple_MobileAsset_Font*/*.asset/AssetData/<file>`.
#[cfg(target_os = "macos")]
fn mac_font_assets(file: &str) -> Vec<PathBuf> {
    let dirs = |path: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(path).map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect()).unwrap_or_default()
    };
    let mut out = Vec::new();
    for catalog in dirs(Path::new("/System/Library/AssetsV2")) {
        if !catalog.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("com_apple_MobileAsset_Font")) {
            continue;
        }
        for asset in dirs(&catalog) {
            let path = asset.join("AssetData").join(file);
            if asset.extension().is_some_and(|e| e == "asset") && path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_index_rejects_what_isnt_a_font() {
        assert_eq!(face_index(b"", "PingFang SC"), None);
        assert_eq!(face_index(b"ttcf\0\0\0\0garbage", "PingFang SC"), None);
        assert_eq!(face_index(&[0u8; 64], "PingFang SC"), None);
    }

    #[test]
    fn face_index_finds_families_in_real_fonts() {
        // The bundled Latin faces stand in for system fonts the test machine may not have.
        let inter: &[u8] = include_bytes!("../../../assets/fonts/Inter-Variable.ttf");
        assert_eq!(face_index(inter, "Inter"), Some(0));
        assert_eq!(face_index(inter, "PingFang SC"), None);
    }

    /// Whatever this machine has loads without panicking, and a found face really is the family's.
    #[test]
    fn system_font_loads_when_present() {
        if let Some(font) = load() {
            let family = candidates().into_iter().find(|c| c.path.is_file()).map(|c| c.family);
            assert!(family.is_some());
            assert!(FontRef::from_index(&font.font, font.index).is_ok());
        }
    }
}
