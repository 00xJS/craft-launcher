//! The Craft App Suite: the twelve apps the launcher knows about.
//!
//! The list is data so a later version can fetch it from an API; [`SUITE`] is the built-in copy.

use serde::{Deserialize, Serialize};

/// Which generation of the suite an app belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppGroup {
    /// The seven original apps, each with a page on getartcraft.com.
    Classic,
    /// The five apps added after them.
    New,
}

impl AppGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::New => "New",
        }
    }
}

/// One app in the suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CraftApp {
    /// Machine name: repo, binary, package and asset stem (`photocraft`).
    pub id: &'static str,
    /// Display name (`PhotoCraft`).
    pub name: &'static str,
    /// What it's for, in a few words.
    pub tagline: &'static str,
    /// The familiar app it reimplements, for orientation.
    pub like: &'static str,
    pub group: AppGroup,
    /// Brand colour (sRGB).
    pub color: [u8; 3],
    /// GitHub `owner/repo`.
    pub repo: &'static str,
    /// Older names its release assets may carry (PdfCraft shipped as PrintCraft until 0.2.1).
    pub former_ids: &'static [&'static str],
}

impl CraftApp {
    /// The product it's inspired by, with its registered-trademark sign (`Adobe Photoshop®`).
    pub fn inspired_by(&self) -> String {
        format!("{}®", self.like)
    }

    /// `ai.storyteller.<id>`: macOS bundle id, Flatpak id and Linux desktop-file id.
    pub fn bundle_id(&self) -> String {
        format!("ai.storyteller.{}", self.id)
    }

    pub fn releases_url(&self) -> String {
        format!("https://github.com/{}/releases", self.repo)
    }

    pub fn repo_url(&self) -> String {
        format!("https://github.com/{}", self.repo)
    }

    /// The app's page on getartcraft.com (classic apps only).
    pub fn website(&self) -> Option<String> {
        (self.group == AppGroup::Classic).then(|| format!("https://getartcraft.com/apps/{}", self.id))
    }

    /// Asset stems this app's releases may use, current first.
    pub fn stems(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.id).chain(self.former_ids.iter().copied())
    }

    pub fn by_id(id: &str) -> Option<&'static CraftApp> {
        SUITE.iter().find(|a| a.id == id)
    }
}

const fn app(
    id: &'static str,
    name: &'static str,
    tagline: &'static str,
    like: &'static str,
    group: AppGroup,
    color: [u8; 3],
    repo: &'static str,
) -> CraftApp {
    CraftApp { id, name, tagline, like, group, color, repo, former_ids: &[] }
}

/// The Craft App Suite, in display order: the seven classic apps, then the five new ones.
pub const SUITE: [CraftApp; 12] = [
    app("photocraft", "PhotoCraft", "Image editing", "Adobe Photoshop", AppGroup::Classic, [0x2f, 0x7b, 0xf5], "storytold/photocraft"),
    app("vectorcraft", "VectorCraft", "Vector illustration", "Adobe Illustrator", AppGroup::Classic, [0xe8, 0x57, 0x3f], "storytold/vectorcraft"),
    app(
        "filmcraft",
        "FilmCraft",
        "Video editing, color and sound",
        "Adobe Premiere Pro",
        AppGroup::Classic,
        [0x8b, 0x5c, 0xf6],
        "storytold/filmcraft",
    ),
    app(
        "lightcraft",
        "LightCraft",
        "Photo library and raw development",
        "Adobe Lightroom",
        AppGroup::Classic,
        [0xf2, 0xa5, 0x16],
        "storytold/lightcraft",
    ),
    CraftApp {
        former_ids: &["printcraft"],
        ..app(
            "pdfcraft",
            "PdfCraft",
            "Reading, organizing and protecting PDFs",
            "Adobe Acrobat",
            AppGroup::Classic,
            [0x12, 0xa5, 0x8a],
            "storytold/pdfcraft",
        )
    },
    app(
        "effectcraft",
        "EffectCraft",
        "Motion graphics and visual effects",
        "Adobe After Effects",
        AppGroup::Classic,
        [0xe0, 0x36, 0x8f],
        "storytold/effectcraft",
    ),
    app("designcraft", "DesignCraft", "Page layout and publishing", "Adobe InDesign", AppGroup::Classic, [0x7b, 0xb5, 0x1c], "storytold/designcraft"),
    app("wordcraft", "WordCraft", "Writing and document design", "Microsoft Word", AppGroup::New, [0x3f, 0x5f, 0xe0], "storytold/wordcraft"),
    app("deckcraft", "DeckCraft", "Presentations and slide shows", "Microsoft PowerPoint", AppGroup::New, [0xf0, 0x7a, 0x1e], "storytold/deckcraft"),
    app("gridcraft", "GridCraft", "Spreadsheets and data", "Microsoft Excel", AppGroup::New, [0x1f, 0x9d, 0x55], "storytold/gridcraft"),
    app("cadcraft", "CADCraft", "Computer-aided design and drafting", "Autodesk AutoCAD", AppGroup::New, [0x3e, 0xa6, 0xe8], "storytold/cadcraft"),
    app(
        "soundcraft",
        "SoundCraft",
        "Recording, editing and mixing audio",
        "Avid Pro Tools",
        AppGroup::New,
        [0x1f, 0xa5, 0xc9],
        "storytold/soundcraft",
    ),
];

/// Shown wherever the products the apps are inspired by are named.
pub const TRADEMARK_NOTICE: &str = "Adobe, Photoshop, Illustrator, Premiere Pro, Lightroom, Acrobat, After Effects and InDesign are either \
registered trademarks or trademarks of Adobe in the United States and/or other countries. Microsoft, Word, PowerPoint and Excel are \
trademarks of the Microsoft group of companies. Autodesk and AutoCAD are registered trademarks or trademarks of Autodesk, Inc. Avid \
and Pro Tools are trademarks or registered trademarks of Avid Technology, Inc. All other trademarks are the property of their \
respective owners. These names are used only to describe what each Craft App is for; the Craft App Suite is independent and is not \
affiliated with, sponsored by or endorsed by any of these companies.";

/// ArtCraft's website.
pub const WEBSITE_URL: &str = "https://getartcraft.com";
/// ArtCraft on GitHub: every Craft App's source.
pub const GITHUB_URL: &str = "https://github.com/storytold";
/// ArtCraft's social accounts: (service, URL). Shown as small icons, less prominent than Discord.
pub const SOCIAL: [(&str, &str); 4] = [
    ("X", "https://x.com/get_artcraft"),
    ("YouTube", "https://www.youtube.com/@OfficialArtcraftStudios"),
    ("TikTok", "https://www.tiktok.com/@artcraft.studios"),
    ("Instagram", "https://www.instagram.com/officialartcraftstudios"),
];

/// The ArtCraft community on Discord.
pub const DISCORD_URL: &str = "https://discord.gg/artcraft";

/// The launcher's own repo, checked so it can say when a newer launcher is out.
pub const LAUNCHER_REPO: &str = "storytold/craft-launcher";
/// The launcher's own asset stem.
pub const LAUNCHER_ID: &str = "artcraft-launcher";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_has_seven_classic_and_five_new_apps_with_unique_ids() {
        assert_eq!(SUITE.iter().filter(|a| a.group == AppGroup::Classic).count(), 7);
        assert_eq!(SUITE.iter().filter(|a| a.group == AppGroup::New).count(), 5);
        let mut ids: Vec<_> = SUITE.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SUITE.len());
        for a in &SUITE {
            assert_eq!(a.repo, format!("storytold/{}", a.id));
            assert_eq!(a.name.to_lowercase(), a.id);
        }
    }

    #[test]
    fn every_inspiring_product_is_covered_by_the_trademark_notice() {
        for a in &SUITE {
            assert!(a.inspired_by().ends_with('®'));
            // The company and the product name both appear in the notice.
            let (company, product) = a.like.split_once(' ').unwrap();
            assert!(TRADEMARK_NOTICE.contains(company), "{company}");
            assert!(TRADEMARK_NOTICE.contains(product), "{product}");
        }
    }

    #[test]
    fn pdfcraft_also_answers_to_printcraft() {
        let pdf = CraftApp::by_id("pdfcraft").unwrap();
        assert_eq!(pdf.stems().collect::<Vec<_>>(), ["pdfcraft", "printcraft"]);
        assert_eq!(pdf.website().as_deref(), Some("https://getartcraft.com/apps/pdfcraft"));
        assert_eq!(CraftApp::by_id("wordcraft").unwrap().website(), None);
    }
}
