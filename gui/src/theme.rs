//! FIDOM visual language: palette, type, and shared widget treatments.
//! See `docs/VISUAL-OVERHAUL.md`. Keep this module presentation-only.
//!
//! References: cyberpunk HUD conventions (L-bracket frames, scanlines,
//! near-black grounds, neon used as signal), CRT terminal phosphor, and the
//! civic-decay direction in the visual-overhaul plan.

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, StrokeKind,
    TextStyle,
};

pub const INK: Color32 = Color32::from_rgb(0x06, 0x0B, 0x0E);
pub const VOID: Color32 = Color32::from_rgb(0x03, 0x06, 0x08);
pub const PANEL: Color32 = Color32::from_rgb(0x0A, 0x12, 0x17);
pub const PANEL_ALT: Color32 = Color32::from_rgb(0x0F, 0x1B, 0x22);
pub const PANEL_HOT: Color32 = Color32::from_rgb(0x14, 0x26, 0x2F);
pub const LINE: Color32 = Color32::from_rgb(0x21, 0x36, 0x40);
pub const LINE_HOT: Color32 = Color32::from_rgb(0x31, 0x55, 0x62);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xF2, 0xF5);
pub const MUTED: Color32 = Color32::from_rgb(0x8F, 0xA8, 0xB3);
pub const FAINT: Color32 = Color32::from_rgb(0x5D, 0x76, 0x82);
pub const OLDNET: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x24);
pub const BETANET: Color32 = Color32::from_rgb(0x4C, 0xE4, 0xF2);
pub const MONEY: Color32 = Color32::from_rgb(0xF2, 0xC1, 0x4E);
pub const DANGER: Color32 = Color32::from_rgb(0xFF, 0x4D, 0x5E);
pub const SUCCESS: Color32 = Color32::from_rgb(0x5A, 0xE6, 0xA2);
pub const VEIL: Color32 = Color32::from_rgb(0xB3, 0x88, 0xFF);
pub const PAPER: Color32 = Color32::from_rgb(0xE8, 0xE1, 0xD0);
pub const LOOP: Color32 = Color32::from_rgb(0x7C, 0xE0, 0x92);
pub const DATA: Color32 = Color32::from_rgb(0x5E, 0xC8, 0xF0);
pub const ALGO: Color32 = Color32::from_rgb(0xD8, 0x8C, 0xF0);

/// The display face (Chakra Petch): headings, tabs, buttons, card titles.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, display_family())
}

/// A translucent version of a palette color.
pub fn alpha(color: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

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
    fonts.font_data.insert(
        "chakra".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf")).into(),
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
    fonts
        .families
        .entry(display_family())
        .or_default()
        .extend(["chakra".to_owned(), "plex_sans".to_owned()]);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = VOID;
    style.visuals.faint_bg_color = PANEL_ALT;
    style.visuals.code_bg_color = VOID;
    style.visuals.window_stroke = Stroke::new(1.0, LINE_HOT);
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(130),
    };
    style.visuals.selection.bg_fill = alpha(BETANET, 48);
    style.visuals.selection.stroke = Stroke::new(1.0, BETANET);
    style.visuals.text_cursor.stroke = Stroke::new(1.5, BETANET);
    style.visuals.hyperlink_color = BETANET;
    style.visuals.striped = true;

    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::ZERO;

    style.visuals.widgets.inactive.bg_fill = PANEL_ALT;
    style.visuals.widgets.inactive.weak_bg_fill = PANEL;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    style.visuals.widgets.inactive.expansion = 0.0;

    style.visuals.widgets.hovered.bg_fill = PANEL_HOT;
    style.visuals.widgets.hovered.weak_bg_fill = PANEL_ALT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BETANET);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    style.visuals.widgets.hovered.expansion = 0.0;

    style.visuals.widgets.active.bg_fill = Color32::from_rgb(0x1A, 0x33, 0x3D);
    style.visuals.widgets.active.weak_bg_fill = PANEL_HOT;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, OLDNET);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, OLDNET);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::ZERO;
    style.visuals.widgets.active.expansion = 1.0;

    style.visuals.widgets.open.bg_fill = PANEL_HOT;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, BETANET);
    style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::ZERO;

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.window_margin = egui::Margin::same(10);
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.bar_outer_margin = 0.0;

    style.text_styles.insert(TextStyle::Heading, display(21.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, display(12.5));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(10.0, FontFamily::Monospace));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

/// Honor the reduced-motion preference without a second theme.
pub fn set_motion(ctx: &egui::Context, reduced: bool) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.animation_time = if reduced { 0.0 } else { 0.15 };
        ctx.set_style_of(theme, style);
    }
}

/// L-bracket marks at the four corners of a panel. The signature HUD cue:
/// the panel reads as tracked rather than merely boxed.
pub fn corner_marks(painter: &egui::Painter, rect: egui::Rect, color: Color32, len: f32) {
    let s = Stroke::new(1.0, color);
    let r = rect.shrink(0.5);
    for (corner, dir) in [
        (r.left_top(), egui::vec2(1.0, 1.0)),
        (r.right_top(), egui::vec2(-1.0, 1.0)),
        (r.left_bottom(), egui::vec2(1.0, -1.0)),
        (r.right_bottom(), egui::vec2(-1.0, -1.0)),
    ] {
        painter.line_segment([corner, corner + egui::vec2(dir.x * len, 0.0)], s);
        painter.line_segment([corner, corner + egui::vec2(0.0, dir.y * len)], s);
    }
}

/// A fixed backdrop for a view: near-black ground, hairline grid, faint
/// scanlines. Painted before the scroll content so it stays still.
pub fn backdrop(ui: &egui::Ui, accent: Color32) {
    let rect = ui.max_rect();
    let p = ui.painter();
    p.rect_filled(rect, 0.0, INK);

    // A soft phosphor bloom in the upper-left keeps the field from reading
    // as a flat web background.
    p.circle_filled(
        rect.left_top() + egui::vec2(140.0, -60.0),
        620.0,
        alpha(accent, 7),
    );

    let step = 32.0;
    let grid = alpha(accent, 12);
    let mut x = rect.left();
    while x < rect.right() {
        p.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
        x += step;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        p.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, grid),
        );
        y += step;
    }

    // Scanlines: dark hairlines over the grid, not a full CRT shader.
    let mut sy = rect.top();
    while sy < rect.bottom() {
        p.line_segment(
            [egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)],
            Stroke::new(1.0, Color32::from_black_alpha(22)),
        );
        sy += 4.0;
    }

    p.rect_stroke(
        rect.shrink(0.5),
        0.0,
        Stroke::new(1.0, alpha(LINE, 150)),
        StrokeKind::Inside,
    );
}

/// A quieter scanline wash for the side rails.
pub fn scanlines(ui: &egui::Ui, alpha_black: u8) {
    let rect = ui.max_rect();
    let p = ui.painter();
    let mut y = rect.top();
    while y < rect.bottom() {
        p.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_black_alpha(alpha_black)),
        );
        y += 4.0;
    }
}

/// A titled, framed block. Every region gets a hard-edged panel, an accent
/// rail, and L-brackets; the shell should read as a monitored terminal.
pub fn card<R>(
    ui: &mut egui::Ui,
    title: &str,
    accent: Color32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    // Use last frame's rect for hover: the frame's own rect is only known
    // after the content has been laid out. The shell repaints every frame,
    // so the light follows the cursor immediately.
    let hover_id = ui.id().with(("card_hover", title));
    let previous: Option<egui::Rect> = ui.data(|d| d.get_temp(hover_id));
    let hot = previous.is_some_and(|r| ui.rect_contains_pointer(r));
    let fill = if hot { PANEL_HOT } else { PANEL };
    let border = if hot {
        Stroke::new(1.0, alpha(accent, 220))
    } else {
        Stroke::new(1.0, alpha(LINE, 220))
    };
    let shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: if hot { 18 } else { 10 },
        spread: 0,
        color: if hot {
            alpha(accent, 42)
        } else {
            Color32::from_black_alpha(110)
        },
    };
    let response = egui::Frame::new()
        .fill(fill)
        .stroke(border)
        .corner_radius(egui::CornerRadius::ZERO)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .outer_margin(egui::Margin::symmetric(0, 3))
        .shadow(shadow)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("//")
                        .family(display_family())
                        .size(10.5)
                        .strong()
                        .color(alpha(accent, 230)),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(title.to_uppercase())
                        .family(display_family())
                        .size(10.5)
                        .color(MUTED),
                );
            });
            let y = ui.cursor().top();
            ui.painter().line_segment(
                [
                    egui::pos2(ui.max_rect().left(), y),
                    egui::pos2(ui.max_rect().right(), y),
                ],
                Stroke::new(1.0, alpha(LINE, 170)),
            );
            ui.add_space(6.0);
            add(ui)
        });

    let rect = response.response.rect;
    ui.data_mut(|d| d.insert_temp(hover_id, rect));
    let p = ui.painter();
    p.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, alpha(accent, if hot { 255 } else { 190 })),
    );
    p.line_segment(
        [
            rect.left_top() + egui::vec2(0.0, 1.0),
            rect.left_top() + egui::vec2(if hot { 84.0 } else { 54.0 }, 1.0),
        ],
        Stroke::new(2.0, accent),
    );
    corner_marks(
        p,
        rect,
        if hot { accent } else { alpha(accent, 135) },
        if hot { 9.0 } else { 7.0 },
    );
    if hot {
        p.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, alpha(accent, 90)),
            StrokeKind::Inside,
        );
    }
    response.inner
}

/// A sub-heading inside a card. Keep it a real label so accesskit and the
/// headless tests still see the text.
pub fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(7.0);
    ui.label(
        RichText::new(format!("› {}", label.to_uppercase()))
            .family(display_family())
            .size(9.5)
            .strong()
            .color(FAINT),
    );
}

pub fn label(ui: &mut egui::Ui, s: impl Into<String>) -> egui::Response {
    ui.label(
        RichText::new(s.into())
            .family(FontFamily::Monospace)
            .size(11.5)
            .color(MUTED),
    )
}

/// A segmented phosphor meter. The label is painted into the bar for the
/// visual, and exposed to accesskit as a progress indicator so headless
/// tests and screen readers still get the value.
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
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(240.0), 25.0),
        egui::Sense::hover(),
    );
    let text = format!("{label} {:.0}/{:.0}", value, max);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, &text)
    });

    let p = ui.painter();
    let label_y = rect.top() + 1.0;
    p.text(
        egui::pos2(rect.left() + 1.0, label_y),
        egui::Align2::LEFT_TOP,
        label.to_uppercase(),
        FontId::monospace(9.0),
        MUTED,
    );
    p.text(
        egui::pos2(rect.right() - 1.0, label_y),
        egui::Align2::RIGHT_TOP,
        format!("{value:.0}/{max:.0}"),
        FontId::monospace(9.0),
        TEXT,
    );

    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + 12.0),
        rect.right_bottom(),
    );
    p.rect_filled(bar, 0.0, VOID);
    p.rect_stroke(
        bar,
        0.0,
        Stroke::new(1.0, alpha(LINE, 230)),
        StrokeKind::Inside,
    );
    let inner = bar.shrink2(egui::vec2(2.0, 2.0));
    let segments = 20.0;
    let gap = 1.0;
    let seg_w = (inner.width() - gap * (segments - 1.0)) / segments;
    for i in 0..segments as u32 {
        let x = inner.left() + i as f32 * (seg_w + gap);
        let seg = egui::Rect::from_min_size(
            egui::pos2(x, inner.top()),
            egui::vec2(seg_w, inner.height()),
        );
        let filled = (i as f32 + 0.5) / segments <= frac;
        p.rect_filled(seg, 0.0, if filled { color } else { alpha(LINE, 70) });
    }
    response
}

pub fn chip(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    ui.label(
        RichText::new(text.into())
            .family(FontFamily::Monospace)
            .size(10.0)
            .strong()
            .color(color),
    )
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
        ui.label(
            RichText::new(label)
                .family(FontFamily::Monospace)
                .size(10.0)
                .color(FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value.into())
                    .family(FontFamily::Monospace)
                    .size(11.5)
                    .color(color),
            );
        });
    })
    .response
}

pub fn heading(ui: &mut egui::Ui, text: impl Into<String>) {
    let response = ui.label(
        RichText::new(format!("// {}", text.into()))
            .family(display_family())
            .size(21.0)
            .strong()
            .color(TEXT),
    );
    let r = response.rect;
    ui.painter().line_segment(
        [
            r.left_bottom() + egui::vec2(0.0, 3.0),
            r.left_bottom() + egui::vec2(42.0, 3.0),
        ],
        Stroke::new(2.0, alpha(BETANET, 150)),
    );
    ui.add_space(3.0);
}
