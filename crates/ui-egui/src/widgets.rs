//! Small building blocks: pills, buttons, segmented controls.

use egui::{Color32, CornerRadius, Response, RichText, Sense, Stroke, Ui, vec2};

use crate::model::Tone;
use crate::theme::{self, Tokens, tint};

pub fn tone_color(t: &Tokens, tone: Tone) -> Color32 {
    match tone {
        Tone::Good => t.good,
        Tone::Warn => t.warn,
        Tone::Info => t.info,
        Tone::Neutral => t.text_muted,
        Tone::Faint => t.text_faint,
    }
}

/// A rounded status label with a coloured dot.
pub fn pill(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let t = Tokens::get(ui.ctx());
    let c = tone_color(&t, tone);
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::new(12.0, theme::medium()), c);
    let size = vec2(galley.size().x + 26.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(11), tint(c, if t.dark { 0.14 } else { 0.11 }));
    p.circle_filled(egui::pos2(rect.left() + 11.0, rect.center().y), 3.5, c);
    p.galley(egui::pos2(rect.left() + 19.0, rect.center().y - galley.size().y / 2.0), galley, c);
    // Painted, so tell screen readers what it says.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    resp
}

/// A filled call-to-action button.
pub fn primary(ui: &mut Ui, text: &str, fill: Color32, width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    let btn = egui::Button::new(RichText::new(text).color(t.accent_text).family(theme::semibold()))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(theme::RADIUS_SMALL)
        .min_size(vec2(width, 32.0));
    let resp = ui.add(btn);
    if resp.hovered() {
        ui.painter().rect_filled(resp.rect, CornerRadius::same(theme::RADIUS_SMALL), Color32::from_white_alpha(18));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A quiet outlined button.
pub fn secondary(ui: &mut Ui, text: &str, width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    let btn = egui::Button::new(RichText::new(text).color(t.text).family(theme::medium()))
        .fill(if t.dark { Color32::from_rgb(0x23, 0x27, 0x31) } else { Color32::from_rgb(0xf1, 0xf2, 0xf6) })
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(theme::RADIUS_SMALL)
        .min_size(vec2(width, 32.0));
    ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A square icon-only button.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let btn = egui::Button::new(RichText::new(icon).size(16.0).color(t.text_muted))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .min_size(vec2(32.0, 32.0));
    ui.add(btn).on_hover_text(tip)
}

/// Two or more mutually exclusive options in one rounded control.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str, &str)]) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    egui::Frame::new()
        .fill(if t.dark { Color32::from_rgb(0x0e, 0x10, 0x14) } else { Color32::from_rgb(0xec, 0xee, 0xf2) })
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(theme::RADIUS_SMALL + 1)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // In a right-to-left parent the row fills from the right: keep the options' order.
            let rtl = ui.layout().prefer_right_to_left();
            ui.horizontal(|ui| {
                let ordered: Vec<&(T, &str, &str)> = if rtl { options.iter().rev().collect() } else { options.iter().collect() };
                for (opt, label, tip) in ordered {
                    let on = *value == *opt;
                    let btn =
                        egui::Button::new(RichText::new(*label).size(13.0).family(theme::medium()).color(if on { t.text } else { t.text_muted }))
                            .fill(if on { t.card_hover } else { Color32::TRANSPARENT })
                            .stroke(if on { Stroke::new(1.0, t.border_strong) } else { Stroke::NONE })
                            .corner_radius(theme::RADIUS_SMALL - 1)
                            .min_size(vec2(0.0, 26.0));
                    if ui.add(btn).on_hover_text(*tip).clicked() && !on {
                        *value = *opt;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A filter chip with a count.
pub fn chip(ui: &mut Ui, on: bool, label: &str, count: usize, highlight: Option<Color32>) -> Response {
    let t = Tokens::get(ui.ctx());
    let text_color = if on { t.text } else { t.text_muted };
    let mut job = egui::text::LayoutJob::default();
    job.append(label, 0.0, egui::TextFormat { font_id: egui::FontId::new(13.0, theme::medium()), color: text_color, ..Default::default() });
    job.append(
        &format!("  {count}"),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(12.0, theme::medium()),
            color: highlight.filter(|_| count > 0).unwrap_or(t.text_faint),
            ..Default::default()
        },
    );
    let btn = egui::Button::new(job)
        .fill(if on { tint(t.accent, if t.dark { 0.20 } else { 0.12 }) } else { Color32::TRANSPARENT })
        .stroke(if on { Stroke::new(1.0, tint(t.accent, 0.55)) } else { Stroke::new(1.0, t.border) })
        .corner_radius(CornerRadius::same(15))
        .min_size(vec2(0.0, 28.0));
    ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A slim progress bar with a caption.
pub fn progress(ui: &mut Ui, fraction: Option<f32>, caption: &str, color: Color32, width: f32) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
    let p = ui.painter();
    let bar = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - 6.0), vec2(width, 6.0));
    p.rect_filled(bar, CornerRadius::same(3), t.border);
    let f = match fraction {
        Some(f) => f.clamp(0.0, 1.0),
        None => {
            // Indeterminate: a sweeping segment.
            let time = ui.input(|i| i.time) as f32;
            let x = (time * 0.8).fract();
            let seg =
                egui::Rect::from_min_size(egui::pos2(bar.left() + bar.width() * (x * 1.3 - 0.3).max(0.0), bar.top()), vec2(bar.width() * 0.3, 6.0))
                    .intersect(bar);
            p.rect_filled(seg, CornerRadius::same(3), color);
            ui.ctx().request_repaint();
            -1.0
        }
    };
    if f >= 0.0 {
        p.rect_filled(egui::Rect::from_min_size(bar.min, vec2(bar.width() * f, 6.0)), CornerRadius::same(3), color);
    }
    p.text(egui::pos2(rect.left(), rect.top() + 7.0), egui::Align2::LEFT_CENTER, caption, egui::FontId::new(12.0, theme::medium()), t.text_muted);
    let resp = ui.interact(rect, ui.id().with(("progress", caption)), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, caption));
}

pub fn human_bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let n = n as f64;
    if n >= 1024.0 * MB {
        format!("{:.1} GB", n / (1024.0 * MB))
    } else if n >= MB {
        format!("{:.0} MB", n / MB)
    } else {
        format!("{:.0} KB", (n / 1024.0).max(1.0))
    }
}

/// Which drawn glyph a [`glyph_button`] shows (drawn, so no font needs the symbol).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Pause,
    Close,
}

/// A small square button with a drawn glyph.
pub fn glyph_button(ui: &mut Ui, glyph: Glyph, tip: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter();
    if hovered {
        p.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_SMALL),
            if t.dark { Color32::from_rgb(0x2b, 0x30, 0x3c) } else { Color32::from_rgb(0xe4, 0xe7, 0xee) },
        );
    }
    let c = if hovered { t.text } else { t.text_muted };
    let m = rect.center();
    match glyph {
        Glyph::Pause => {
            for dx in [-3.5, 3.5] {
                p.rect_filled(egui::Rect::from_center_size(m + vec2(dx, 0.0), vec2(3.0, 11.0)), CornerRadius::same(1), c);
            }
        }
        Glyph::Close => {
            let s = 4.5;
            p.line_segment([m + vec2(-s, -s), m + vec2(s, s)], Stroke::new(1.6, c));
            p.line_segment([m + vec2(-s, s), m + vec2(s, -s)], Stroke::new(1.6, c));
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "Discord": opens the ArtCraft community server. Outlined in Discord's blurple so it reads as
/// a community link rather than an app action.
pub fn discord_button(ui: &mut Ui) -> Response {
    let t = Tokens::get(ui.ctx());
    let blurple = Color32::from_rgb(0x58, 0x65, 0xf2);
    let text_color = if t.dark { Color32::from_rgb(0xc9, 0xcd, 0xfb) } else { Color32::from_rgb(0x40, 0x4e, 0xed) };
    let btn = egui::Button::new(RichText::new("💬  Discord").size(13.0).family(theme::medium()).color(text_color))
        .fill(tint(blurple, if t.dark { 0.16 } else { 0.08 }))
        .stroke(Stroke::new(1.0, tint(blurple, if t.dark { 0.55 } else { 0.40 })))
        .corner_radius(theme::RADIUS_SMALL + 1)
        .min_size(vec2(0.0, 32.0));
    ui.add(btn).on_hover_text("Join the ArtCraft community on Discord").on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A quiet text link: faint until hovered, then accent and underlined. Opens `url` in the browser.
pub fn small_link(ui: &mut Ui, text: &str, url: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = egui::FontId::new(11.5, theme::medium());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, t.text_faint);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    let color = if resp.hovered() { t.accent } else { t.text_faint };
    let p = ui.painter();
    p.galley_with_override_text_color(rect.min, galley, color);
    if resp.hovered() {
        p.line_segment([egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.bottom())], Stroke::new(1.0, color));
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, text));
    let resp = resp.on_hover_text(url).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    resp
}
