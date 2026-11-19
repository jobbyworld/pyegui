//! `markdown_to_pdf`: a markdown text turned into a PDF with the look of `markdown()`.
//!
//! The markdown is drawn by egui, off screen, with the same code as the viewer (`egui_commonmark`, mermaid
//! diagrams included). What egui drew (text as positioned glyphs, rectangles, lines, images) is cut into pages
//! between two rows of text and written with the writer of `craft-pdf`. The cover, the table of contents and
//! the header and footer are drawn by egui too, in the same context, so a theme is a set of egui visuals.
//!
//! The headings are read with `pulldown-cmark` and found again in the shapes by their text: they give the
//! bookmarks and the table of contents, and the links of the text become link annotations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use craft_pdf::write::{self as w, backend as k};
use eframe::egui::{self, epaint, Color32, FontFamily, FontId, Pos2, Rect, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Options as MdOptions, Parser, Tag, TagEnd};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

/// PDF points per egui point (egui points are CSS pixels: 96 per inch, a PDF point is 1/72 inch)
const SCALE: f32 = 0.75;
/// textures (images, diagrams) are made at this density: sharp in print
const DENSITY: f32 = 2.0;

type Textures = HashMap<egui::TextureId, Arc<egui::ColorImage>>;

// ---- options -----------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Geo {
    w: f32,
    h: f32,
    margin: f32,
}

impl Geo {
    /// width of the text, in egui points
    fn content_w(&self) -> f32 {
        (self.w - 2.0 * self.margin) / SCALE
    }
    /// height of the text of a page, in egui points
    fn content_h(&self) -> f32 {
        (self.h - 2.0 * self.margin) / SCALE
    }
}

#[derive(Clone)]
struct Theme {
    dark: bool,
    /// the paper, when it is not white
    background: Option<Color32>,
    /// black text and no tint: for a printer
    print: bool,
}

impl Theme {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "light" => Some(Self { dark: false, background: None, print: false }),
            "dark" => Some(Self { dark: true, background: Some(Color32::from_gray(27)), print: false }),
            "print" => Some(Self { dark: false, background: None, print: true }),
            _ => None,
        }
    }

    fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        if self.print {
            visuals.widgets.noninteractive.fg_stroke.color = Color32::BLACK;
            visuals.widgets.active.fg_stroke.color = Color32::BLACK;
            visuals.hyperlink_color = Color32::from_rgb(0, 51, 153);
        }
        visuals
    }

    fn mermaid(&self) -> &'static str {
        if self.dark {
            "dark"
        } else {
            "default"
        }
    }
}

#[derive(Default, Clone)]
struct Cover {
    title: Option<String>,
    subtitle: Option<String>,
    author: Option<String>,
    date: Option<String>,
}

/// left, center and right texts of a header or a footer, with {title}, {page} and {pages}
#[derive(Default, Clone)]
struct Bar {
    left: String,
    center: String,
    right: String,
}

struct Options {
    theme: Theme,
    geo: Geo,
    title: Option<String>,
    author: Option<String>,
    cover: Option<Cover>,
    toc: Option<(String, u8)>,
    bookmarks: bool,
    header: Option<Bar>,
    footer: Option<Bar>,
    base_dir: Option<PathBuf>,
}

// ---- the offscreen egui ------------------------------------------------------------------------------

struct Rendered {
    shapes: Vec<epaint::ClippedShape>,
    height: f32,
}

struct Env {
    ctx: egui::Context,
    textures: Textures,
}

impl Env {
    fn new(theme: &Theme) -> Self {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        ctx.set_pixels_per_point(DENSITY);
        let which = if theme.dark { egui::Theme::Dark } else { egui::Theme::Light };
        ctx.set_theme(which);
        ctx.set_visuals_of(which, theme.visuals());
        Self { ctx, textures: Textures::new() }
    }

    /// Runs `f` in an ui of this width. With `wait`, the frame is run again until nothing changes any more:
    /// the loaders of images work in threads.
    fn render(&mut self, width: f32, wait: bool, mut f: impl FnMut(&mut egui::Ui)) -> Rendered {
        let mut height = 0.0;
        let mut shapes = Vec::new();
        let mut quiet = 0;
        let (passes, patience) = if wait { (200, 6) } else { (2, 2) };
        for _ in 0..passes {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 200_000.0))),
                ..Default::default()
            };
            let before = (height, self.textures.len());
            let mut output = self.ctx.run_ui(input, |ui| {
                f(ui);
                height = ui.min_rect().height();
            });
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    if let (None, egui::ImageData::Color(image)) = (delta.pos, &delta.image) {
                        self.textures.insert(*id, image.clone());
                    }
                }
            }
            output.textures_delta.clear();
            shapes = output.shapes;
            let again = output.viewport_output.values().any(|v| v.repaint_delay.is_zero());
            if again || before != (height, self.textures.len()) {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet >= patience {
                    break;
                }
            }
            if wait {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
        Rendered { shapes, height }
    }

    /// Three texts on one line: left, centered and right.
    fn bar(&mut self, width: f32, slots: [&str; 3], size: f32) -> Rendered {
        self.render(width, false, |ui| {
            let color = ui.visuals().weak_text_color();
            for (i, text) in slots.iter().enumerate() {
                if text.is_empty() {
                    continue;
                }
                let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(size), color);
                let w = galley.size().x;
                let x = match i {
                    0 => 0.0,
                    1 => (width - w) / 2.0,
                    _ => width - w,
                };
                ui.painter().galley(Pos2::new(x, 0.0), galley, color);
            }
        })
    }
}

// ---- headings and links of the text ------------------------------------------------------------------

struct Heading {
    level: u8,
    title: String,
    /// top and bottom in the coordinates of egui, once found in the shapes
    span: Option<(f32, f32)>,
}

struct Link {
    text: String,
    url: String,
    span: Option<Rect>,
}

fn parse_markdown(text: &str) -> (Vec<Heading>, Vec<Link>) {
    let mut headings = Vec::new();
    let mut links = Vec::new();
    let mut heading: Option<Heading> = None;
    let mut link: Option<Link> = None;
    let options = MdOptions::ENABLE_TABLES | MdOptions::ENABLE_STRIKETHROUGH | MdOptions::ENABLE_TASKLISTS;
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(Heading { level: level as u8, title: String::new(), span: None })
            }
            Event::End(TagEnd::Heading(_)) => headings.extend(heading.take()),
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some(Link { text: String::new(), url: dest_url.to_string(), span: None })
            }
            Event::End(TagEnd::Link) => links.extend(link.take()),
            Event::Text(t) | Event::Code(t) => {
                if let Some(h) = heading.as_mut() {
                    h.title.push_str(&t);
                }
                if let Some(l) = link.as_mut() {
                    l.text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(h) = heading.as_mut() {
                    h.title.push(' ');
                }
                if let Some(l) = link.as_mut() {
                    l.text.push(' ');
                }
            }
            _ => {}
        }
    }
    (headings, links)
}

fn text_shapes<'a>(shape: &'a epaint::Shape, out: &mut Vec<&'a epaint::TextShape>) {
    match shape {
        epaint::Shape::Vec(v) => v.iter().for_each(|s| text_shapes(s, out)),
        epaint::Shape::Text(t) => out.push(t),
        _ => {}
    }
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The first run of consecutive texts, from `from`, that make `wanted` together (spaces ignored: a heading
/// with a part in bold or in code is drawn as several texts). Returns the first and the last index.
fn find_run(
    texts: &[&epaint::TextShape],
    squeezed: &[String],
    from: usize,
    wanted: &str,
    accept: impl Fn(&epaint::TextShape) -> bool,
) -> Option<(usize, usize)> {
    if wanted.is_empty() {
        return None;
    }
    for i in from..texts.len() {
        if !wanted.starts_with(squeezed[i].as_str()) || squeezed[i].is_empty() || !accept(texts[i]) {
            continue;
        }
        let mut acc = String::new();
        for j in i..texts.len().min(i + 24) {
            acc.push_str(&squeezed[j]);
            if acc == wanted {
                return Some((i, j));
            }
            if !wanted.starts_with(&acc) {
                break;
            }
        }
    }
    None
}

/// Finds the shapes of the headings and of the links, in the order of the document.
fn locate(shapes: &[epaint::ClippedShape], headings: &mut [Heading], links: &mut [Link], link_color: Color32) {
    let mut texts = Vec::new();
    shapes.iter().for_each(|s| text_shapes(&s.shape, &mut texts));
    let squeezed: Vec<String> = texts.iter().map(|t| squeeze(&t.galley.job.text)).collect();

    let mut cursor = 0;
    for heading in headings.iter_mut() {
        if let Some((i, j)) = find_run(&texts, &squeezed, cursor, &squeeze(&heading.title), |_| true) {
            let run = &texts[i..=j];
            let top = run.iter().map(|t| t.pos.y).fold(f32::MAX, f32::min);
            let bottom = run.iter().map(|t| t.pos.y + t.galley.size().y).fold(f32::MIN, f32::max);
            heading.span = Some((top, bottom));
            cursor = j + 1;
        }
    }

    let is_link = |t: &epaint::TextShape| {
        t.fallback_color == link_color
            || t.override_text_color == Some(link_color)
            || t.galley.job.sections.iter().any(|s| s.format.color == link_color)
    };
    let mut cursor = 0;
    for link in links.iter_mut() {
        if let Some((i, j)) = find_run(&texts, &squeezed, cursor, &squeeze(&link.text), is_link) {
            let rect = texts[i..=j]
                .iter()
                .map(|t| Rect::from_min_size(t.pos, t.galley.size()))
                .reduce(|a, b| a.union(b));
            link.span = rect;
            cursor = j + 1;
        }
    }
}

// ---- fonts -------------------------------------------------------------------------------------------

struct Fonts {
    defs: egui::FontDefinitions,
    pdf: HashMap<String, k::text::Font>,
    shapers: HashMap<String, harfrust::ShaperData>,
}

impl Fonts {
    fn new() -> Self {
        Self { defs: egui::FontDefinitions::default(), pdf: HashMap::new(), shapers: HashMap::new() }
    }

    /// The glyphs of a text in a font, as egui shaped them (it uses the same shaper): a ligature such as "fi"
    /// is one glyph that stands for two characters, which the character to glyph table of the font cannot say.
    fn shape(&mut self, name: &str, text: &str) -> Option<Vec<k::text::KrillaGlyph>> {
        let data = self.defs.font_data.get(name)?.clone();
        let bytes: &[u8] = (*data.font).as_ref();
        let font = harfrust::FontRef::from_index(bytes, data.index).ok()?;
        let upem = ttf_parser::Face::parse(bytes, data.index).ok()?.units_per_em() as f32;
        let shaper_data = self.shapers.entry(name.to_string()).or_insert_with(|| harfrust::ShaperData::new(&font));
        let shaper = shaper_data.shaper(&font).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
        let (infos, positions) = (shaped.glyph_infos(), shaped.glyph_positions());
        let glyphs = infos
            .iter()
            .zip(positions)
            .map(|(info, pos)| {
                let start = info.cluster as usize;
                let end = infos.iter().map(|i| i.cluster as usize).filter(|c| *c > start).min().unwrap_or(text.len());
                k::text::KrillaGlyph::new(
                    k::text::GlyphId::new(info.glyph_id),
                    pos.x_advance as f32 / upem,
                    pos.x_offset as f32 / upem,
                    pos.y_offset as f32 / upem,
                    0.0,
                    start..end,
                    None,
                )
            })
            .collect();
        Some(glyphs)
    }

    /// The font of the family that has a glyph for `c`, as egui picks it (the first one of the family),
    /// with the id of the glyph. Fonts are embedded once.
    fn resolve(&mut self, family: &FontFamily, c: char) -> Option<(String, k::text::Font, k::text::GlyphId)> {
        let names = self.defs.families.get(family)?.clone();
        for name in names {
            let data = self.defs.font_data.get(&name)?.clone();
            let bytes: &[u8] = (*data.font).as_ref();
            let Ok(face) = ttf_parser::Face::parse(bytes, data.index) else { continue };
            let Some(glyph) = face.glyph_index(c) else { continue };
            if !self.pdf.contains_key(&name) {
                let font = w::font(bytes, data.index, None).ok()?;
                self.pdf.insert(name.clone(), font);
            }
            return Some((name.clone(), self.pdf[&name].clone(), k::text::GlyphId::new(glyph.0 as u32)));
        }
        None
    }
}

// ---- drawing -----------------------------------------------------------------------------------------

fn fill_of(c: Color32) -> k::paint::Fill {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    k::paint::Fill {
        paint: k::color::rgb::Color::new(r, g, b).into(),
        opacity: k::num::NormalizedF32::new(a as f32 / 255.0).unwrap(),
        ..Default::default()
    }
}

fn stroke_of(c: Color32, width: f32) -> k::paint::Stroke {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    k::paint::Stroke {
        paint: k::color::rgb::Color::new(r, g, b).into(),
        width: width * SCALE,
        opacity: k::num::NormalizedF32::new(a as f32 / 255.0).unwrap(),
        ..Default::default()
    }
}

struct Painter<'a, 'b> {
    surface: &'a mut k::surface::Surface<'b>,
    fonts: &'a mut Fonts,
    textures: &'a Textures,
    page_h: f32,
    /// the y of the top of the page in the coordinates of egui
    top: f32,
    /// where the origin of egui lands on the page, in PDF points
    origin: (f32, f32),
    /// what was not drawn, to be reported
    skipped: &'a mut HashMap<String, usize>,
}

impl Painter<'_, '_> {
    fn pt(&self, p: Pos2) -> (f32, f32) {
        (p.x * SCALE + self.origin.0, (p.y - self.top) * SCALE + self.origin.1)
    }

    fn shape(&mut self, shape: &epaint::Shape) {
        use epaint::Shape;
        match shape {
            Shape::Noop => {}
            Shape::Vec(shapes) => shapes.iter().for_each(|s| self.shape(s)),
            Shape::Text(t) => self.text(t),
            Shape::Rect(r) => self.rect(r),
            Shape::LineSegment { points, stroke } => {
                let mut pb = k::geom::PathBuilder::new();
                let (x0, y0) = self.pt(points[0]);
                let (x1, y1) = self.pt(points[1]);
                pb.move_to(x0, y0);
                pb.line_to(x1, y1);
                self.draw_path(pb, None, Some(stroke_of(stroke.color, stroke.width)));
            }
            Shape::Path(p) => {
                if p.points.len() >= 2 {
                    let mut pb = k::geom::PathBuilder::new();
                    for (i, point) in p.points.iter().enumerate() {
                        let (x, y) = self.pt(*point);
                        if i == 0 {
                            pb.move_to(x, y)
                        } else {
                            pb.line_to(x, y)
                        }
                    }
                    if p.closed {
                        pb.close();
                    }
                    let stroke = match p.stroke.color {
                        epaint::ColorMode::Solid(c) if p.stroke.width > 0.0 => Some(stroke_of(c, p.stroke.width)),
                        _ => None,
                    };
                    let fill = (p.fill != Color32::TRANSPARENT).then(|| fill_of(p.fill));
                    self.draw_path(pb, fill, stroke);
                }
            }
            Shape::Circle(c) => {
                let (cx, cy) = self.pt(c.center);
                let r = c.radius * SCALE;
                let mut pb = k::geom::PathBuilder::new();
                ellipse(&mut pb, cx, cy, r, r);
                let stroke = (c.stroke.width > 0.0).then(|| stroke_of(c.stroke.color, c.stroke.width));
                let fill = (c.fill != Color32::TRANSPARENT).then(|| fill_of(c.fill));
                self.draw_path(pb, fill, stroke);
            }
            Shape::Mesh(mesh) => self.mesh(mesh),
            _ => *self.skipped.entry("other shapes".to_string()).or_default() += 1,
        }
    }

    fn draw_path(&mut self, pb: k::geom::PathBuilder, fill: Option<k::paint::Fill>, stroke: Option<k::paint::Stroke>) {
        if fill.is_none() && stroke.is_none() {
            return;
        }
        if let Some(path) = pb.finish() {
            self.surface.set_fill(fill);
            self.surface.set_stroke(stroke);
            self.surface.draw_path(&path);
        }
    }

    fn rect(&mut self, r: &epaint::RectShape) {
        if let Some(brush) = r.brush.as_ref().filter(|b| b.uv != Rect::ZERO) {
            self.image(r.rect.min, r.rect.max, brush.uv.min, brush.uv.max, brush.fill_texture_id);
            return;
        }
        let (x0, y0) = self.pt(r.rect.min);
        let (x1, y1) = self.pt(r.rect.max);
        let c = r.corner_radius;
        let mut pb = k::geom::PathBuilder::new();
        rounded_rect(&mut pb, x0, y0, x1, y1, [c.nw, c.ne, c.se, c.sw].map(|v| v as f32 * SCALE));
        let stroke = (r.stroke.width > 0.0 && r.stroke.color != Color32::TRANSPARENT)
            .then(|| stroke_of(r.stroke.color, r.stroke.width));
        let fill = (r.fill != Color32::TRANSPARENT && r.blur_width <= 0.0).then(|| fill_of(r.fill));
        if r.blur_width > 0.0 {
            *self.skipped.entry("blurred shadows".to_string()).or_default() += 1;
        }
        self.draw_path(pb, fill, stroke);
    }

    fn text(&mut self, t: &epaint::TextShape) {
        let job = &t.galley.job;
        // the chars of the job, without the newlines that the rows leave out
        let mut chars = job.text.char_indices().filter(|(_, c)| *c != '\n');
        let mut section = 0usize;
        for placed in &t.galley.rows {
            let base_y = t.pos.y + placed.pos.y;
            if (base_y - self.top) * SCALE > self.page_h || (base_y + placed.row.size.y - self.top) < -1.0 {
                for _ in &placed.row.glyphs {
                    chars.next();
                }
                continue;
            }
            // a run: consecutive glyphs of one font, one section
            let mut run: Option<Run> = None;
            for glyph in &placed.row.glyphs {
                let Some((byte, ch)) = chars.next() else { break };
                while section + 1 < job.sections.len() && byte >= job.sections[section].byte_range.end.0 {
                    section += 1;
                }
                // the "copy" button of the code blocks
                if ch == '\u{1F5D0}' {
                    continue;
                }
                let format = &job.sections[section].format;
                let mut color = t.override_text_color.unwrap_or(format.color);
                if color == Color32::PLACEHOLDER {
                    color = t.fallback_color;
                }
                let size = format.font_id.size;
                let Some((name, font, id)) = self.fonts.resolve(&format.font_id.family, ch) else {
                    *self
                        .skipped
                        .entry(format!("glyph without a font: {ch:?} U+{:04X}", ch as u32))
                        .or_default() += 1;
                    continue;
                };
                if ch.is_whitespace() && run.is_none() {
                    continue;
                }
                let x = t.pos.x + placed.pos.x + glyph.pos.x;
                let y = base_y + glyph.pos.y;
                let same = run.as_ref().is_some_and(|r| r.name == name && r.section == section && r.size == size);
                if !same {
                    if let Some(r) = run.take() {
                        self.flush(r);
                    }
                    run = Some(Run {
                        name,
                        font,
                        section,
                        size,
                        color,
                        x,
                        y,
                        text: String::new(),
                        glyphs: Vec::new(),
                        underline: format.underline,
                        strike: format.strikethrough,
                        end_x: x,
                        last_x: x,
                    });
                }
                let r = run.as_mut().unwrap();
                // egui places the glyphs itself (kerning included): the advance of the one before is the
                // distance to this one, so the PDF puts every glyph where egui did
                if let Some(previous) = r.glyphs.last_mut() {
                    previous.x_advance = (x - r.last_x) / size;
                }
                r.last_x = x;
                let start = r.text.len();
                r.text.push(ch);
                r.glyphs.push(k::text::KrillaGlyph::new(
                    id,
                    glyph.advance_width / size,
                    0.0,
                    0.0,
                    0.0,
                    start..r.text.len(),
                    None,
                ));
                r.end_x = x + glyph.advance_width;
            }
            if let Some(r) = run.take() {
                self.flush(r);
            }
        }
    }

    fn flush(&mut self, r: Run) {
        let (x, y) = self.pt(Pos2::new(r.x, r.y));
        self.surface.set_fill(Some(fill_of(r.color)));
        self.surface.set_stroke(None);
        // shaped again, like egui did; the glyphs placed one by one are the fallback
        let glyphs = self.fonts.shape(&r.name, &r.text).unwrap_or(r.glyphs);
        self.surface.draw_glyphs(k::geom::Point::from_xy(x, y), &glyphs, r.font, &r.text, r.size * SCALE, false);
        for (line, dy) in [(r.underline, 2.0), (r.strike, -r.size * 0.3)] {
            if line.width > 0.0 {
                let (x0, y0) = self.pt(Pos2::new(r.x, r.y + dy));
                let (x1, y1) = self.pt(Pos2::new(r.end_x, r.y + dy));
                let mut pb = k::geom::PathBuilder::new();
                pb.move_to(x0, y0);
                pb.line_to(x1, y1);
                self.draw_path(pb, None, Some(stroke_of(line.color, line.width)));
            }
        }
    }

    /// An image drawn as a mesh: a quad with the part of its texture it shows.
    fn mesh(&mut self, mesh: &epaint::Mesh) {
        if mesh.vertices.is_empty() || mesh.texture_id == egui::TextureId::Managed(0) {
            return;
        }
        let (mut min, mut max) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
        let (mut uv_min, mut uv_max) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
        for v in &mesh.vertices {
            min = min.min(v.pos);
            max = max.max(v.pos);
            uv_min = uv_min.min(v.uv);
            uv_max = uv_max.max(v.uv);
        }
        self.image(min, max, uv_min, uv_max, mesh.texture_id);
    }

    /// A part of a texture (uv in 0..1) drawn in a rectangle of egui points.
    fn image(&mut self, min: Pos2, max: Pos2, uv_min: Pos2, uv_max: Pos2, texture: egui::TextureId) {
        let Some(image) = self.textures.get(&texture) else {
            *self.skipped.entry(format!("image without a texture: {texture:?}")).or_default() += 1;
            return;
        };
        let [iw, ih] = image.size;
        let x0 = (uv_min.x * iw as f32).round().clamp(0.0, iw as f32) as usize;
        let x1 = (uv_max.x * iw as f32).round().clamp(0.0, iw as f32) as usize;
        let y0 = (uv_min.y * ih as f32).round().clamp(0.0, ih as f32) as usize;
        let y1 = (uv_max.y * ih as f32).round().clamp(0.0, ih as f32) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                rgba.extend_from_slice(&image.pixels[y * iw + x].to_srgba_unmultiplied());
            }
        }
        let mut png = Vec::new();
        let encoded = image::RgbaImage::from_raw((x1 - x0) as u32, (y1 - y0) as u32, rgba)
            .map(|img| img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png));
        if !matches!(encoded, Some(Ok(()))) {
            return;
        }
        let Ok(pdf_image) = k::image::Image::from_png(Arc::new(png).into(), true) else { return };
        let (px0, py0) = self.pt(min);
        let (px1, py1) = self.pt(max);
        let Some(size) = k::geom::Size::from_wh(px1 - px0, py1 - py0) else { return };
        self.surface.push_transform(&k::geom::Transform::from_translate(px0, py0));
        self.surface.draw_image(pdf_image, size);
        self.surface.pop();
    }
}

struct Run {
    name: String,
    font: k::text::Font,
    section: usize,
    size: f32,
    color: Color32,
    x: f32,
    y: f32,
    text: String,
    glyphs: Vec<k::text::KrillaGlyph>,
    underline: epaint::Stroke,
    strike: epaint::Stroke,
    end_x: f32,
    last_x: f32,
}

fn ellipse(pb: &mut k::geom::PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32) {
    const K: f32 = 0.552_284_75;
    pb.move_to(cx + rx, cy);
    pb.cubic_to(cx + rx, cy + K * ry, cx + K * rx, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - K * rx, cy + ry, cx - rx, cy + K * ry, cx - rx, cy);
    pb.cubic_to(cx - rx, cy - K * ry, cx - K * rx, cy - ry, cx, cy - ry);
    pb.cubic_to(cx + K * rx, cy - ry, cx + rx, cy - K * ry, cx + rx, cy);
    pb.close();
}

/// radii: top left, top right, bottom right, bottom left
fn rounded_rect(pb: &mut k::geom::PathBuilder, x0: f32, y0: f32, x1: f32, y1: f32, radii: [f32; 4]) {
    const K: f32 = 0.552_284_75;
    let limit = ((x1 - x0).min(y1 - y0) / 2.0).max(0.0);
    let [tl, tr, br, bl] = radii.map(|r| r.min(limit));
    pb.move_to(x0 + tl, y0);
    pb.line_to(x1 - tr, y0);
    pb.cubic_to(x1 - tr + K * tr, y0, x1, y0 + tr - K * tr, x1, y0 + tr);
    pb.line_to(x1, y1 - br);
    pb.cubic_to(x1, y1 - br + K * br, x1 - br + K * br, y1, x1 - br, y1);
    pb.line_to(x0 + bl, y1);
    pb.cubic_to(x0 + bl - K * bl, y1, x0, y1 - bl + K * bl, x0, y1 - bl);
    pb.line_to(x0, y0 + tl);
    pb.cubic_to(x0, y0 + tl - K * tl, x0 + tl - K * tl, y0, x0 + tl, y0);
    pb.close();
}

// ---- pages -------------------------------------------------------------------------------------------

/// The top of each page, in egui points. A page ends before the first row of text that it would cut, and a
/// heading is never the last thing of a page: it goes to the next one with the text that follows.
fn page_tops(shapes: &[epaint::ClippedShape], height: f32, headings: &[(f32, f32)], page: f32) -> Vec<f32> {
    let mut texts = Vec::new();
    shapes.iter().for_each(|s| text_shapes(&s.shape, &mut texts));
    let mut spans = Vec::new();
    for t in texts {
        for row in &t.galley.rows {
            let top = t.pos.y + row.pos.y;
            spans.push((top, top + row.row.size.y));
        }
    }
    let mut tops = vec![0.0];
    let mut start = 0.0;
    while start + page < height {
        let limit = start + page;
        let mut cut = spans
            .iter()
            .filter(|(top, bottom)| *top < limit - 0.5 && *bottom > limit + 0.5)
            .map(|(top, _)| *top)
            .fold(limit, f32::min);
        while let Some((top, _)) = headings
            .iter()
            .find(|(top, bottom)| *top > start + 1.0 && *bottom <= cut + 1.0 && *bottom >= cut - 36.0)
        {
            cut = *top;
        }
        start = if cut > start + 1.0 { cut } else { limit };
        tops.push(start);
    }
    tops
}

fn page_of(tops: &[f32], y: f32) -> usize {
    tops.iter().rposition(|t| *t <= y + 0.5).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq)]
enum Sec {
    Cover,
    Toc,
    Body,
}

struct PageSpec {
    sec: Sec,
    top: f32,
    /// the first y that is not on this page
    end: f32,
}

fn specs(sec: Sec, tops: &[f32], page: f32) -> Vec<PageSpec> {
    tops.iter()
        .enumerate()
        .map(|(i, top)| PageSpec { sec, top: *top, end: tops.get(i + 1).copied().unwrap_or(f32::MAX).min(top + page) })
        .collect()
}

type OutlineItem = (u8, String, k::destination::XyzDestination);

fn outline(items: &[OutlineItem]) -> k::outline::Outline {
    fn nest(items: &[OutlineItem], i: &mut usize, level: u8) -> Vec<k::outline::OutlineNode> {
        let mut out = Vec::new();
        while *i < items.len() && items[*i].0 >= level {
            let (lvl, title, dest) = &items[*i];
            *i += 1;
            let mut node = k::outline::OutlineNode::new(title.clone(), dest.clone());
            for child in nest(items, i, *lvl + 1) {
                node.push_child(child);
            }
            out.push(node);
        }
        out
    }
    let mut outline = k::outline::Outline::new();
    let mut i = 0;
    for node in nest(items, &mut i, 1) {
        outline.push_child(node);
    }
    outline
}

fn fill_template(template: &str, title: &str, page: usize, pages: usize) -> String {
    template
        .replace("{title}", title)
        .replace("{page}", &page.to_string())
        .replace("{pages}", &pages.to_string())
}

// ---- the cover and the table of contents -------------------------------------------------------------

fn draw_cover(ui: &mut egui::Ui, cover: &Cover, title: &str, geo: &Geo) {
    let accent = ui.visuals().hyperlink_color;
    ui.add_space(geo.content_h() * 0.30);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(38.0).strong());
        ui.add_space(14.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0, 3.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.5, accent);
        ui.add_space(18.0);
        if let Some(subtitle) = &cover.subtitle {
            ui.label(RichText::new(subtitle).size(19.0).weak());
            ui.add_space(8.0);
        }
    });
    ui.add_space(geo.content_h() * 0.30);
    ui.vertical_centered(|ui| {
        for line in [&cover.author, &cover.date].into_iter().flatten() {
            ui.label(RichText::new(line).size(15.0));
        }
    });
}

const TOC_ROW: f32 = 24.0;

fn draw_toc(ui: &mut egui::Ui, width: f32, title: &str, entries: &[(u8, String, usize)], rects: &mut Vec<Rect>) {
    rects.clear();
    ui.label(RichText::new(title).size(30.0));
    ui.add_space(14.0);
    for (level, text, page) in entries {
        let indent = (*level as f32 - 1.0) * 20.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, TOC_ROW), egui::Sense::hover());
        rects.push(rect);
        let color = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let size = if *level == 1 { 16.0 } else { 14.5 };
        let number = ui.painter().layout_no_wrap(page.to_string(), FontId::proportional(size), color);
        let room = (width - indent - number.size().x - 30.0).max(50.0);
        let mut job = egui::text::LayoutJob::simple(text.clone(), FontId::proportional(size), color, room);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let title_galley = ui.fonts_mut(|f| f.layout_job(job));
        let y = rect.top() + (TOC_ROW - title_galley.size().y) / 2.0;
        let title_w = title_galley.size().x;
        ui.painter().galley(Pos2::new(rect.left() + indent, y), title_galley, color);
        // the leader dots between the title and the number
        let dot = ui.painter().layout_no_wrap(". ".to_string(), FontId::proportional(size), weak);
        let from = rect.left() + indent + title_w + 8.0;
        let to = rect.right() - number.size().x - 8.0;
        let count = ((to - from) / dot.size().x).floor().max(0.0) as usize;
        if count > 0 {
            let dots = ui.painter().layout_no_wrap(". ".repeat(count), FontId::proportional(size), weak);
            ui.painter().galley(Pos2::new(from, y), dots, weak);
        }
        ui.painter().galley(Pos2::new(rect.right() - number.size().x, y), number, color);
    }
}

// ---- the document ------------------------------------------------------------------------------------

type Skipped = Vec<(String, usize)>;

fn build(text: &str, o: &Options) -> Result<(Vec<u8>, usize, Skipped), String> {
    let geo = o.geo;
    let mut env = Env::new(&o.theme);
    let (mut headings, mut links) = parse_markdown(text);

    // the body
    let body_text = crate::markdown::replace_mermaid(&env.ctx, text, o.theme.mermaid()).into_owned();
    let mut cache = CommonMarkCache::default();
    let scheme = o.base_dir.as_ref().map(|d| {
        let path = d.to_string_lossy().replace('\\', "/");
        let mut s = if path.starts_with('/') { format!("file://{path}") } else { format!("file:///{path}") };
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    });
    let body = env.render(geo.content_w(), true, |ui| {
        let mut viewer = CommonMarkViewer::new();
        if let Some(scheme) = &scheme {
            viewer = viewer.default_implicit_uri_scheme(scheme.clone());
        }
        viewer.show(ui, &mut cache, &body_text);
    });
    let link_color = env.ctx.global_style().visuals.hyperlink_color;
    locate(&body.shapes, &mut headings, &mut links, link_color);

    let found: Vec<(f32, f32)> = headings.iter().filter_map(|h| h.span).collect();
    let body_tops = page_tops(&body.shapes, body.height, &found, geo.content_h());

    let title = o
        .title
        .clone()
        .or_else(|| headings.iter().find(|h| h.level == 1).map(|h| h.title.clone()))
        .unwrap_or_default();

    // the cover
    let cover = o.cover.as_ref().map(|c| {
        let cover_title = c.title.clone().unwrap_or_else(|| title.clone());
        env.render(geo.content_w(), false, |ui| draw_cover(ui, c, &cover_title, &geo))
    });
    let cover_pages = cover.as_ref().map_or(0, |_| 1);

    // the table of contents: drawn once to count its pages, then with the numbers of the pages
    let toc_entries: Vec<&Heading> = headings
        .iter()
        .filter(|h| o.toc.as_ref().is_some_and(|(_, depth)| h.level <= *depth) && h.span.is_some())
        .collect();
    let mut toc_rects = Vec::new();
    let mut toc = None;
    let mut toc_tops = Vec::new();
    if let Some((toc_title, _)) = &o.toc {
        let probe: Vec<(u8, String, usize)> = toc_entries.iter().map(|h| (h.level, h.title.clone(), 0)).collect();
        let first = env.render(geo.content_w(), false, |ui| draw_toc(ui, geo.content_w(), toc_title, &probe, &mut toc_rects));
        toc_tops = page_tops(&first.shapes, first.height, &[], geo.content_h());
        let offset = cover_pages + toc_tops.len();
        let entries: Vec<(u8, String, usize)> = toc_entries
            .iter()
            .map(|h| (h.level, h.title.clone(), offset + page_of(&body_tops, h.span.unwrap().0) + 1))
            .collect();
        toc = Some(env.render(geo.content_w(), false, |ui| draw_toc(ui, geo.content_w(), toc_title, &entries, &mut toc_rects)));
    }
    let offset = cover_pages + toc_tops.len();

    // the pages, in order
    let mut pages: Vec<PageSpec> = Vec::new();
    if cover.is_some() {
        pages.push(PageSpec { sec: Sec::Cover, top: 0.0, end: f32::MAX });
    }
    if toc.is_some() {
        pages.extend(specs(Sec::Toc, &toc_tops, geo.content_h()));
    }
    pages.extend(specs(Sec::Body, &body_tops, geo.content_h()));
    let total = pages.len();

    let mut skipped = HashMap::new();
    let bytes = w::guard::<_, w::WriteError>(|| {
        let metadata = w::Metadata {
            creator: "pyegui".into(),
            producer: "pyegui".into(),
            title: (!title.is_empty()).then(|| title.clone()),
            authors: o.author.iter().cloned().collect(),
            ..Default::default()
        };
        let mut writer = w::Writer::new(k::SerializeSettings::default(), metadata);
        let mut fonts = Fonts::new();

        let dest = |y: f32| {
            let page = page_of(&body_tops, y);
            let local = ((y - body_tops[page]) * SCALE + geo.margin - 6.0).max(0.0);
            k::destination::XyzDestination::new(offset + page, k::geom::Point::from_xy(geo.margin, local))
        };

        if o.bookmarks {
            let items: Vec<OutlineItem> = headings
                .iter()
                .filter_map(|h| h.span.map(|(top, _)| (h.level, h.title.clone(), dest(top))))
                .collect();
            if !items.is_empty() {
                writer.document_mut().set_outline(outline(&items));
            }
        }

        for (index, spec) in pages.iter().enumerate() {
            let rendered = match spec.sec {
                Sec::Cover => cover.as_ref().unwrap(),
                Sec::Toc => toc.as_ref().unwrap(),
                Sec::Body => &body,
            };
            let document = writer.document_mut();
            let mut page = document.start_page_with(k::page::PageSettings::from_wh(geo.w, geo.h).unwrap());
            {
                let mut surface = page.surface();
                // the paper
                if let Some(color) = o.theme.background {
                    let mut pb = k::geom::PathBuilder::new();
                    pb.push_rect(k::geom::Rect::from_xywh(0.0, 0.0, geo.w, geo.h).unwrap());
                    surface.set_fill(Some(fill_of(color)));
                    surface.set_stroke(None);
                    surface.draw_path(&pb.finish().unwrap());
                }
                // the text, clipped to the body of the page
                let mut clip = k::geom::PathBuilder::new();
                let height = ((spec.end - spec.top) * SCALE).min(geo.h - 2.0 * geo.margin);
                clip.push_rect(k::geom::Rect::from_xywh(geo.margin, geo.margin, geo.w - 2.0 * geo.margin, height).unwrap());
                surface.push_clip_path(&clip.finish().unwrap(), &k::paint::FillRule::NonZero);
                let mut painter = Painter {
                    surface: &mut surface,
                    fonts: &mut fonts,
                    textures: &env.textures,
                    page_h: geo.h,
                    top: spec.top,
                    origin: (geo.margin, geo.margin),
                    skipped: &mut skipped,
                };
                for clipped in &rendered.shapes {
                    painter.shape(&clipped.shape);
                }
                surface.pop();

                // header and footer, but not on the cover
                if spec.sec != Sec::Cover {
                    for (bar, y) in [(&o.header, geo.margin * 0.45), (&o.footer, geo.h - geo.margin * 0.55)] {
                        let Some(bar) = bar else { continue };
                        let slots = [&bar.left, &bar.center, &bar.right].map(|s| fill_template(s, &title, index + 1, total));
                        if slots.iter().all(|s| s.is_empty()) {
                            continue;
                        }
                        let strip = env.bar(geo.content_w(), [&slots[0], &slots[1], &slots[2]], 11.0);
                        let mut painter = Painter {
                            surface: &mut surface,
                            fonts: &mut fonts,
                            textures: &env.textures,
                            page_h: geo.h,
                            top: 0.0,
                            origin: (geo.margin, y),
                            skipped: &mut skipped,
                        };
                        for clipped in &strip.shapes {
                            painter.shape(&clipped.shape);
                        }
                    }
                }
                surface.finish();
            }

            // the links of the page
            let on_page = |y: f32| y >= spec.top - 0.5 && y < spec.end;
            let to_pdf = |r: Rect| {
                k::geom::Rect::from_xywh(
                    r.min.x * SCALE + geo.margin,
                    (r.min.y - spec.top) * SCALE + geo.margin,
                    r.width() * SCALE,
                    r.height() * SCALE,
                )
            };
            let mut annotations: Vec<(k::geom::Rect, k::annotation::Target)> = Vec::new();
            match spec.sec {
                Sec::Body => {
                    for link in &links {
                        if let (Some(rect), Some(target)) = (link.span, w::uri_target(&link.url)) {
                            if let (true, Some(r)) = (on_page(rect.min.y), to_pdf(rect)) {
                                annotations.push((r, target));
                            }
                        }
                    }
                }
                Sec::Toc => {
                    for (rect, heading) in toc_rects.iter().zip(&toc_entries) {
                        if let (true, Some(r)) = (on_page(rect.min.y), to_pdf(*rect)) {
                            let target = k::annotation::Target::Destination(k::destination::Destination::Xyz(dest(
                                heading.span.unwrap().0,
                            )));
                            annotations.push((r, target));
                        }
                    }
                }
                Sec::Cover => {}
            }
            for (rect, target) in annotations {
                page.add_annotation(k::annotation::LinkAnnotation::new(rect, target).into());
            }
            page.finish();
        }
        writer.finish()
    })
    .map_err(|e| e.to_string())?;

    let mut skipped: Skipped = skipped.into_iter().collect();
    skipped.sort();
    Ok((bytes, total, skipped))
}

// ---- python ------------------------------------------------------------------------------------------

fn page_size(value: &Bound<'_, PyAny>) -> PyResult<(f32, f32)> {
    const MM: f32 = 72.0 / 25.4;
    if let Ok(name) = value.extract::<String>() {
        return match name.to_lowercase().as_str() {
            "a3" => Ok((297.0 * MM, 420.0 * MM)),
            "a4" => Ok((210.0 * MM, 297.0 * MM)),
            "a5" => Ok((148.0 * MM, 210.0 * MM)),
            "letter" => Ok((612.0, 792.0)),
            "legal" => Ok((612.0, 1008.0)),
            _ => Err(PyValueError::new_err(format!(
                "Unknown page size '{name}', expected \"A3\", \"A4\", \"A5\", \"Letter\", \"Legal\" or a (width, height) in millimeters"
            ))),
        };
    }
    let (w, h): (f32, f32) = value.extract()?;
    Ok((w * MM, h * MM))
}

/// A dict of strings
fn text_dict(value: &Bound<'_, PyAny>, name: &str) -> PyResult<HashMap<String, String>> {
    value
        .extract::<HashMap<String, String>>()
        .map_err(|_| PyValueError::new_err(format!("'{name}' takes a dict of strings")))
}

/// `header` and `footer`: a flag, a text for the center or a dict; None when the key is absent
fn bar_of(value: Option<Bound<'_, PyAny>>, name: &str, default: Option<Bar>) -> PyResult<Option<Bar>> {
    let Some(value) = value else { return Ok(default) };
    if let Ok(flag) = value.extract::<bool>() {
        return Ok(flag.then(|| Bar { center: "{page} / {pages}".into(), ..Default::default() }));
    }
    if let Ok(text) = value.extract::<String>() {
        return Ok(Some(Bar { center: text, ..Default::default() }));
    }
    let d = text_dict(&value, name)?;
    Ok(Some(Bar {
        left: d.get("left").cloned().unwrap_or_default(),
        center: d.get("center").cloned().unwrap_or_default(),
        right: d.get("right").cloned().unwrap_or_default(),
    }))
}

/// Turns a markdown text into a PDF, drawn like `markdown()` is.
///
/// The text is drawn by the same code as the viewer: headings, lists, tables, quotes, code blocks with their
/// colors, links, images and mermaid diagrams. The pages are cut between two lines of text, and a heading
/// is never alone at the bottom of a page. The text stays text (vector, selectable), the headings become
/// the bookmarks of the file and the links become links of the PDF. No window is needed and no ui is
/// involved: it can be called from a thread.
///
/// Args:
///     text (str): the markdown
///
///     path (str | None): the file to write. None: the bytes of the PDF are returned
///
/// Keyword args:
///
///     theme (str): "light" (the default), "dark" (dark pages) or "print" (black text, white paper)
///
///     title (str): title of the document, of its cover and of `{title}` in the header and the footer.
///     Default is the first heading of level 1
///
///     author (str): author written in the properties of the PDF
///
///     cover (dict | bool): a cover page: `dict(title=..., subtitle=..., author=..., date=...)`, every key is
///     optional. True is a cover with the title only
///
///     toc (bool | str | dict): a table of contents with the page of every heading, linked to the pages.
///     A str is its title, a dict is `dict(title="Contents", depth=3)`. Default is no table of contents
///
///     bookmarks (bool): the headings as the bookmarks of the PDF. Default is True
///
///     header (str | dict): text at the top of the pages, or `dict(left=..., center=..., right=...)`
///
///     footer (bool | str | dict): same as `header`. `{page}`, `{pages}` and `{title}` are replaced.
///     Default is "{page} / {pages}" in the center, False for none. No header and no footer on the cover
///
///     page_size (str | tuple): "A3", "A4" (the default), "A5", "Letter", "Legal" or (width, height) in mm
///
///     margin (float): margin of the pages in mm. Default is 20
///
///     base_dir (str): the folder that the relative paths of the images are read from
///
/// Returns:
///     bytes | None: the PDF when `path` is None
///
/// Example::
///
///     markdown_to_pdf(Path("README.md").read_text(encoding="utf-8"), "README.pdf",
///                     theme="light", cover=dict(subtitle="Version 1.0", author="Joachim"),
///                     toc=True, footer="{title}  -  {page} / {pages}", base_dir=".")
#[pyfunction]
#[pyo3(signature = (text, path=None, **kwargs))]
fn markdown_to_pdf<'py>(
    py: Python<'py>,
    text: &str,
    path: Option<String>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let item = |key: &str| -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(match kwargs {
            Some(k) => k.get_item(key)?.filter(|v| !v.is_none()),
            None => None,
        })
    };
    let string = |key: &str| -> PyResult<Option<String>> { crate::kwarg::<String>(kwargs, key) };

    let theme_name = string("theme")?.unwrap_or_else(|| "light".into());
    let theme = Theme::parse(&theme_name).ok_or_else(|| {
        PyValueError::new_err(format!("Unknown theme '{theme_name}', expected \"light\", \"dark\" or \"print\""))
    })?;
    let (w, h) = match item("page_size")? {
        Some(v) => page_size(&v)?,
        None => (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
    };
    let margin = crate::kwarg::<f32>(kwargs, "margin")?.unwrap_or(20.0) * 72.0 / 25.4;
    if margin < 0.0 || margin * 2.0 >= w.min(h) {
        return Err(PyValueError::new_err("'margin' is too big for the page"));
    }

    let cover = match item("cover")? {
        None => None,
        Some(v) => match v.extract::<bool>() {
            Ok(flag) => flag.then(Cover::default),
            Err(_) => {
                let d = text_dict(&v, "cover")?;
                Some(Cover {
                    title: d.get("title").cloned(),
                    subtitle: d.get("subtitle").cloned(),
                    author: d.get("author").cloned(),
                    date: d.get("date").cloned(),
                })
            }
        },
    };
    let toc = match item("toc")? {
        None => None,
        Some(v) => {
            if let Ok(flag) = v.extract::<bool>() {
                flag.then(|| ("Table des matières".to_string(), 3))
            } else if let Ok(title) = v.extract::<String>() {
                Some((title, 3))
            } else {
                let d = v
                    .cast::<PyDict>()
                    .map_err(|_| PyValueError::new_err("'toc' takes a bool, a title or a dict(title=..., depth=...)"))?;
                let title = crate::kwarg::<String>(Some(d), "title")?.unwrap_or_else(|| "Table des matières".into());
                let depth = crate::kwarg::<u8>(Some(d), "depth")?.unwrap_or(3).clamp(1, 6);
                Some((title, depth))
            }
        }
    };
    let footer_default = Bar { center: "{page} / {pages}".into(), ..Default::default() };
    let options = Options {
        theme,
        geo: Geo { w, h, margin },
        title: string("title")?,
        author: string("author")?,
        cover,
        toc,
        bookmarks: crate::kwarg::<bool>(kwargs, "bookmarks")?.unwrap_or(true),
        header: bar_of(item("header")?, "header", None)?,
        footer: bar_of(item("footer")?, "footer", Some(footer_default))?,
        base_dir: string("base_dir")?.map(PathBuf::from),
    };

    let (bytes, _pages, _skipped) = py.detach(|| build(text, &options)).map_err(PyRuntimeError::new_err)?;
    match path {
        Some(path) => {
            std::fs::write(&path, bytes).map_err(|e| PyRuntimeError::new_err(format!("{path}: {e}")))?;
            Ok(None)
        }
        None => Ok(Some(PyBytes::new(py, &bytes))),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(markdown_to_pdf, m)?)
}
