//! The text of a PDF page with where every character is: what search, selection and `PdfDocument.text`
//! read. An app-owned `Device` of `craft-pdf` records the Unicode of every glyph with its box in the
//! coordinates of the page as it is drawn (points, origin at the top left, rotation applied), a space
//! where the glyphs are apart and a newline where the baseline changes or the text jumps back. Lines
//! are not sorted again: the content order is the reading order of nearly every producer, and a
//! geometric sort would interleave the columns.
//!
//! The invisible text of a scanned and recognized page is kept: it is what makes such a page
//! searchable and selectable in every viewer.

use craft_pdf::interpret::font::Glyph;
use craft_pdf::interpret::hayro_cmap::BfString;
use craft_pdf::interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask, TransformExt,
};
use craft_pdf::syntax::Pdf;
use eframe::egui::{Pos2, Rect};
use kurbo::{Affine, BezPath, Point};

/// Most characters kept for a page (a hostile page cannot make the text large).
const MAX_CHARS: usize = 200_000;
/// A gap wider than this fraction of the font size is a word break.
const SPACE_GAP: f64 = 0.2;

/// One character of a page and its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ch {
    pub c: char,
    /// left, top, right, bottom in points
    pub r: [f32; 4],
    /// False for the space and the newline that were added between glyphs
    pub real: bool,
}

impl Ch {
    pub fn rect(&self) -> Rect {
        Rect::from_min_max(Pos2::new(self.r[0], self.r[1]), Pos2::new(self.r[2], self.r[3]))
    }
}

/// The text of a page.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageText {
    pub chars: Vec<Ch>,
    /// the links of the page, read with the text
    pub links: Vec<crate::pdf_nav::Link>,
}

struct Last {
    /// Writing direction (unit vector).
    u: [f64; 2],
    /// Position of the glyph's baseline across the line.
    across: f64,
    /// Where the previous glyph ended along the line.
    end: f64,
    ended_with_space: bool,
}

struct TextDevice {
    chars: Vec<Ch>,
    last: Option<Last>,
}

impl TextDevice {
    fn synthetic(&mut self, c: char) {
        if self.chars.len() >= MAX_CHARS {
            return;
        }
        let r = self.chars.last().map_or([0.0; 4], |p| [p.r[2], p.r[1], p.r[2], p.r[3]]);
        self.chars.push(Ch { c, r, real: false });
    }
}

impl Device<'_> for TextDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_glyph(&mut self, glyph: &Glyph<'_>, transform: Affine, glyph_transform: Affine, _: &Paint<'_>, _: &GlyphDrawMode) {
        if self.chars.len() >= MAX_CHARS {
            return;
        }
        let s = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };
        let s: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
        if s.is_empty() {
            return;
        }
        // glyph space -> page space; outline glyphs use 1000 units per em
        let t = transform * glyph_transform;
        let [a, b, ..] = t.as_coeffs();
        let scale = a.hypot(b);
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let u = [a / scale, b / scale];
        let size = scale * 1000.0;
        let advance = match glyph {
            Glyph::Outline(g) => g.advance_width().filter(|w| w.is_finite() && *w > 0.0).map_or(500.0, f64::from),
            Glyph::Type3(g) => g.advance_width().filter(|w| w.is_finite() && *w > 0.0).map_or(600.0, f64::from),
        };
        let origin = t * Point::new(0.0, 0.0);
        let along = origin.x * u[0] + origin.y * u[1];
        let across = origin.x * -u[1] + origin.y * u[0];
        let end = along + advance * scale;
        let is_space = s.iter().all(|c| c.is_whitespace());
        if !(along.is_finite() && across.is_finite() && end.is_finite()) {
            return;
        }
        match &self.last {
            Some(last) if last.u[0] * u[0] + last.u[1] * u[1] > 0.9 && (across - last.across).abs() <= 0.5 * size && along >= last.end - size => {
                if along - last.end > SPACE_GAP * size && !last.ended_with_space && !is_space {
                    self.synthetic(' ');
                }
            }
            Some(_) if self.chars.last().is_some_and(|c| c.c != '\n') => self.synthetic('\n'),
            Some(_) | None => {}
        }
        // the box of the glyph: from a bit under the baseline to above the capitals
        let count = s.len() as f64;
        for (k, c) in s.into_iter().enumerate() {
            let (x0, x1) = (advance * k as f64 / count, advance * (k as f64 + 1.0) / count);
            let corners = [(x0, -220.0), (x1, -220.0), (x1, 860.0), (x0, 860.0)].map(|(x, y)| t * Point::new(x, y));
            let min_x = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            let min_y = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_y = corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            if ![min_x, max_x, min_y, max_y].iter().all(|v| v.is_finite()) {
                continue;
            }
            self.chars.push(Ch { c, r: [min_x as f32, min_y as f32, max_x as f32, max_y as f32], real: true });
        }
        self.last = Some(Last { u, across, end, ended_with_space: is_space });
    }
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Reads the text of a page (from 0). A page that cannot be interpreted gives an empty text: one bad
/// page does not lose the others.
pub(crate) fn extract_page(pdf: &Pdf, index: usize) -> PageText {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pages = pdf.pages();
        let page = pages.get(index)?;
        let (w, h) = page.render_dimensions();
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut device = TextDevice { chars: Vec::new(), last: None };
        craft_pdf::interpret_page(page, &mut context, &mut device).ok()?;
        Some(device.chars)
    }));
    PageText { chars: result.ok().flatten().unwrap_or_default(), links: Vec::new() }
}

/// A character as search compares it: lower case, without the accents of the Latin letters, a
/// newline as a space.
pub(crate) fn fold(c: char) -> char {
    let c = if c == '\n' || c == '\u{a0}' { ' ' } else { c };
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        '’' | '‘' => '\'',
        '\u{2010}'..='\u{2015}' => '-',
        other => other,
    }
}

impl PageText {
    /// The text of the characters between two boundaries.
    pub fn string(&self, from: usize, to: usize) -> String {
        let to = to.min(self.chars.len());
        self.chars.get(from..to).map_or_else(String::new, |c| c.iter().map(|c| c.c).collect())
    }

    /// The whole text of the page, trimmed.
    pub fn text(&self) -> String {
        self.string(0, self.chars.len()).trim().to_owned()
    }

    /// The places of `query` (already folded) in the page, as `(start, end)` boundaries. A run of
    /// spaces and newlines in the page matches one space of the query.
    pub fn find(&self, query: &[char], accents: bool) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        if query.is_empty() {
            return found;
        }
        let key = |c: char| if accents { let l = if c == '\n' || c == '\u{a0}' { ' ' } else { c }; l.to_lowercase().next().unwrap_or(l) } else { fold(c) };
        let page: Vec<char> = self.chars.iter().map(|c| key(c.c)).collect();
        let n = query.len();
        let mut i = 0;
        while i + n <= page.len() {
            if page[i..i + n] == *query {
                found.push((i, i + n));
                i += n;
            } else {
                i += 1;
            }
        }
        found
    }

    /// The boundary (between two characters, from 0 to the length) nearest to a point of the page, and
    /// the distance of the point to the text it is on, in points.
    pub fn boundary_at(&self, p: Pos2) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32, bool)> = None;
        for (i, ch) in self.chars.iter().enumerate().filter(|(_, c)| c.real) {
            let r = ch.rect();
            let dx = (r.left() - p.x).max(0.0).max(p.x - r.right());
            let dy = (r.top() - p.y).max(0.0).max(p.y - r.bottom());
            // the line counts more than the column: the pointer in the margin of a line picks its end
            let d = dx * 0.5 + dy * 2.0;
            if best.is_none_or(|(_, b, _)| d < b) {
                best = Some((i, d, p.x > r.center().x));
            }
        }
        best.map(|(i, d, after)| (if after { i + 1 } else { i }, d))
    }

    /// The boundaries of the word around a boundary.
    pub fn word_around(&self, at: usize) -> (usize, usize) {
        let word = |i: usize| self.chars.get(i).is_some_and(|c| c.real && (c.c.is_alphanumeric() || c.c == '_'));
        let mut anchor = at.min(self.chars.len().saturating_sub(1));
        if !word(anchor) && anchor > 0 && word(anchor - 1) {
            anchor -= 1;
        }
        if !word(anchor) {
            return (at, at);
        }
        let mut a = anchor;
        while a > 0 && word(a - 1) {
            a -= 1;
        }
        let mut b = anchor + 1;
        while word(b) {
            b += 1;
        }
        (a, b)
    }

    /// The boundaries of the line around a boundary.
    pub fn line_around(&self, at: usize) -> (usize, usize) {
        let at = at.min(self.chars.len());
        let mut a = at;
        while a > 0 && self.chars[a - 1].c != '\n' {
            a -= 1;
        }
        let mut b = at;
        while b < self.chars.len() && self.chars[b].c != '\n' {
            b += 1;
        }
        (a, b)
    }

    /// The rectangles to paint to mark the characters between two boundaries: one per line, the gaps
    /// between the words filled.
    pub fn rects(&self, from: usize, to: usize) -> Vec<Rect> {
        let to = to.min(self.chars.len());
        let mut out: Vec<Rect> = Vec::new();
        let mut line: Option<Rect> = None;
        for ch in self.chars.get(from..to).unwrap_or_default() {
            if ch.c == '\n' {
                out.extend(line.take());
                continue;
            }
            if !ch.real {
                continue;
            }
            let r = ch.rect();
            line = Some(match line {
                // a character far from the line (another baseline) starts a new one
                Some(l) if (r.center().y - l.center().y).abs() > l.height().max(r.height()) * 0.6 => {
                    out.push(l);
                    r
                }
                Some(l) => l.union(r),
                None => r,
            });
        }
        out.extend(line);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> PageText {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut chars = Vec::new();
        for c in text.chars() {
            if c == '\n' {
                chars.push(Ch { c, r: [x, y, x, y + 10.0], real: false });
                x = 0.0;
                y += 12.0;
            } else {
                chars.push(Ch { c, r: [x, y, x + 6.0, y + 10.0], real: c != ' ' });
                x += 6.0;
            }
        }
        PageText { chars, links: Vec::new() }
    }

    fn query(q: &str) -> Vec<char> {
        q.chars().map(fold).collect()
    }

    #[test]
    fn search_ignores_case_and_accents_unless_asked() {
        let p = page("Un Éléphant à côté\nde l'ÉCOLE");
        assert_eq!(p.find(&query("elephant"), false), vec![(3, 11)]);
        assert_eq!(p.find(&query("ecole"), false).len(), 1);
        assert_eq!(p.find(&query("côté de"), false), vec![(14, 21)], "a newline matches a space");
        let exact: Vec<char> = "éléphant".chars().collect();
        assert_eq!(p.find(&exact, true), vec![(3, 11)], "the case is still ignored with accents");
        let plain: Vec<char> = "elephant".chars().collect();
        assert!(p.find(&plain, true).is_empty(), "but an accent is not an e");
        assert!(p.find(&query("zzz"), false).is_empty() && p.find(&[], false).is_empty());
    }

    #[test]
    fn a_point_finds_its_boundary_and_a_word_its_limits() {
        let p = page("bonjour le monde");
        let (b, _) = p.boundary_at(Pos2::new(1.0, 5.0)).unwrap();
        assert_eq!(b, 0, "left half of the first character");
        let (b, _) = p.boundary_at(Pos2::new(5.0, 5.0)).unwrap();
        assert_eq!(b, 1, "right half of the first character");
        let (b, d) = p.boundary_at(Pos2::new(500.0, 5.0)).unwrap();
        assert_eq!(b, p.chars.len(), "past the end of the line: its end");
        assert!(d > 0.0);
        assert_eq!(p.word_around(9), (8, 10));
        assert_eq!(p.word_around(0), (0, 7));
        assert_eq!(p.word_around(7), (0, 7), "just after a word still picks it");
    }

    #[test]
    fn the_rectangles_of_a_selection_make_one_per_line_with_the_gaps_filled() {
        let p = page("ab cd\nef");
        let rects = p.rects(0, p.chars.len());
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(30.0, 10.0)), "the space is filled");
        assert_eq!(p.line_around(7), (6, 8));
        assert_eq!(p.string(3, 5), "cd");
        assert_eq!(p.text(), "ab cd\nef");
    }
}
