//! The header, toolbar, footer, the grid of cards and the table.

use artcraft_launcher_core::platform::pick_asset;
use artcraft_launcher_core::protocol::{Command, JobPhase};
use artcraft_launcher_core::status::AppStatus;
use artcraft_launcher_core::{AppGroup, time};
use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{Dialog, LauncherUi};
use crate::model::{self, AppEntry};
use crate::settings::{Filter, SortKey, ViewMode};
use crate::theme::{self, Tokens, rgb, tint};
use crate::widgets;

/// What the main button of an app does.
enum Primary {
    Install,
    Update(String),
    Open,
    Download(String),
    Unavailable(&'static str),
}

/// ArtCraft's tagline, under the launcher's name in the header.
const TAGLINE: &str = "Open Source Tools for Creatives, Makers, and Builders";

const CARD_MIN_W: f32 = 300.0;
const CARD_H: f32 = 210.0;
const GAP: f32 = 14.0;

impl LauncherUi {
    pub(crate) fn header(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            // The ArtCraft wordmark exactly as supplied, then the launcher's own name beside it.
            // The logo and divider sit centred on the two-line name block beside them.
            const ROW_H: f32 = 42.0;
            let logo_size = vec2(24.0 * crate::icons::ARTCRAFT_LOGO_ASPECT, 24.0);
            let (row, _) = ui.allocate_exact_size(vec2(logo_size.x, ROW_H), Sense::hover());
            egui::Image::new(crate::icons::artcraft_logo(t.dark))
                .alt_text("ArtCraft")
                .paint_at(ui, egui::Rect::from_center_size(row.center(), logo_size));
            ui.add_space(10.0);
            let (rule, _) = ui.allocate_exact_size(vec2(1.0, ROW_H), Sense::hover());
            ui.painter().rect_filled(egui::Rect::from_center_size(rule.center(), vec2(1.0, 30.0)), CornerRadius::ZERO, t.border_strong);
            ui.add_space(10.0);
            // Leave room for the controls on the right; the tagline gives way first.
            let room = (ui.available_width() - 300.0).max(120.0);
            ui.vertical(|ui| {
                ui.set_max_width(room);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("Launcher").size(17.0).family(theme::semibold()).color(t.text));
                    ui.add(egui::Label::new(RichText::new("  ·  Craft App Suite").size(14.0).color(t.text_muted)).truncate());
                });
                ui.add(egui::Label::new(RichText::new(TAGLINE).size(12.0).color(t.text_faint)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, "⚙", "Settings").clicked() {
                    self.dialog = Some(Dialog::Settings);
                }
                let refresh = ui.add_enabled_ui(!self.checking, |ui| widgets::icon_button(ui, "⟳", "Check for updates")).inner;
                if refresh.clicked() {
                    self.check_now();
                }
                if self.checking {
                    ui.add(egui::Spinner::new().size(16.0).color(t.text_muted));
                }
                ui.add_space(8.0);
                widgets::segmented(
                    ui,
                    &mut self.settings.view,
                    &[(ViewMode::Grid, "⊞  Grid", "Show apps as cards"), (ViewMode::Table, "☰  Table", "Show apps as a table")],
                );
                ui.add_space(8.0);
                if widgets::discord_button(ui).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(artcraft_launcher_core::catalog::DISCORD_URL));
                }
            });
        });
    }

    pub(crate) fn toolbar(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        if let Some(r) = self.launcher_update.clone() {
            egui::Frame::new()
                .fill(tint(t.accent, if t.dark { 0.14 } else { 0.08 }))
                .stroke(Stroke::new(1.0, tint(t.accent, 0.45)))
                .corner_radius(theme::RADIUS)
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("ArtCraft Launcher {} is available.", r.version_label())).family(theme::medium()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, "×", "Dismiss").clicked() {
                                self.launcher_update = None;
                            }
                            let url = self.platform.and_then(|p| launcher_download(&r, p)).unwrap_or_else(|| r.html_url.clone());
                            if widgets::primary(ui, "Download", t.accent, 0.0).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                            if ui.link("What's new").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(r.html_url.clone()));
                            }
                        });
                    });
                });
            ui.add_space(10.0);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let updates = self.count(Filter::Updates);
            for (filter, label, highlight) in [
                (Filter::All, "All", None),
                (Filter::Installed, "Installed", None),
                (Filter::Updates, "Updates", Some(t.warn)),
                (Filter::NotInstalled, "Not installed", None),
            ] {
                if !self.caps.manage_apps && filter != Filter::All {
                    continue;
                }
                let n = self.count(filter);
                if widgets::chip(ui, self.settings.filter == filter, label, n, highlight).clicked() {
                    self.settings.filter = filter;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.caps.manage_apps && updates > 0 {
                    let label = if updates == 1 { "Update 1 app".to_string() } else { format!("Update all ({updates})") };
                    if widgets::primary(ui, &label, t.accent, 0.0).clicked() {
                        let ids: Vec<&str> = self
                            .entries
                            .iter()
                            .filter(|e| e.status(self.platform, true) == AppStatus::UpdateAvailable && e.job.is_none())
                            .map(|e| e.app.id)
                            .collect();
                        for id in ids {
                            self.send(Command::Install { app: id.to_string() });
                        }
                    }
                    ui.add_space(4.0);
                }
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔍  Search apps").desired_width(200.0).margin(vec2(10.0, 6.0)));
                if self.settings.view == ViewMode::Grid {
                    egui::ComboBox::from_id_salt("sort")
                        .selected_text(RichText::new(sort_label(self.settings.sort)).size(13.0))
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for key in [
                                SortKey::Suite,
                                SortKey::Name,
                                SortKey::InspiredBy,
                                SortKey::Status,
                                SortKey::Installed,
                                SortKey::Latest,
                                SortKey::Released,
                            ] {
                                if ui.selectable_label(self.settings.sort == key, sort_label(key)).clicked() {
                                    self.settings.sort = key;
                                    self.settings.sort_descending = key.starts_descending();
                                }
                            }
                        });
                }
            });
        });
    }

    pub(crate) fn footer(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let now = self.backend.now();
        ui.horizontal(|ui| {
            let small = |s: String| RichText::new(s).size(12.0).color(t.text_muted);
            if self.checking {
                ui.label(small("Checking for updates…".into()));
            } else if let Some(at) = self.settings.last_check {
                let via = self.last_source.map(|s| format!(" · {}", s.label())).unwrap_or_default();
                ui.label(small(format!("Checked {}{via}", time::format_ago(at, now))));
            } else {
                ui.label(small("Not checked yet".into()));
            }
            if let Some(until) = self.rate_limited_until.filter(|u| *u > now) {
                ui.label(
                    RichText::new(format!("· GitHub's hourly limit was reached; full details return in {} min", ((until - now) / 60).max(1)))
                        .size(12.0)
                        .color(t.warn),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match self.platform {
                    Some(p) if self.caps.manage_apps => format!("Installing for {}", p.label()),
                    Some(p) => format!("Downloads for {}", p.label()),
                    None => "No downloads for this system".into(),
                };
                ui.label(small(label));
                ui.add_space(14.0);
                // Right to left, so this reads: Website · GitHub · Discord, then the social icons.
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    widgets::social_icons(ui, 13.0);
                });
                ui.add_space(8.0);
                use artcraft_launcher_core::catalog::{DISCORD_URL, GITHUB_URL, WEBSITE_URL};
                widgets::small_link(ui, "Discord", DISCORD_URL);
                ui.label(RichText::new("·").size(11.5).color(t.text_faint));
                widgets::small_link(ui, "GitHub", GITHUB_URL);
                ui.label(RichText::new("·").size(11.5).color(t.text_faint));
                widgets::small_link(ui, "Website", WEBSITE_URL);
            });
        });
    }

    // ---- grid ----------------------------------------------------------------------------

    pub(crate) fn grid(&mut self, ui: &mut Ui, ids: &[&'static str]) {
        let width = ui.available_width();
        let cols = (((width + GAP) / (CARD_MIN_W + GAP)).floor() as usize).max(1);
        let card_w = ((width - GAP * (cols as f32 - 1.0)) / cols as f32).floor();
        // In suite order, split into the classic apps and the new ones.
        let grouped = self.settings.sort == SortKey::Suite && self.settings.filter == crate::settings::Filter::All && self.search.trim().is_empty();
        let sections: Vec<(Option<&str>, Vec<&'static str>)> = if grouped {
            [AppGroup::Classic, AppGroup::New]
                .iter()
                .map(|g| {
                    let list = ids.iter().copied().filter(|id| self.entry(id).is_some_and(|e| e.app.group == *g)).collect();
                    (Some(if *g == AppGroup::Classic { "Classic apps" } else { "New apps" }), list)
                })
                .collect()
        } else {
            vec![(None, ids.to_vec())]
        };
        let t = Tokens::get(ui.ctx());
        for (title, list) in sections {
            if list.is_empty() {
                continue;
            }
            if let Some(title) = title {
                ui.add_space(6.0);
                ui.label(RichText::new(title.to_uppercase()).size(11.5).family(theme::semibold()).color(t.text_faint));
                ui.add_space(4.0);
            }
            for row in list.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for id in row {
                        self.card(ui, id, card_w);
                    }
                });
                ui.add_space(GAP);
            }
        }
    }

    fn card(&mut self, ui: &mut Ui, id: &str, width: f32) {
        let Some(entry) = self.entry(id).cloned() else { return };
        let t = Tokens::get(ui.ctx());
        let (rect, resp) = ui.allocate_exact_size(vec2(width, CARD_H), Sense::hover());
        let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
        let app_color = rgb(entry.app.color);
        let painter = ui.painter_at(rect.expand(12.0));
        if hovered {
            painter.add(
                egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.dark { 110 } else { 28 }) }
                    .as_shape(rect, CornerRadius::same(theme::RADIUS)),
            );
        }
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS),
            if hovered { t.card_hover } else { t.card },
            Stroke::new(1.0, if hovered { t.border_strong } else { t.border }),
            egui::StrokeKind::Inside,
        );
        // A soft wash of the app's colour from the top edge.
        gradient_top(&painter, rect.shrink(1.0), theme::RADIUS as f32 - 1.0, 74.0, app_color, if t.dark { 0.16 } else { 0.10 });

        let status = entry.status(self.platform, self.caps.manage_apps);
        let inner = rect.shrink2(vec2(16.0, 14.0));
        // A detached child: the card's contents never move the next card along.
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        // The app's own pages, top right: its website (classic apps) and its GitHub repo.
        {
            let corner = egui::Rect::from_min_max(egui::pos2(inner.right() - 130.0, inner.top()), egui::pos2(inner.right(), inner.top() + 18.0));
            let mut links = ui.new_child(egui::UiBuilder::new().max_rect(corner).layout(egui::Layout::right_to_left(egui::Align::Min)));
            links.spacing_mut().item_spacing.x = 10.0;
            widgets::small_link(&mut links, "GitHub", &entry.app.repo_url());
            if let Some(site) = entry.app.website() {
                widgets::small_link(&mut links, "Website", &site);
            }
        }
        {
            let ui = &mut child;
            ui.horizontal(|ui| {
                if let Some(icon) = crate::icons::app(entry.app.id) {
                    ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(54.0, 54.0)));
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(entry.app.name).size(17.0).family(theme::semibold()).color(t.text));
                        if entry.app.group == AppGroup::New {
                            ui.label(RichText::new("NEW").size(9.5).family(theme::semibold()).color(app_color));
                        }
                    });
                    ui.add(egui::Label::new(RichText::new(entry.app.tagline).size(12.5).color(t.text_muted)).truncate());
                    ui.add(egui::Label::new(RichText::new(format!("Like {}", entry.app.inspired_by())).size(11.5).color(t.text_faint)).truncate());
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let (text, tone) = model::entry_status_text(&entry, &status, self.caps.manage_apps);
                let tip = entry.failure().map(|f| format!("Couldn't check for updates: {f}"));
                let r = widgets::pill(ui, text, tone);
                if let Some(tip) = tip {
                    r.on_hover_text(tip);
                }
            });
            ui.add_space(2.0);
            ui.label(RichText::new(version_line(&entry, &status, self.backend.now())).size(12.0).color(t.text_muted));
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.horizontal(|ui| self.actions(ui, &entry, &status, inner.width()));
            });
        }
    }

    // ---- table ---------------------------------------------------------------------------

    pub(crate) fn table(&mut self, ui: &mut Ui, ids: &[&'static str]) {
        use egui_extras::{Column, TableBuilder};
        let t = Tokens::get(ui.ctx());
        let now = self.backend.now();
        let mut sort_clicked = None;
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(theme::RADIUS)
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(false)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::remainder().at_least(200.0).clip(true))
                    .column(Column::exact(172.0).clip(true))
                    .column(Column::exact(176.0))
                    .column(Column::exact(84.0))
                    .column(Column::exact(84.0))
                    .column(Column::exact(104.0))
                    .column(Column::exact(206.0))
                    .header(34.0, |mut header| {
                        for (key, label) in [
                            (Some(SortKey::Name), "App"),
                            (Some(SortKey::InspiredBy), "Inspired by"),
                            (Some(SortKey::Status), "Status"),
                            (Some(SortKey::Installed), "Installed"),
                            (Some(SortKey::Latest), "Latest"),
                            (Some(SortKey::Released), "Released"),
                            (None, ""),
                        ] {
                            header.col(|ui| {
                                let Some(k) = key else { return };
                                let active = k == self.settings.sort;
                                let tip = if active { "Click to reverse the order" } else { "Click to sort by this column" };
                                // Measure first so the whole header cell (label + arrow) is the target.
                                let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
                                let hovered = resp.hovered();
                                let color = if active || hovered { t.text } else { t.text_faint };
                                let p = ui.painter();
                                let font = egui::FontId::new(11.0, theme::semibold());
                                let galley = p.layout_no_wrap(label.to_uppercase(), font.clone(), color);
                                let text_pos = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
                                let arrow_x = text_pos.x + galley.size().x + 5.0;
                                p.galley(text_pos, galley, color);
                                // ▲ / ▼ for the active column; a faint hint on hover.
                                let arrow = if active {
                                    Some(self.settings.sort_descending)
                                } else if hovered {
                                    Some(k.starts_descending())
                                } else {
                                    None
                                };
                                if let Some(down) = arrow {
                                    let c = egui::pos2(arrow_x + 4.0, rect.center().y);
                                    let (tip_y, base_y) = if down { (c.y + 3.0, c.y - 2.0) } else { (c.y - 3.0, c.y + 2.0) };
                                    let pts = vec![egui::pos2(c.x - 3.5, base_y), egui::pos2(c.x + 3.5, base_y), egui::pos2(c.x, tip_y)];
                                    let arrow_color = if active { t.accent } else { t.text_faint };
                                    p.add(egui::Shape::convex_polygon(pts, arrow_color, Stroke::NONE));
                                }
                                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
                                if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    sort_clicked = Some(k);
                                }
                            });
                        }
                    })
                    .body(|mut body| {
                        for id in ids {
                            let Some(entry) = self.entry(id).cloned() else { continue };
                            let status = entry.status(self.platform, self.caps.manage_apps);
                            body.row(58.0, |mut row| {
                                row.col(|ui| {
                                    if let Some(icon) = crate::icons::app(entry.app.id) {
                                        ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(36.0, 36.0)));
                                    }
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        ui.label(RichText::new(entry.app.name).size(14.5).family(theme::semibold()).color(t.text));
                                        ui.add(egui::Label::new(RichText::new(entry.app.tagline).size(12.0).color(t.text_muted)).truncate());
                                    });
                                });
                                row.col(|ui| {
                                    ui.add(egui::Label::new(RichText::new(entry.app.inspired_by()).size(13.0).color(t.text_muted)).truncate());
                                });
                                row.col(|ui| {
                                    let (text, tone) = model::entry_status_text(&entry, &status, self.caps.manage_apps);
                                    let r = widgets::pill(ui, text, tone);
                                    if let Some(f) = entry.failure() {
                                        r.on_hover_text(format!("Couldn't check for updates: {f}"));
                                    }
                                });
                                row.col(|ui| {
                                    ui.label(
                                        RichText::new(entry.installed_label().unwrap_or_else(|| "—".into())).color(if entry.installed.is_some() {
                                            t.text
                                        } else {
                                            t.text_faint
                                        }),
                                    );
                                });
                                row.col(|ui| {
                                    let c = if status == AppStatus::UpdateAvailable { t.warn } else { t.text };
                                    ui.label(RichText::new(entry.latest_label().unwrap_or_else(|| "—".into())).color(c));
                                });
                                row.col(|ui| {
                                    let when = entry.release().and_then(|r| r.published_at);
                                    let r = ui.label(RichText::new(when.map_or_else(|| "—".into(), time::format_date)).color(t.text_muted));
                                    if let Some(w) = when {
                                        r.on_hover_text(time::format_ago(w, now));
                                    }
                                });
                                row.col(|ui| self.actions(ui, &entry, &status, 196.0));
                            });
                        }
                    });
            });
        if let Some(k) = sort_clicked {
            if self.settings.sort == k {
                self.settings.sort_descending = !self.settings.sort_descending;
            } else {
                self.settings.sort = k;
                self.settings.sort_descending = k.starts_descending();
            }
        }
    }

    /// The trademark notice under the grid and the table.
    pub(crate) fn trademark_notice(&self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(artcraft_launcher_core::catalog::TRADEMARK_NOTICE).size(11.0).color(t.text_faint)).wrap());
    }

    // ---- actions -------------------------------------------------------------------------

    fn primary_action(&self, entry: &AppEntry, status: &AppStatus) -> Primary {
        if !self.caps.manage_apps {
            let platform = self.platform;
            let url = entry.release().zip(platform).and_then(|(r, p)| {
                let stems: Vec<&str> = entry.app.stems().collect();
                pick_asset(r, &stems, p, p.download_kinds()).map(|(a, _)| a.url.clone())
            });
            return match (status, url) {
                (AppStatus::NoRelease, _) => Primary::Unavailable("Coming soon"),
                (AppStatus::Checking, _) => Primary::Unavailable("Checking…"),
                (_, Some(url)) => Primary::Download(url),
                (_, None) => Primary::Unavailable("Not available"),
            };
        }
        match status {
            AppStatus::Available => Primary::Install,
            AppStatus::UpdateAvailable => Primary::Update(entry.latest_label().unwrap_or_default()),
            AppStatus::UpToDate | AppStatus::Installed => Primary::Open,
            AppStatus::Checking => Primary::Unavailable("Checking…"),
            AppStatus::NoRelease => Primary::Unavailable("Coming soon"),
            AppStatus::Unavailable => Primary::Unavailable(if entry.failure().is_some() { "Offline" } else { "Not available" }),
        }
    }

    /// The main button (or progress) and the ⋯ menu.
    fn actions(&mut self, ui: &mut Ui, entry: &AppEntry, status: &AppStatus, width: f32) {
        let t = Tokens::get(ui.ctx());
        let id = entry.app.id.to_string();
        let main_w = (width - 40.0).max(120.0);
        if let Some(phase) = entry.job {
            // Two small buttons (pause, cancel) beside the bar while downloading.
            let bar_w = (width - 88.0).max(100.0);
            let short = bar_w < 200.0;
            let amount = |done: u64, total: u64| {
                if total > 0 { format!("{} of {}", widgets::human_bytes(done), widgets::human_bytes(total)) } else { widgets::human_bytes(done) }
            };
            let (fraction, caption, color) = match phase {
                JobPhase::Downloading { done, total } => {
                    let f = (total > 0).then(|| done as f32 / total as f32);
                    let pct = f.map(|f| format!(" · {:.0}%", f * 100.0)).unwrap_or_default();
                    let caption = if bar_w < 150.0 {
                        match f {
                            Some(f) => format!("{:.0}% of {}", f * 100.0, widgets::human_bytes(total)),
                            None => widgets::human_bytes(done),
                        }
                    } else if short {
                        format!("{}{pct}", amount(done, total))
                    } else {
                        format!("Downloading {}{pct}", amount(done, total))
                    };
                    (f, caption, t.accent)
                }
                JobPhase::Reconnecting { done, total } => {
                    let f = (total > 0).then(|| done as f32 / total as f32);
                    (f, if short { "Reconnecting…".into() } else { format!("Reconnecting… {}", amount(done, total)) }, t.warn)
                }
                JobPhase::Verifying => (None, "Verifying checksum…".into(), t.accent),
                JobPhase::Installing => (None, "Installing…".into(), t.accent),
                JobPhase::Removing => (None, "Removing…".into(), t.accent),
            };
            let downloading = matches!(phase, JobPhase::Downloading { .. } | JobPhase::Reconnecting { .. });
            widgets::progress(ui, fraction, &caption, color, if downloading { bar_w } else { main_w });
            if downloading {
                if widgets::glyph_button(ui, widgets::Glyph::Pause, "Pause download").clicked() {
                    self.send(Command::Pause { app: id.clone() });
                }
                if widgets::glyph_button(ui, widgets::Glyph::Close, "Cancel download").clicked() {
                    self.send(Command::Cancel { app: id });
                }
            }
            return;
        }
        if let Some(p) = entry.paused
            && self.caps.manage_apps
        {
            let pct = if p.total > 0 { format!(" · {:.0}%", p.done as f32 / p.total as f32 * 100.0) } else { String::new() };
            // Resume, discard (32) and the menu (~40), with spacing, within `width`.
            let label = if width < 260.0 { format!("Resume{pct}") } else { format!("Resume download{pct}") };
            let resp = widgets::primary(ui, &label, t.accent, (width - 96.0).max(90.0));
            let tip = if p.total > 0 {
                format!("{} of {} downloaded", widgets::human_bytes(p.done), widgets::human_bytes(p.total))
            } else {
                format!("{} downloaded", widgets::human_bytes(p.done))
            };
            if resp.on_hover_text(tip).clicked() {
                self.send(Command::Install { app: id.clone() });
            }
            if widgets::glyph_button(ui, widgets::Glyph::Close, "Discard download").clicked() {
                self.send(Command::Cancel { app: id.clone() });
            }
            self.menu(ui, entry, status);
            return;
        }
        match self.primary_action(entry, status) {
            Primary::Install => {
                if widgets::primary(ui, "Install", t.accent, main_w).clicked() {
                    self.send(Command::Install { app: id.clone() });
                }
            }
            Primary::Update(v) => {
                if widgets::primary(ui, &format!("Update to {v}"), t.accent, main_w).clicked() {
                    self.send(Command::Install { app: id.clone() });
                }
            }
            Primary::Open => {
                if widgets::secondary(ui, "Open", main_w).clicked() {
                    self.send(Command::Launch { app: id.clone() });
                }
            }
            Primary::Download(url) => {
                let label = match self.platform {
                    Some(p) => format!("Download for {}", p.os.label()),
                    None => "Download".into(),
                };
                if widgets::primary(ui, &label, t.accent, main_w).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
            Primary::Unavailable(label) => {
                ui.add_enabled_ui(false, |ui| widgets::secondary(ui, label, main_w));
            }
        }
        self.menu(ui, entry, status);
    }

    fn menu(&mut self, ui: &mut Ui, entry: &AppEntry, status: &AppStatus) {
        let id = entry.app.id.to_string();
        let manage = self.caps.manage_apps;
        let reveal = self.caps.reveal_label;
        ui.menu_button(RichText::new("…").size(16.0), |ui| {
            ui.set_min_width(200.0);
            if entry.release().is_some() && ui.button("Release notes").clicked() {
                self.dialog = Some(Dialog::Notes(id.clone()));
                ui.close();
            }
            if manage && status.is_installed() {
                if ui.button("Open").clicked() {
                    self.send(Command::Launch { app: id.clone() });
                    ui.close();
                }
                if ui.button(reveal).clicked() {
                    self.send(Command::Reveal { app: id.clone() });
                    ui.close();
                }
                if *status == AppStatus::UpToDate && ui.button("Reinstall").clicked() {
                    self.send(Command::Install { app: id.clone() });
                    ui.close();
                }
                ui.separator();
                if ui.button(RichText::new("Uninstall…").color(Tokens::get(ui.ctx()).bad)).clicked() {
                    self.dialog = Some(Dialog::ConfirmUninstall(id.clone()));
                    ui.close();
                }
            }
            if !manage && let (Some(r), Some(p)) = (entry.release(), self.platform) {
                let stems: Vec<&str> = entry.app.stems().collect();
                let options: Vec<(String, String)> = p
                    .download_kinds()
                    .iter()
                    .filter_map(|k| {
                        pick_asset(r, &stems, p, &[*k]).map(|(a, _)| (format!("{} · {}", k.label(), widgets::human_bytes(a.size)), a.url.clone()))
                    })
                    .collect();
                if !options.is_empty() {
                    ui.separator();
                    for (label, url) in options {
                        if ui.button(label).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            ui.close();
                        }
                    }
                }
            }
            ui.separator();
            let link = |ui: &mut Ui, label: &str, url: String| {
                if ui.button(label).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    ui.close();
                }
            };
            match entry.app.website() {
                Some(site) => link(ui, "Website", site),
                None => {
                    ui.add_enabled(false, egui::Button::new("Website (coming soon)"));
                }
            }
            link(ui, "GitHub repository", entry.app.repo_url());
            link(ui, "All releases", entry.app.releases_url());
        });
    }
}

fn sort_label(k: SortKey) -> &'static str {
    match k {
        SortKey::Suite => "Suite order",
        SortKey::Name => "Name",
        SortKey::InspiredBy => "Inspired by",
        SortKey::Status => "Status",
        SortKey::Installed => "Installed version",
        SortKey::Latest => "Latest version",
        SortKey::Released => "Newest release",
    }
}

/// "Installed 0.4.0 · 0.5.0 out Oct 8, 2026" and friends.
fn version_line(e: &AppEntry, status: &AppStatus, now: i64) -> String {
    let released = e.release().and_then(|r| r.published_at).map(|at| format!(" · {}", time::format_date(at))).unwrap_or_default();
    let _ = now;
    match (status, e.installed_label(), e.latest_label()) {
        (AppStatus::UpdateAvailable, Some(i), Some(l)) => format!("{i} installed → {l} released{released}"),
        (AppStatus::UpToDate, Some(i), _) => format!("Version {i}{released}"),
        (_, Some(i), Some(l)) => format!("{i} installed · latest {l}"),
        (_, Some(i), None) => format!("Version {i} installed"),
        (_, None, Some(l)) => format!("Version {l}{released}"),
        (AppStatus::NoRelease, None, None) => "No release published yet".into(),
        _ => " ".into(),
    }
}

/// The launcher's own download for this platform.
fn launcher_download(r: &artcraft_launcher_core::Release, p: artcraft_launcher_core::Platform) -> Option<String> {
    use artcraft_launcher_core::PackageKind;
    let kinds: &[PackageKind] = match p.os {
        artcraft_launcher_core::Os::Linux => &[PackageKind::AppImage, PackageKind::Deb],
        _ => p.default_kinds(),
    };
    pick_asset(r, &[artcraft_launcher_core::catalog::LAUNCHER_ID], p, kinds).map(|(a, _)| a.url.clone())
}

/// Fills the top `height` of a rounded rect with opaque `color` at `alpha`, fading to transparent.
fn gradient_top(painter: &egui::Painter, rect: egui::Rect, radius: f32, height: f32, color: Color32, alpha: f32) {
    let mut mesh = egui::Mesh::default();
    let h = height.min(rect.height());
    let steps = 24;
    let mut prev: Option<(u32, u32)> = None;
    for i in 0..=steps {
        let y = h * i as f32 / steps as f32;
        // Inset along the rounded corner.
        let inset = if y < radius { radius - (radius * radius - (radius - y) * (radius - y)).max(0.0).sqrt() } else { 0.0 };
        let a = 1.0 - i as f32 / steps as f32;
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (255.0 * alpha * a * a) as u8);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(egui::pos2(rect.left() + inset, rect.top() + y), c);
        mesh.colored_vertex(egui::pos2(rect.right() - inset, rect.top() + y), c);
        if let Some((l, r)) = prev {
            mesh.add_triangle(l, r, base);
            mesh.add_triangle(r, base + 1, base);
        }
        prev = Some((base, base + 1));
    }
    painter.add(egui::Shape::mesh(mesh));
}
