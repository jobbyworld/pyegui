//! A section of a sidebar that folds: a small title (Favoris, Emplacements, Étiquettes in the Finder) that a click
//! folds or opens, a chevron at its right (shown while the title is under the pointer, by default), the content of
//! the section under it when it is open, and a gap after it. The look of finder-rs is the default.
//!
//! The look is given by tokens kept for each theme, like the ones of the tabs and grids: `section.text` (the color
//! of the title), `section.font_size`, `section.height` (the row of the title), `section.gap` (the space after a
//! section), `section.uppercase` (the title in capitals), `section.chevron` (`"hover"`, `"always"` or `"none"`) and
//! `section.chevron_size`. The order of lookup is: the token of the theme, then the default drawn from the theme.
//!
//! The open state is the caller's ([`show`], with a `&mut bool` it keeps and saves) or kept by egui in its
//! persisted memory ([`show_remembered`], by an id).

use std::sync::RwLock;

use egui::{Color32, Id, Rect, Response, Sense, TextStyle, Ui};

/// The SVG chevrons of `examples/icons`, drawn tinted (the same uri as the glyphs of finder-rs)
const CHEVRON_DOWN: &[u8] = include_bytes!("../../examples/icons/chevron-down.svg");
const CHEVRON_RIGHT: &[u8] = include_bytes!("../../examples/icons/chevron-right.svg");

/// When the chevron of a section is drawn
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum SectionChevron {
    /// while the title is under the pointer (the Finder of macOS)
    #[default]
    Hover,
    Always,
    None,
}

impl SectionChevron {
    pub fn parse(name: &str) -> Option<SectionChevron> {
        Some(match name {
            "hover" => SectionChevron::Hover,
            "always" => SectionChevron::Always,
            "none" => SectionChevron::None,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            SectionChevron::Hover => "hover",
            SectionChevron::Always => "always",
            SectionChevron::None => "none",
        }
    }
}

/// What the user gave for one theme; nothing set means the default
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct SectionSet {
    pub text: Option<Color32>,
    pub font_size: Option<f32>,
    pub height: Option<f32>,
    pub gap: Option<f32>,
    pub uppercase: Option<bool>,
    pub chevron: Option<SectionChevron>,
    pub chevron_size: Option<f32>,
}

const EMPTY: SectionSet = SectionSet { text: None, font_size: None, height: None, gap: None, uppercase: None, chevron: None, chevron_size: None };

/// `[theme]`: 0 is light, 1 dark
static TOKENS: RwLock<[SectionSet; 2]> = RwLock::new([EMPTY; 2]);

/// The tokens given for a theme
pub fn tokens(dark: bool) -> SectionSet {
    TOKENS.read().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]
}

/// Changes the tokens of a theme
pub fn update_tokens(dark: bool, change: impl FnOnce(&mut SectionSet)) {
    change(&mut TOKENS.write().unwrap_or_else(|e| e.into_inner())[usize::from(dark)]);
}

/// Forgets every token
pub fn reset_tokens() {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = [EMPTY; 2];
}

/// Every value, with the defaults where no token is given
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionLook {
    pub text: Color32,
    pub font_size: f32,
    pub height: f32,
    pub gap: f32,
    pub uppercase: bool,
    pub chevron: SectionChevron,
    pub chevron_size: f32,
}

/// The look for this style: the defaults are the ones of finder-rs (the `Small` text in the weak color, a row of
/// 22, a gap of 12, a chevron of 16 on hover)
pub fn look(style: &egui::Style) -> SectionLook {
    let t = tokens(style.visuals.dark_mode);
    SectionLook {
        text: t.text.unwrap_or(style.visuals.weak_text_color()),
        font_size: t.font_size.unwrap_or_else(|| TextStyle::Small.resolve(style).size),
        height: t.height.unwrap_or(22.0),
        gap: t.gap.unwrap_or(12.0),
        uppercase: t.uppercase.unwrap_or(false),
        chevron: t.chevron.unwrap_or_default(),
        chevron_size: t.chevron_size.unwrap_or(16.0),
    }
}

/// The chevron of a section or of a row: an SVG tinted with `color`, down when open, to the right when folded
pub fn paint_chevron(ui: &Ui, center: egui::Pos2, size: f32, open: bool, color: Color32) {
    let (name, bytes) = if open { ("chevron-down", CHEVRON_DOWN) } else { ("chevron-right", CHEVRON_RIGHT) };
    egui::Image::from_bytes(format!("bytes://glyph/{name}.svg"), bytes)
        .tint(color)
        .fit_to_exact_size(egui::vec2(size, size))
        .paint_at(ui, Rect::from_center_size(center, egui::vec2(size, size)));
}

/// The title of a section, as wide as the ui: draws it and returns its response (a click on it should fold or
/// open the section, see [`show`])
pub fn header(ui: &mut Ui, title: &str, open: bool) -> Response {
    let look = look(ui.style());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), look.height), Sense::click());
    let text = if look.uppercase { title.to_uppercase() } else { title.to_owned() };
    let font = egui::FontId::new(look.font_size, TextStyle::Small.resolve(ui.style()).family);
    let chevron = match look.chevron {
        SectionChevron::Always => true,
        SectionChevron::Hover => response.hovered(),
        SectionChevron::None => false,
    };
    if ui.is_rect_visible(rect) {
        // the title is cut before the chevron
        let right = if look.chevron == SectionChevron::None { rect.right() } else { rect.right() - look.chevron_size - 6.0 };
        let galley = ui.painter().layout_no_wrap(text.clone(), font, look.text);
        let at = egui::pos2(rect.left() + 2.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))).galley(at, galley, look.text);
        if chevron {
            paint_chevron(ui, egui::pos2(rect.right() - 14.0, rect.center().y), look.chevron_size, open, look.text);
        }
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, &text));
    response
}

/// What [`show`] gives back
pub struct SectionOutput<R> {
    /// the section is open (after the click of this frame)
    pub open: bool,
    /// the response of the title
    pub header: Response,
    /// what the content returned, None when folded
    pub inner: Option<R>,
}

/// A section: its title, then `add_contents` when it is open, then the gap of the sections. A click on the title
/// changes `open`.
pub fn show<R>(ui: &mut Ui, open: &mut bool, title: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> SectionOutput<R> {
    let header = header(ui, title, *open);
    if header.clicked() {
        *open = !*open;
    }
    let inner = open.then(|| add_contents(ui));
    let gap = look(ui.style()).gap;
    ui.add_space(gap);
    SectionOutput { open: *open, header, inner }
}

/// Whether the section `id` is open, as kept by egui ([`show_remembered`]), `default` if it never changed
pub fn is_open(ctx: &egui::Context, id: Id, default: bool) -> bool {
    ctx.data_mut(|d| *d.get_persisted_mut_or(id, default))
}

/// Opens or folds the section `id` kept by egui
pub fn set_open(ctx: &egui::Context, id: Id, open: bool) {
    ctx.data_mut(|d| d.insert_persisted(id, open));
}

/// A section whose open state is kept by egui (in its persisted memory) under `id`
pub fn show_remembered<R>(ui: &mut Ui, id: Id, title: &str, default_open: bool, add_contents: impl FnOnce(&mut Ui) -> R) -> SectionOutput<R> {
    let mut open = is_open(ui.ctx(), id, default_open);
    let out = show(ui, &mut open, title, add_contents);
    if out.header.clicked() {
        set_open(ui.ctx(), id, open);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TOKENS_TEST_LOCK as LOCK;

    #[test]
    fn defaults_are_the_ones_of_finder_rs_and_a_token_wins() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        let style = egui::Style::default();
        let l = look(&style);
        assert_eq!(l.height, 22.0);
        assert_eq!(l.gap, 12.0);
        assert_eq!(l.chevron, SectionChevron::Hover);
        assert_eq!(l.chevron_size, 16.0);
        assert_eq!(l.text, style.visuals.weak_text_color());
        assert_eq!(l.font_size, TextStyle::Small.resolve(&style).size);
        assert!(!l.uppercase);
        update_tokens(style.visuals.dark_mode, |t| {
            t.uppercase = Some(true);
            t.gap = Some(4.0);
            t.chevron = SectionChevron::parse("always");
        });
        let l = look(&style);
        assert!(l.uppercase);
        assert_eq!(l.gap, 4.0);
        assert_eq!(l.chevron, SectionChevron::Always);
        reset_tokens();
        assert_eq!(look(&style).gap, 12.0);
    }

    #[test]
    fn a_folded_section_does_not_draw_its_content() {
        let ctx = egui::Context::default();
        let mut open = false;
        let mut drawn = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out = show(ui, &mut open, "Favoris", |_| drawn = true);
            assert!(!out.open && out.inner.is_none());
        });
        output.textures_delta.clear();
        assert!(!drawn);
        let mut open = true;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out = show(ui, &mut open, "Favoris", |_| 3);
            assert_eq!(out.inner, Some(3));
        });
        output.textures_delta.clear();
    }
}
