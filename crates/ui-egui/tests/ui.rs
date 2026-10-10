//! Drives the launcher UI against a fake backend: what's shown for each status, and which
//! commands the buttons send. `ARTCRAFT_LAUNCHER_SNAPSHOTS=<dir>` also saves screenshots.

use std::cell::RefCell;
use std::rc::Rc;

use artcraft_launcher_core::protocol::{Backend, Capabilities, Command, Event, JobPhase, LatestRelease, PausedDownload, Source};
use artcraft_launcher_core::release::{Asset, Release};
use artcraft_launcher_core::status::InstallSource;
use artcraft_launcher_core::{Arch, Installed, Os, Platform, SUITE, version};
use artcraft_launcher_ui_egui::settings::ViewMode;
use artcraft_launcher_ui_egui::{Host, LauncherUi, Settings};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const NOW: i64 = 1_791_500_000;

struct Fake {
    events: Vec<Event>,
    sent: Rc<RefCell<Vec<Command>>>,
    manage: bool,
}

impl Backend for Fake {
    fn send(&mut self, command: Command) {
        self.sent.borrow_mut().push(command);
    }
    fn poll(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { manage_apps: self.manage, reveal_label: "Show in Finder" }
    }
    fn platform(&self) -> Option<Platform> {
        Some(Platform { os: Os::MacOs, arch: Arch::Arm64 })
    }
    fn now(&self) -> i64 {
        NOW
    }
}

struct NoHost;
impl Host for NoHost {}

fn release(id: &str, v: &str, at: i64) -> Release {
    let name = format!("{id}-{v}-macos-universal.dmg");
    Release {
        tag: format!("v{v}"),
        version: version::parse(v),
        name: format!("v{v}"),
        published_at: Some(at),
        html_url: format!("https://github.com/storytold/{id}/releases/tag/v{v}"),
        notes: "## What's Changed\n* Faster things by @a in https://github.com/storytold/x/pull/1".into(),
        prerelease: false,
        assets: vec![Asset {
            name: name.clone(),
            size: 150 << 20,
            url: format!("https://github.com/storytold/{id}/releases/download/v{v}/{name}"),
            sha256: None,
        }],
    }
}

/// A realistic mix: updates, up-to-date apps, a download in progress, one app with no release.
fn events() -> Vec<Event> {
    let mut ev = Vec::new();
    let versions = [
        ("photocraft", "0.5.0"),
        ("vectorcraft", "0.7.0"),
        ("filmcraft", "0.4.0"),
        ("lightcraft", "0.4.0"),
        ("pdfcraft", "0.4.0"),
        ("effectcraft", "0.6.0"),
        ("designcraft", "0.4.0"),
        ("wordcraft", "0.3.0"),
        ("deckcraft", "0.3.0"),
        ("gridcraft", "0.3.0"),
        ("cadcraft", "0.3.0"),
    ];
    for (i, (id, v)) in versions.iter().enumerate() {
        ev.push(Event::Latest { app: (*id).into(), latest: LatestRelease::Found(release(id, v, NOW - 86_400 - i as i64 * 600)) });
    }
    ev.push(Event::Latest { app: "soundcraft".into(), latest: LatestRelease::None });
    let installed = [("photocraft", "0.4.1"), ("lightcraft", "0.4.0"), ("wordcraft", "0.3.0"), ("effectcraft", "0.5.0")];
    for app in &SUITE {
        let inst = installed.iter().find(|(id, _)| *id == app.id).map(|(_, v)| Installed {
            version: version::parse(v),
            source: InstallSource::MacApp,
            location: format!("/Applications/{}.app", app.name),
        });
        ev.push(Event::Detected { app: app.id.into(), installed: inst });
    }
    ev.push(Event::Progress { app: "filmcraft".into(), phase: JobPhase::Downloading { done: 48 << 20, total: 131 << 20 } });
    ev.push(Event::Paused { app: "deckcraft".into(), download: Some(PausedDownload { done: 30 << 20, total: 120 << 20 }) });
    ev.push(Event::Progress { app: "gridcraft".into(), phase: JobPhase::Reconnecting { done: 10 << 20, total: 90 << 20 } });
    ev.push(Event::CheckFinished { at: NOW - 180, source: Source::GitHubApi, rate_limited_until: None });
    ev
}

fn harness(view: ViewMode, theme: egui::Theme, manage: bool) -> (Harness<'static, Option<LauncherUi>>, Rc<RefCell<Vec<Command>>>) {
    let sent = Rc::new(RefCell::new(Vec::new()));
    let sent2 = sent.clone();
    let mut backend = Some(Fake { events: events(), sent: sent2, manage });
    let settings = Settings { view, notify: false, ..Settings::default() };
    let harness = Harness::builder().with_size(egui::vec2(1180.0, 900.0)).with_theme(theme).wgpu().build_ui_state(
        move |ui, state: &mut Option<LauncherUi>| {
            if state.is_none()
                && let Some(b) = backend.take()
            {
                let mut s = settings.clone();
                s.theme = if theme == egui::Theme::Dark {
                    artcraft_launcher_ui_egui::theme::ThemeChoice::Dark
                } else {
                    artcraft_launcher_ui_egui::theme::ThemeChoice::Light
                };
                *state = Some(LauncherUi::new(ui.ctx(), Box::new(b), s));
            }
            if let Some(l) = state.as_mut() {
                l.logic(&ui.ctx().clone(), &mut NoHost);
                l.ui(ui, &mut NoHost);
            }
        },
        None,
    );
    (harness, sent)
}

fn snapshot(h: &mut Harness<'_, Option<LauncherUi>>, name: &str) {
    let Some(dir) = std::env::var_os("ARTCRAFT_LAUNCHER_SNAPSHOTS") else { return };
    let img = h.render().unwrap();
    img.save(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
}

#[test]
fn grid_shows_every_status_and_installs_on_click() {
    let (mut h, sent) = harness(ViewMode::Grid, egui::Theme::Dark, true);
    h.run_steps(8);
    snapshot(&mut h, "grid-dark");
    // Statuses the user asked for: available, installed and up to date, update with version.
    assert!(h.query_all_by_label("Available to install").count() >= 3);
    assert!(h.query_all_by_label("Installed · up to date").count() >= 2);
    h.get_by_label("Update to 0.5.0");
    h.get_by_label("Update to 0.6.0");
    h.get_by_label_contains("0.4.1 installed → 0.5.0 released");
    assert!(h.query_all_by_label("Coming soon").count() >= 1);
    h.get_by_label_contains("48 MB of 131 MB");
    h.get_by_label("Update all (2)").click();
    h.run_steps(2);
    let installs: Vec<_> = sent.borrow().iter().filter_map(|c| if let Command::Install { app } = c { Some(app.clone()) } else { None }).collect();
    assert_eq!(installs.len(), 2, "{installs:?}");
    assert!(installs.contains(&"photocraft".to_string()) && installs.contains(&"effectcraft".to_string()));
}

#[test]
fn downloads_can_be_paused_resumed_and_discarded() {
    let (mut h, sent) = harness(ViewMode::Grid, egui::Theme::Dark, true);
    h.run_steps(4);
    // Bring the paused and reconnecting apps on screen.
    if let Some(l) = h.state_mut().as_mut() {
        l.settings.filter = artcraft_launcher_ui_egui::settings::Filter::NotInstalled;
    }
    h.run_steps(6);
    snapshot(&mut h, "grid-downloads-dark");
    h.get_by_label("Download paused");
    h.get_by_label("Reconnecting…");
    h.get_by_label_contains("· 25%").click();
    h.run_steps(2);
    assert!(sent.borrow().contains(&Command::Install { app: "deckcraft".into() }));
    h.get_by_label("Discard download").click();
    h.run_steps(2);
    assert!(sent.borrow().contains(&Command::Cancel { app: "deckcraft".into() }));
    h.get_all_by_label("Pause download").next().unwrap().click();
    h.run_steps(2);
    assert!(sent.borrow().iter().any(|c| matches!(c, Command::Pause { .. })));
}

#[test]
fn table_view_and_light_theme() {
    let (mut h, sent) = harness(ViewMode::Table, egui::Theme::Light, true);
    h.run_steps(8);
    snapshot(&mut h, "table-light");
    h.get_by_label("Update to 0.5.0").click();
    h.run_steps(2);
    assert!(sent.borrow().contains(&Command::Install { app: "photocraft".into() }));
    h.get_all_by_label("Open").next().unwrap().click();
    h.run_steps(2);
    assert!(sent.borrow().iter().any(|c| matches!(c, Command::Launch { .. })));
}

#[test]
fn switching_views_and_filters() {
    let (mut h, _) = harness(ViewMode::Grid, egui::Theme::Dark, true);
    h.run_steps(6);
    h.get_by_label_contains("Table").click();
    h.run_steps(4);
    assert_eq!(h.state().as_ref().unwrap().settings.view, ViewMode::Table);
    h.get_by_label_contains("Updates").click();
    h.run_steps(4);
    snapshot(&mut h, "table-updates-dark");
    assert!(h.query_by_label("WordCraft").is_none());
    h.get_by_label("PhotoCraft");
}

#[test]
fn web_build_offers_downloads() {
    let (mut h, _) = harness(ViewMode::Grid, egui::Theme::Dark, false);
    h.run_steps(8);
    snapshot(&mut h, "web-grid-dark");
    assert!(h.query_all_by_label("Download for macOS").count() >= 8);
    assert!(h.query_by_label("Install").is_none());
}
