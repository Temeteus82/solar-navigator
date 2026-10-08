use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::{Color32, CornerRadius, Stroke};

/// Panel and window background: a cool near-black that sits with the starfield
/// rather than egui's neutral grey.
const PANEL_FILL: Color32 = Color32::from_rgb(14, 17, 24);
/// Text-field and scroll-track background, one step darker than the panel.
const FIELD_FILL: Color32 = Color32::from_rgb(8, 10, 15);
/// Button / inactive widget fills, lightening through hover and press.
const WIDGET_FILL: Color32 = Color32::from_rgb(30, 36, 48);
const WIDGET_HOVER_FILL: Color32 = Color32::from_rgb(42, 50, 66);
const WIDGET_ACTIVE_FILL: Color32 = Color32::from_rgb(54, 64, 84);
/// Outline of inactive widgets. WCAG 1.4.11 wants 3:1 for control boundaries;
/// this is ~3.5:1 on `PANEL_FILL`, so unchecked checkboxes stay visible.
const WIDGET_BORDER: Color32 = Color32::from_rgb(96, 108, 130);
/// Accent for selection, focus and links.
const ACCENT: Color32 = Color32::from_rgb(140, 190, 255);
const ACCENT_FILL: Color32 = Color32::from_rgb(36, 62, 104);

/// Applies the app's egui theme and icon font once, on the first UI pass.
///
/// Runs in `EguiPrimaryContextPass` rather than `Startup` because the egui
/// context only exists once bevy_egui has set up the primary window.
pub(super) fn apply_theme(mut contexts: EguiContexts, mut applied: Local<bool>) -> Result {
    if *applied {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;

    let mut fonts = egui::FontDefinitions::default();
    // The solid variant: the thin Regular glyphs vanish at body text size.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Fill);
    ctx.set_fonts(fonts);

    // A space viewer is dark-only: following a light OS theme would put the
    // panel on a white background beside a black viewport, and the contrast
    // tuning below assumes dark.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, style_dark);

    *applied = true;
    Ok(())
}

fn style_dark(style: &mut egui::Style) {
    // Bump every text style up by 1pt from egui defaults.
    for (style_key, font_id) in style.text_styles.iter_mut() {
        font_id.size = match style_key {
            egui::TextStyle::Small => 11.0,
            egui::TextStyle::Body => 15.0,
            egui::TextStyle::Monospace => 15.0,
            egui::TextStyle::Button => 15.0,
            egui::TextStyle::Heading => 21.0,
            _ => font_id.size,
        };
    }

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.interact_size.y = 24.0;

    let visuals = &mut style.visuals;
    visuals.panel_fill = PANEL_FILL;
    visuals.window_fill = PANEL_FILL;
    visuals.extreme_bg_color = FIELD_FILL;
    visuals.faint_bg_color = WIDGET_FILL;
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT_FILL;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    let widgets = &mut visuals.widgets;
    for (state, fill) in [
        (&mut widgets.inactive, WIDGET_FILL),
        (&mut widgets.hovered, WIDGET_HOVER_FILL),
        (&mut widgets.active, WIDGET_ACTIVE_FILL),
        (&mut widgets.open, WIDGET_HOVER_FILL),
    ] {
        // Small: this radius also rounds the 14 px checkbox, and anything
        // much larger turns it into a circle that reads as a radio button.
        state.corner_radius = CornerRadius::same(3);
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
    }
    widgets.noninteractive.corner_radius = CornerRadius::same(3);
    widgets.noninteractive.bg_fill = PANEL_FILL;
    widgets.noninteractive.weak_bg_fill = PANEL_FILL;
    widgets.inactive.bg_stroke = Stroke::new(1.0, WIDGET_BORDER);
    widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);

    // Accessibility (WCAG 1.4.3): egui's dark default body text (~gray 140,
    // ~5:1 on the panel background) leaves no headroom for a *passing*
    // weaker hint/secondary colour. Lift primary text to ~gray 205 (~10:1)
    // and raise the weak-text alpha so hint and secondary text still clear
    // the 4.5:1 minimum (~4.8:1) while staying visibly subordinate. The
    // panel fill above is darker than egui's default, so both ratios only
    // improve.
    widgets.noninteractive.fg_stroke.color = Color32::from_gray(205);
    visuals.weak_text_alpha = 0.66;
}
