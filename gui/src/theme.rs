//! FIDOM visual language: palette, type, and shared widget treatments.
//! See `docs/VISUAL-OVERHAUL.md`. Keep this module presentation-only.

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle,
};

pub const INK: Color32 = Color32::from_rgb(0x0A, 0x0E, 0x12);
pub const PANEL: Color32 = Color32::from_rgb(0x11, 0x18, 0x20);
pub const PANEL_ALT: Color32 = Color32::from_rgb(0x16, 0x1F, 0x29);
pub const LINE: Color32 = Color32::from_rgb(0x26, 0x32, 0x3D);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xED, 0xF2);
pub const MUTED: Color32 = Color32::from_rgb(0x93, 0xA1, 0xAE);
pub const FAINT: Color32 = Color32::from_rgb(0x7C, 0x89, 0x96);
pub const OLDNET: Color32 = Color32::from_rgb(0xF0, 0xA8, 0x30);
pub const OLDNET_DIM: Color32 = Color32::from_rgb(0xB9, 0x7A, 0x22);
pub const BETANET: Color32 = Color32::from_rgb(0x59, 0xD8, 0xE6);
pub const MONEY: Color32 = Color32::from_rgb(0xE6, 0xB8, 0x5C);
pub const DANGER: Color32 = Color32::from_rgb(0xF0, 0x6A, 0x6F);
pub const SUCCESS: Color32 = Color32::from_rgb(0x62, 0xC9, 0x8A);
pub const VEIL: Color32 = Color32::from_rgb(0xA7, 0x8B, 0xFA);
pub const PAPER: Color32 = Color32::from_rgb(0xE8, 0xE1, 0xD0);
pub const LOOP: Color32 = Color32::from_rgb(120, 200, 120);
pub const DATA: Color32 = Color32::from_rgb(110, 190, 230);
pub const ALGO: Color32 = Color32::from_rgb(220, 150, 240);

/// Semantic color for a resource component.
pub fn resource_color(res: sim::Res) -> Color32 {
    match res {
        sim::Res::Loop => LOOP,
        sim::Res::Data => DATA,
        sim::Res::Algo => ALGO,
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "plex_sans".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "plex_mono".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "plex_sans".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "plex_mono".to_owned());
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = INK;
    style.visuals.faint_bg_color = PANEL_ALT;
    style.visuals.code_bg_color = INK;
    style.visuals.selection.bg_fill = Color32::from_rgb(0x1B, 0x3A, 0x45);
    style.visuals.selection.stroke = Stroke::new(1.0, BETANET);
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.bg_fill = PANEL_ALT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x1B, 0x27, 0x32);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BETANET);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(0x20, 0x2D, 0x38);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, OLDNET);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.menu_margin = egui::Margin::same(8);

    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(21.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.5, FontFamily::Monospace),
    );
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

/// Honor the reduced-motion preference without a second theme.
pub fn set_motion(ctx: &egui::Context, reduced: bool) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.animation_time = if reduced { 0.0 } else { 0.2 };
        ctx.set_style_of(theme, style);
    }
}

/// A titled, framed block. The shell is a dense terminal, so every region
/// gets an explicit edge and header; without them a new player cannot tell
/// where one system ends and the next begins.
pub fn card<R>(
    ui: &mut egui::Ui,
    title: &str,
    accent: Color32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(PANEL_ALT)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .outer_margin(egui::Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (bar, _) = ui.allocate_exact_size(egui::vec2(3.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(bar, 1.0, accent);
                ui.add_space(2.0);
                ui.label(
                    RichText::new(title.to_uppercase())
                        .size(10.0)
                        .strong()
                        .color(MUTED),
                );
            });
            ui.add(egui::Separator::default().spacing(6.0));
            add(ui)
        })
        .inner
}

/// A sub-heading inside a card. Keep it a real label so accesskit and the
/// headless tests still see the text.
pub fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(label.to_uppercase())
            .size(10.0)
            .strong()
            .color(MUTED),
    );
}

pub fn label(ui: &mut egui::Ui, s: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(s.into()).size(12.0).color(MUTED))
}

pub fn meter(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    max: f32,
    color: Color32,
) -> egui::Response {
    let frac = if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    };
    ui.add(
        egui::ProgressBar::new(frac)
            .desired_width(ui.available_width().min(220.0))
            .fill(color)
            .text(
                RichText::new(format!("{label} {:.0}/{:.0}", value, max))
                    .size(10.0)
                    .color(TEXT),
            ),
    )
}

pub fn chip(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    ui.label(RichText::new(text.into()).size(10.5).strong().color(color))
}

/// A compact label/value row for the narrow rail: quiet label on the left,
/// tabular value on the right. Keeps long prose from wrapping mid-number.
pub fn stat_row(
    ui: &mut egui::Ui,
    label: &str,
    value: impl Into<String>,
    color: Color32,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(FAINT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value.into())
                    .size(12.0)
                    .monospace()
                    .color(color),
            );
        });
    })
    .response
}

pub fn heading(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(20.0).strong().color(TEXT));
}
