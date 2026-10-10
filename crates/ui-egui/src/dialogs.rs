//! Modal dialogs: release notes, uninstall confirmation, settings.

use artcraft_launcher_core::protocol::Command;
use artcraft_launcher_core::{Os, PackageKind, time};
use egui::{RichText, vec2};

use crate::app::{Dialog, Host, LauncherUi};
use crate::settings::CheckEvery;
use crate::theme::{self, ThemeChoice, Tokens};
use crate::widgets;

impl LauncherUi {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context, host: &mut dyn Host) {
        let Some(dialog) = self.dialog.clone() else { return };
        let t = Tokens::get(ctx);
        let modal = egui::Modal::new(egui::Id::new("dialog")).frame(
            egui::Frame::new().fill(t.surface).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(theme::RADIUS + 4).inner_margin(22),
        );
        let mut close = false;
        let response = match &dialog {
            Dialog::Notes(id) => modal.show(ctx, |ui| close = self.notes_dialog(ui, id)),
            Dialog::ConfirmUninstall(id) => modal.show(ctx, |ui| close = self.uninstall_dialog(ui, id)),
            Dialog::Settings => modal.show(ctx, |ui| close = self.settings_dialog(ui, host)),
        };
        if close || response.should_close() {
            self.dialog = None;
        }
    }

    fn title(ui: &mut egui::Ui, icon: Option<egui::ImageSource<'static>>, title: &str, subtitle: &str) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            if let Some(icon) = icon {
                ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(40.0, 40.0)));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(title).size(18.0).family(theme::semibold()).color(t.text));
                if !subtitle.is_empty() {
                    ui.label(RichText::new(subtitle).size(12.5).color(t.text_muted));
                }
            });
        });
        ui.add_space(8.0);
    }

    fn notes_dialog(&mut self, ui: &mut egui::Ui, id: &str) -> bool {
        let Some(entry) = self.entry(id).cloned() else { return true };
        let Some(release) = entry.release().cloned() else { return true };
        ui.set_width(560.0f32.min(ui.ctx().content_rect().width() - 80.0));
        let date = release.published_at.map(|d| format!("Released {}", time::format_date(d))).unwrap_or_default();
        Self::title(ui, crate::icons::app(id), &format!("{} {}", entry.app.name, release.version_label()), &date);
        egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.6).show(ui, |ui| {
            if release.notes.is_empty() {
                ui.label("These notes weren't fetched (GitHub's request limit was reached). They're on the release page.");
            } else {
                crate::notes::show(ui, &release.notes);
            }
        });
        ui.add_space(12.0);
        let mut close = false;
        ui.horizontal(|ui| {
            if ui.link("Open on GitHub").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(release.html_url.clone()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::secondary(ui, "Close", 90.0).clicked();
            });
        });
        close
    }

    fn uninstall_dialog(&mut self, ui: &mut egui::Ui, id: &str) -> bool {
        let Some(entry) = self.entry(id).cloned() else { return true };
        let t = Tokens::get(ui.ctx());
        ui.set_width(420.0);
        Self::title(ui, crate::icons::app(id), &format!("Uninstall {}?", entry.app.name), "");
        let how = match entry.installed.as_ref().map(|i| i.source) {
            Some(artcraft_launcher_core::InstallSource::MacApp) => "The app will be moved to the Trash. Your documents and settings stay.",
            Some(artcraft_launcher_core::InstallSource::Msi) => {
                "Windows will remove it like any installed program. Your documents and settings stay."
            }
            Some(
                artcraft_launcher_core::InstallSource::Deb
                | artcraft_launcher_core::InstallSource::Rpm
                | artcraft_launcher_core::InstallSource::Pacman,
            ) => "Your system's package manager will remove it, and may ask for your password. Your documents and settings stay.",
            _ => "The app will be removed. Your documents and settings stay.",
        };
        ui.label(RichText::new(how).color(t.text_muted));
        ui.add_space(16.0);
        let mut close = false;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::primary(ui, "Uninstall", t.bad, 110.0).clicked() {
                self.send(Command::Uninstall { app: id.to_string() });
                close = true;
            }
            if widgets::secondary(ui, "Cancel", 90.0).clicked() {
                close = true;
            }
        });
        close
    }

    fn settings_dialog(&mut self, ui: &mut egui::Ui, host: &mut dyn Host) -> bool {
        let t = Tokens::get(ui.ctx());
        ui.set_width(460.0);
        // The app icon is the black mark; on a dark dialog show the blue one.
        Self::title(ui, Some(crate::icons::artcraft_mark(t.dark)), "Settings", &format!("ArtCraft Launcher {}", env!("CARGO_PKG_VERSION")));
        let section = |ui: &mut egui::Ui, s: &str| {
            ui.add_space(10.0);
            ui.label(RichText::new(s.to_uppercase()).size(11.0).family(theme::semibold()).color(t.text_faint));
            ui.add_space(2.0);
        };
        let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add(ui));
            });
        };

        section(ui, "Updates");
        let mut every = self.settings.check_every;
        row(ui, "Check for updates", &mut |ui| {
            egui::ComboBox::from_id_salt("every").selected_text(every.label()).width(150.0).show_ui(ui, |ui| {
                for e in CheckEvery::ALL {
                    ui.selectable_value(&mut every, e, e.label());
                }
            });
        });
        self.settings.check_every = every;
        ui.checkbox(&mut self.settings.notify, "Notify me when an installed app has an update");
        ui.checkbox(&mut self.settings.prereleases, "Include pre-releases (betas and release candidates)");

        if self.caps.manage_apps && self.platform.is_some_and(|p| p.os == Os::Linux) {
            section(ui, "Installing on Linux");
            let mut choice = self.settings.preferred_kinds.first().copied();
            row(ui, "Install new apps as", &mut |ui| {
                let label = choice.map_or("System package (recommended)", PackageKind::label);
                egui::ComboBox::from_id_salt("kind").selected_text(label).width(210.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut choice, None, "System package (recommended)");
                    for k in [PackageKind::Deb, PackageKind::Rpm, PackageKind::Flatpak, PackageKind::AppImage] {
                        ui.selectable_value(&mut choice, Some(k), k.label());
                    }
                });
            });
            self.settings.preferred_kinds = choice.into_iter().collect();
            ui.label(RichText::new("Updates always use the format an app was installed with.").size(12.0).color(t.text_muted));
        }

        if self.caps.manage_apps {
            section(ui, "Running in the background");
            if host.has_tray() {
                ui.checkbox(&mut self.settings.close_to_tray, "Keep running in the tray when the window is closed");
            }
            if let Some(mut on) = host.login_item()
                && ui.checkbox(&mut on, "Start ArtCraft Launcher when I log in").changed()
                && let Err(e) = host.set_login_item(on)
            {
                let ctx = ui.ctx().clone();
                self.toast(&ctx, format!("Couldn't change the login item: {e}"), true);
            }
        }

        section(ui, "Community");
        ui.label(RichText::new("Questions, ideas and news from the ArtCraft team").color(t.text_muted));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            use artcraft_launcher_core::catalog::{DISCORD_URL, GITHUB_URL, WEBSITE_URL};
            if widgets::discord_button(ui).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(DISCORD_URL));
            }
            if widgets::secondary(ui, "Website", 0.0).on_hover_text(WEBSITE_URL).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(WEBSITE_URL));
            }
            if widgets::secondary(ui, "GitHub", 0.0).on_hover_text(GITHUB_URL).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(GITHUB_URL));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                widgets::social_icons(ui, 16.0);
            });
        });

        section(ui, "Appearance");
        let mut theme_choice = self.settings.theme;
        ui.horizontal(|ui| {
            widgets::segmented(ui, &mut theme_choice, &ThemeChoice::ALL.map(|c| (c, c.label(), c.label())));
        });
        self.settings.theme = theme_choice;

        ui.add_space(18.0);
        let mut close = false;
        ui.horizontal(|ui| {
            if self.caps.manage_apps && widgets::secondary(ui, "Quit ArtCraft Launcher", 0.0).clicked() {
                host.quit();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::primary(ui, "Done", t.accent, 90.0).clicked();
            });
        });
        close
    }
}
