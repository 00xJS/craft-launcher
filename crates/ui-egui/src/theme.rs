//! Colours, type and shapes. Widgets read [`Tokens::get`]; nothing hard-codes a colour.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    /// Follow the system's light or dark setting.
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Match system",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

/// Colour tokens for one appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub info: Color32,
}

impl Tokens {
    pub const DARK: Self = Self {
        dark: true,
        bg: Color32::from_rgb(0x0e, 0x10, 0x14),
        surface: Color32::from_rgb(0x14, 0x17, 0x1d),
        card: Color32::from_rgb(0x19, 0x1c, 0x24),
        card_hover: Color32::from_rgb(0x1f, 0x23, 0x2d),
        border: Color32::from_rgb(0x26, 0x2a, 0x35),
        border_strong: Color32::from_rgb(0x36, 0x3c, 0x4a),
        text: Color32::from_rgb(0xec, 0xee, 0xf2),
        text_muted: Color32::from_rgb(0x9a, 0xa1, 0xb0),
        text_faint: Color32::from_rgb(0x67, 0x6e, 0x7d),
        accent: Color32::from_rgb(0x7c, 0x6c, 0xff),
        accent_text: Color32::WHITE,
        good: Color32::from_rgb(0x3d, 0xd6, 0x8c),
        warn: Color32::from_rgb(0xff, 0xb5, 0x47),
        bad: Color32::from_rgb(0xff, 0x6b, 0x6b),
        info: Color32::from_rgb(0x6c, 0xb4, 0xff),
    };

    pub const LIGHT: Self = Self {
        dark: false,
        bg: Color32::from_rgb(0xf4, 0xf5, 0xf8),
        surface: Color32::from_rgb(0xff, 0xff, 0xff),
        card: Color32::from_rgb(0xff, 0xff, 0xff),
        card_hover: Color32::from_rgb(0xfa, 0xfa, 0xfd),
        border: Color32::from_rgb(0xe3, 0xe5, 0xeb),
        border_strong: Color32::from_rgb(0xc9, 0xcd, 0xd6),
        text: Color32::from_rgb(0x15, 0x17, 0x1c),
        text_muted: Color32::from_rgb(0x5b, 0x62, 0x70),
        text_faint: Color32::from_rgb(0x8b, 0x91, 0x9d),
        accent: Color32::from_rgb(0x5b, 0x4b, 0xf0),
        accent_text: Color32::WHITE,
        good: Color32::from_rgb(0x12, 0x8a, 0x52),
        warn: Color32::from_rgb(0xb8, 0x6e, 0x00),
        bad: Color32::from_rgb(0xc8, 0x2b, 0x2b),
        info: Color32::from_rgb(0x1f, 0x6f, 0xd6),
    };

    pub fn get(ctx: &egui::Context) -> Self {
        if ctx.theme() == egui::Theme::Dark { Self::DARK } else { Self::LIGHT }
    }
}

pub const RADIUS: u8 = 12;
pub const RADIUS_SMALL: u8 = 8;

/// Mixes `c` into the background at `alpha` (0–1): tinted chips and glows.
pub fn tint(c: Color32, alpha: f32) -> Color32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn visuals(t: &Tokens) -> Visuals {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.bg;
    v.window_fill = t.surface;
    v.extreme_bg_color = if t.dark { Color32::from_rgb(0x0b, 0x0d, 0x11) } else { Color32::from_rgb(0xee, 0xf0, 0xf4) };
    v.faint_bg_color = t.card;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = CornerRadius::same(RADIUS + 2);
    v.menu_corner_radius = CornerRadius::same(RADIUS_SMALL + 2);
    v.override_text_color = Some(t.text);
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = tint(t.accent, 0.35);
    v.selection.stroke = Stroke::new(1.0, t.accent);
    let r = CornerRadius::same(RADIUS_SMALL);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, t.card, t.border),
        (&mut v.widgets.inactive, if t.dark { Color32::from_rgb(0x23, 0x27, 0x31) } else { Color32::from_rgb(0xee, 0xf0, 0xf4) }, t.border),
        (&mut v.widgets.hovered, if t.dark { Color32::from_rgb(0x2b, 0x30, 0x3c) } else { Color32::from_rgb(0xe4, 0xe7, 0xee) }, t.border_strong),
        (&mut v.widgets.active, if t.dark { Color32::from_rgb(0x33, 0x39, 0x47) } else { Color32::from_rgb(0xda, 0xde, 0xe7) }, t.border_strong),
        (&mut v.widgets.open, if t.dark { Color32::from_rgb(0x2b, 0x30, 0x3c) } else { Color32::from_rgb(0xe4, 0xe7, 0xee) }, t.border_strong),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.corner_radius = r;
        w.fg_stroke = Stroke::new(1.0, t.text);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text_muted);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    v
}

/// Applies both appearances (egui switches between them with the theme preference).
pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&Tokens::DARK));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&Tokens::LIGHT));
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
        ThemeChoice::Light => egui::ThemePreference::Light,
    });
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.menu_margin = egui::Margin::same(6);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, semibold())),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, medium())),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Registers Inter with named weights.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-Medium", include_bytes!("../../../assets/fonts/Inter-Medium.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, primary) in [("medium", "Inter-Medium"), ("semibold", "Inter-SemiBold")] {
        let mut stack = vec![primary.to_owned()];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(family.into()), stack);
    }
    ctx.set_fonts(fonts);
}
