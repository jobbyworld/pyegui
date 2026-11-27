//! Themes, accent and glass: the styles of egui the pyegui widgets and the Rust applications share.

use std::sync::Mutex;

use egui::{self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, Style, TextStyle};

// global style

/// Options of Context.set_style, parsed before the style is modified
#[derive(Default)]
pub struct StyleOptions {
    pub rounding: Option<f32>,
    pub widget_rounding: Option<f32>,
    pub window_rounding: Option<f32>,
    pub panel_fill: Option<Color32>,
    pub window_fill: Option<Color32>,
    pub window_stroke: Option<Stroke>,
    pub window_shadow: Option<Shadow>,
    pub popup_shadow: Option<Shadow>,
    pub text_color: Option<Color32>,
    pub hyperlink_color: Option<Color32>,
    pub selection_color: Option<Color32>,
    pub extreme_bg_color: Option<Color32>,
    pub faint_bg_color: Option<Color32>,
    pub code_bg_color: Option<Color32>,
    pub button_fill: Option<Color32>,
    pub button_hover_fill: Option<Color32>,
    pub button_active_fill: Option<Color32>,
    pub button_stroke: Option<Stroke>,
    pub button_hover_stroke: Option<Stroke>,
    pub item_spacing: Option<egui::Vec2>,
    pub button_padding: Option<egui::Vec2>,
    pub window_margin: Option<f32>,
    pub menu_margin: Option<egui::Vec2>,
    pub body_size: Option<f32>,
    pub heading_size: Option<f32>,
    pub monospace_size: Option<f32>,
    pub animation_time: Option<f32>,
    pub theme: Option<egui::Theme>,
}

impl StyleOptions {
    pub fn apply(&self, style: &mut Style) {
        let visuals = &mut style.visuals;
        let widgets = &mut visuals.widgets;

        if let Some(rounding) = self.widget_rounding.or(self.rounding) {
            let rounding = CornerRadius::from(rounding);
            widgets.noninteractive.corner_radius = rounding;
            widgets.inactive.corner_radius = rounding;
            widgets.hovered.corner_radius = rounding;
            widgets.active.corner_radius = rounding;
            widgets.open.corner_radius = rounding;
        }
        if let Some(rounding) = self.window_rounding.or(self.rounding) {
            visuals.window_corner_radius = rounding.into();
            visuals.menu_corner_radius = rounding.into();
        }
        if let Some(fill) = self.button_fill {
            widgets.inactive.weak_bg_fill = fill;
            widgets.inactive.bg_fill = fill;
        }
        if let Some(fill) = self.button_hover_fill {
            widgets.hovered.weak_bg_fill = fill;
            widgets.hovered.bg_fill = fill;
        }
        if let Some(fill) = self.button_active_fill {
            widgets.active.weak_bg_fill = fill;
            widgets.active.bg_fill = fill;
            widgets.open.weak_bg_fill = fill;
            widgets.open.bg_fill = fill;
        }
        if let Some(stroke) = self.button_stroke {
            widgets.inactive.bg_stroke = stroke;
        }
        if let Some(stroke) = self.button_hover_stroke {
            widgets.hovered.bg_stroke = stroke;
            widgets.active.bg_stroke = stroke;
        }

        if let Some(fill) = self.panel_fill {
            visuals.panel_fill = fill;
        }
        if let Some(fill) = self.window_fill {
            visuals.window_fill = fill;
        }
        if let Some(stroke) = self.window_stroke {
            visuals.window_stroke = stroke;
        }
        if let Some(shadow) = self.window_shadow {
            visuals.window_shadow = shadow;
        }
        if let Some(shadow) = self.popup_shadow {
            visuals.popup_shadow = shadow;
        }
        if let Some(color) = self.text_color {
            visuals.override_text_color = Some(color);
        }
        if let Some(color) = self.hyperlink_color {
            visuals.hyperlink_color = color;
        }
        if let Some(color) = self.selection_color {
            visuals.selection.bg_fill = color;
        }
        if let Some(color) = self.extreme_bg_color {
            visuals.extreme_bg_color = color;
        }
        if let Some(color) = self.faint_bg_color {
            visuals.faint_bg_color = color;
        }
        if let Some(color) = self.code_bg_color {
            visuals.code_bg_color = color;
        }

        if let Some(spacing) = self.item_spacing {
            style.spacing.item_spacing = spacing;
        }
        if let Some(padding) = self.button_padding {
            style.spacing.button_padding = padding;
        }
        if let Some(margin) = self.window_margin {
            style.spacing.window_margin = margin.into();
        }
        if let Some(margin) = self.menu_margin {
            style.spacing.menu_margin = egui::Margin::symmetric(margin.x as i8, margin.y as i8);
        }

        for (text_style, size) in [
            (TextStyle::Body, self.body_size),
            (TextStyle::Button, self.body_size),
            (TextStyle::Heading, self.heading_size),
            (TextStyle::Monospace, self.monospace_size),
        ] {
            if let (Some(size), Some(font)) = (size, style.text_styles.get_mut(&text_style)) {
                font.size = size;
            }
        }

        if let Some(time) = self.animation_time {
            style.animation_time = time;
        }
    }
}

/// Default style of a theme
pub fn default_style(theme: egui::Theme) -> Style {
    Style {
        visuals: match theme {
            egui::Theme::Light => egui::Visuals::light(),
            egui::Theme::Dark => egui::Visuals::dark(),
        },
        ..Style::default()
    }
}

// modern theme

/// Where the glass goes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlassMode {
    /// every surface is glass
    Full,
    /// the sidebars, bars and menus are glass, the content is an almost opaque sheet
    Partial,
}

/// The parts of the theme that egui styles cannot hold
#[derive(Clone, Copy, Default)]
pub struct ThemeState {
    /// the color of the selection, links, active tabs and plots
    pub accent: Option<Color32>,
    /// set by set_glass_style
    pub glass: Option<GlassMode>,
    /// the opacities given to set_glass_style
    pub levels: GlassLevels,
    /// the color of the glass given to set_glass_style
    pub tint: Option<Color32>,
}

/// How opaque the surfaces of the glass are: `scale` multiplies the opacity of every surface,
/// the others give the opacity of one surface, between 0 (clear) and 1 (opaque)
#[derive(Clone, Copy, Debug)]
pub struct GlassLevels {
    pub scale: f32,
    pub panels: Option<f32>,
    pub windows: Option<f32>,
    pub content: Option<f32>,
    /// the left and right panels, instead of `panels`
    pub sidebar: Option<f32>,
    /// the top and bottom panels, instead of `panels`
    pub bars: Option<f32>,
    /// the central area, instead of `content`
    pub central: Option<f32>,
    /// the panes of the docks: given, each pane is filled on its own
    pub dock: Option<f32>,
    /// what is seen between the boxes: the whole window is filled under them
    pub background: Option<f32>,
}

impl GlassLevels {
    pub const DEFAULT: GlassLevels = GlassLevels {
        scale: 1.0,
        panels: None,
        windows: None,
        content: None,
        sidebar: None,
        bars: None,
        central: None,
        dock: None,
        background: None,
    };

    /// The alpha of a surface: the one given, or the default one scaled
    pub fn alpha(&self, given: Option<f32>, default: u8) -> u8 {
        let opacity = given.unwrap_or(default as f32 / 255.0 * self.scale);
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

impl Default for GlassLevels {
    fn default() -> Self {
        Self::DEFAULT
    }
}

pub static THEME: Mutex<ThemeState> = Mutex::new(ThemeState {
    accent: None,
    glass: None,
    levels: GlassLevels::DEFAULT,
    tint: None,
});

pub fn theme_state() -> ThemeState {
    THEME.lock().map(|t| *t).unwrap_or_default()
}

/// The accent of the modern theme when none is given
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(59, 130, 246);

/// The font family of the titles and strong texts, see app_fonts in lib.rs
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// True when the user asked the system for less transparency (macOS accessibility setting)
pub fn reduce_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// The color mixed with white, by amount between 0 and 1
pub fn lighten(color: Color32, amount: f32) -> Color32 {
    let mix = |c: u8| (c as f32 + (255.0 - c as f32) * amount).round() as u8;
    Color32::from_rgb(mix(color.r()), mix(color.g()), mix(color.b()))
}

/// Derives the selection, links and text cursor from the accent color
pub fn accent_style(style: &mut Style, accent: Color32) {
    let visuals = &mut style.visuals;
    let dark = visuals.dark_mode;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 225);
    // the text on the selection, like the title of the active pill tab
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = if dark { lighten(accent, 0.35) } else { accent };
    visuals.text_cursor.stroke = Stroke::new(2.0, if dark { lighten(accent, 0.3) } else { accent });
}

/// The almost opaque sheet under the content in the partial glass mode, None otherwise
pub fn sheet_fill(visuals: &egui::Visuals) -> Option<Color32> {
    let theme = theme_state();
    if theme.glass != Some(GlassMode::Partial) {
        return None;
    }
    let default = if visuals.dark_mode { 236 } else { 242 };
    let level = theme.levels.central.or(theme.levels.content);
    let alpha = if reduce_transparency() { 255 } else { theme.levels.alpha(level, default) };
    Some(if visuals.dark_mode {
        Color32::from_rgba_unmultiplied(28, 30, 36, alpha)
    } else {
        Color32::from_rgba_unmultiplied(251, 251, 253, alpha)
    })
}

/// A part of the window whose glass can have its own opacity, see set_glass_style
#[derive(Clone, Copy, PartialEq)]
pub enum Surface {
    /// left and right panels
    Sidebar,
    /// top and bottom panels
    Bars,
    /// the central area, and the panes of a dock marked central
    Central,
    /// the other panes of a dock
    Dock,
}

/// The color of the glass of the theme, without opacity
pub fn glass_base(dark: bool, tint: Option<Color32>) -> Color32 {
    let base = tint.unwrap_or(if dark { Color32::from_rgb(20, 22, 30) } else { Color32::from_rgb(250, 250, 255) });
    Color32::from_rgb(base.r(), base.g(), base.b())
}

/// The fill of a surface whose level was given to set_glass_style, None to keep the usual fill
/// (for the panes of a dock: not filled)
pub fn surface_fill(visuals: &egui::Visuals, surface: Surface) -> Option<Color32> {
    let theme = theme_state();
    let mode = theme.glass?;
    let levels = theme.levels;
    let level = match surface {
        Surface::Sidebar => levels.sidebar,
        Surface::Bars => levels.bars,
        Surface::Central => levels.central,
        // a dock filled on its own: its panes take the level of the central area by default
        Surface::Dock => levels.dock,
    }?;
    let dark = visuals.dark_mode;
    // the panels are glass; the content is the sheet of the partial mode, or glass in the full mode
    let color = match (surface, mode) {
        (Surface::Central | Surface::Dock, GlassMode::Partial) => {
            if dark {
                Color32::from_rgb(28, 30, 36)
            } else {
                Color32::from_rgb(251, 251, 253)
            }
        }
        _ => glass_base(dark, theme.tint),
    };
    let alpha = if reduce_transparency() { 255 } else { (level.clamp(0.0, 1.0) * 255.0).round() as u8 };
    Some(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha))
}

/// The glass under the whole window, seen in the gaps between the boxes: set_glass_style(background=...)
pub fn background_fill(visuals: &egui::Visuals) -> Option<Color32> {
    let theme = theme_state();
    theme.glass?;
    let level = theme.levels.background?;
    let color = glass_base(visuals.dark_mode, theme.tint);
    let alpha = if reduce_transparency() { 255 } else { (level.clamp(0.0, 1.0) * 255.0).round() as u8 };
    Some(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha))
}

/// True when the panes of the docks are filled on their own (set_glass_style(dock=...)): the
/// central area under a dock is then left clear
pub fn dock_filled() -> bool {
    let theme = theme_state();
    theme.glass.is_some() && theme.levels.dock.is_some()
}

/// Colors of the plot items without a color: the accent first, then colors that go with it
pub fn plot_palette() -> Option<Vec<Color32>> {
    let accent = theme_state().accent?;
    Some(vec![
        accent,
        Color32::from_rgb(34, 197, 94),
        Color32::from_rgb(245, 158, 11),
        Color32::from_rgb(239, 68, 68),
        Color32::from_rgb(168, 85, 247),
        Color32::from_rgb(6, 182, 212),
    ])
}

/// The style of the menus: the style of egui menus, which removes the button frames and squeezes
/// the buttons to 2 points of padding, with room around the texts in the modern theme
pub fn menu_style() -> egui::style::StyleModifier {
    let modern = theme_state().glass.is_some();
    (move |style: &mut Style| {
        egui::containers::menu::menu_style(style);
        crate::popup_style::apply(style, crate::popup_style::Kind::Menu);
        if modern {
            style.spacing.button_padding = egui::vec2(10.0, 4.0);
        }
    })
    .into()
}

/// The menu config of pyegui, see menu_style
pub fn menu_config() -> egui::containers::menu::MenuConfig {
    egui::containers::menu::MenuConfig::new().style(menu_style())
}

/// Text sizes of the modern theme: 11.5 / 14 / 20, titles in semibold
fn modern_text_styles(style: &mut Style) {
    use FontFamily::{Monospace, Proportional};
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.5, Proportional)),
        (TextStyle::Body, FontId::new(14.0, Proportional)),
        (TextStyle::Button, FontId::new(14.0, Proportional)),
        (TextStyle::Heading, FontId::new(20.0, bold_family())),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
    ]
    .into();
}

/// Translucent style looking like frosted glass. Best used with a backdrop effect in run_native.
pub fn modern_glass_style(
    style: &mut Style,
    tint: Option<Color32>,
    accent: Color32,
    mode: GlassMode,
    levels: GlassLevels,
) {
    glass_style(style, tint);
    let dark = style.visuals.dark_mode;
    let reduce = reduce_transparency();

    // menus and windows are read, so they are almost opaque
    let window_alpha = match (reduce, mode, dark) {
        (true, _, _) => 255,
        (_, GlassMode::Partial, true) => 236,
        (_, GlassMode::Partial, false) => 242,
        (_, GlassMode::Full, true) => 200,
        (_, GlassMode::Full, false) => 215,
    };
    let with_alpha = |color: Color32, alpha: u8| {
        let [r, g, b, _] = color.to_srgba_unmultiplied();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    };
    let window_alpha = if reduce { 255 } else { levels.alpha(levels.windows, window_alpha) };
    style.visuals.window_fill = with_alpha(style.visuals.window_fill, window_alpha);
    let panel = style.visuals.panel_fill;
    let panel_alpha = if reduce { 255 } else { levels.alpha(levels.panels, panel.to_srgba_unmultiplied()[3]) };
    style.visuals.panel_fill = with_alpha(panel, panel_alpha);

    accent_style(style, accent);
    modern_text_styles(style);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.interact_size.y = 24.0;
}

/// Translucent style looking like frosted glass. Best used with a backdrop effect in run_native.
pub fn glass_style(style: &mut Style, tint: Option<Color32>) {
    let dark = style.visuals.dark_mode;
    // white glass on light theme, smoked glass on dark theme
    let base = tint.unwrap_or(if dark {
        Color32::from_rgb(20, 22, 30)
    } else {
        Color32::from_rgb(250, 250, 255)
    });
    let with_alpha = |color: Color32, alpha: u8| {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    };
    let highlight = |alpha: u8| Color32::from_rgba_unmultiplied(255, 255, 255, alpha);

    let visuals = &mut style.visuals;

    visuals.panel_fill = with_alpha(base, if dark { 70 } else { 110 });
    visuals.window_fill = with_alpha(base, if dark { 150 } else { 180 });
    visuals.window_stroke = Stroke::new(1.0, highlight(if dark { 45 } else { 180 }));
    visuals.window_corner_radius = CornerRadius::same(16);
    visuals.menu_corner_radius = CornerRadius::same(12);
    visuals.window_shadow = Shadow {
        offset: [0, 8],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 45 }),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 35 }),
    };
    visuals.extreme_bg_color = if dark {
        Color32::from_black_alpha(70)
    } else {
        highlight(130)
    };
    visuals.text_edit_bg_color = Some(visuals.extreme_bg_color);
    visuals.code_bg_color = if dark { highlight(18) } else { highlight(110) };
    visuals.faint_bg_color = if dark { highlight(8) } else { highlight(50) };

    let widgets = &mut visuals.widgets;
    let (fill, hover, active) = if dark { (22, 40, 60) } else { (90, 150, 210) };

    widgets.noninteractive.bg_fill = with_alpha(base, 0);
    widgets.noninteractive.weak_bg_fill = with_alpha(base, 0);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, highlight(if dark { 30 } else { 120 }));

    for (widget, alpha, stroke_alpha) in [
        (&mut widgets.inactive, fill, if dark { 35 } else { 170 }),
        (&mut widgets.hovered, hover, if dark { 70 } else { 230 }),
        (&mut widgets.active, active, if dark { 90 } else { 255 }),
        (&mut widgets.open, active, if dark { 90 } else { 255 }),
    ] {
        widget.bg_fill = highlight(alpha);
        widget.weak_bg_fill = highlight(alpha);
        widget.bg_stroke = Stroke::new(1.0, highlight(stroke_alpha));
        widget.corner_radius = CornerRadius::same(10);
        widget.expansion = 0.0;
    }

    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.window_margin = 12.0.into();
    // the menus are rounded by 12: their items keep away from the rounded corners
    style.spacing.menu_margin = egui::Margin::symmetric(10, 8);
}


/// The three glasses of a window: the sidebar, the content that is inactive (bands, bars of tabs,
/// inactive tabs, headers of columns) and the content that is active (the active tab and its pane)
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Sidebar = 0,
    Inactive = 1,
    Active = 2,
}

/// What the user chose for each layer: a tint and an opacity, None for the default ones
/// (the light theme, then the dark one): the two themes have their own settings
pub static LAYERS: Mutex<[[(Option<Color32>, Option<f32>); 3]; 2]> = Mutex::new([[(None, None); 3]; 2]);

pub fn set_layer(dark: bool, layer: Layer, tint: Option<Color32>, opacity: Option<f32>) {
    if let Ok(mut layers) = LAYERS.lock() {
        layers[dark as usize][layer as usize] = (tint, opacity);
    }
}

/// The tint and the opacity of a layer: the chosen ones, or the defaults of the theme
pub fn layer_values(layer: Layer, dark: bool) -> (Color32, f32) {
    let (tint, opacity) = LAYERS.lock().map(|l| l[dark as usize][layer as usize]).unwrap_or((None, None));
    let (default_tint, default_opacity) = match (layer, dark) {
        (Layer::Sidebar, false) => (Color32::from_rgb(240, 240, 246), 0.55),
        (Layer::Sidebar, true) => (Color32::from_rgb(24, 26, 34), 0.55),
        (Layer::Inactive, false) => (Color32::from_rgb(232, 232, 238), 0.5),
        (Layer::Inactive, true) => (Color32::from_rgb(28, 30, 38), 0.5),
        (Layer::Active, false) => (Color32::from_rgb(251, 251, 253), 0.9),
        (Layer::Active, true) => (Color32::from_rgb(36, 38, 46), 0.9),
    };
    (tint.unwrap_or(default_tint), opacity.unwrap_or(default_opacity))
}

/// The fill of a layer: its tint and its opacity, opaque when the window has no glass
pub fn layer_fill(layer: Layer, visuals: &egui::Visuals) -> Color32 {
    let (tint, opacity) = layer_values(layer, visuals.dark_mode);
    let glass = theme_state().glass.is_some() && !reduce_transparency();
    let alpha = if glass { (opacity.clamp(0.0, 1.0) * 255.0).round() as u8 } else { 255 };
    Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), alpha)
}

/// The tint of the bands, of the tab bars of the docks and of the headers of columns: the glass of
/// the inactive content
pub fn bar_tint(visuals: &egui::Visuals) -> Color32 {
    layer_fill(Layer::Inactive, visuals)
}
