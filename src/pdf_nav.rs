//! Navigation inside a PDF: the links of a page (to another page or to a web address) and the
//! bookmarks of the document (its outline), read from the objects of the file with `craft-pdf`.
//!
//! The geometry is the one of `pdf_text`: points, origin at the top left of the page as it is drawn.

use std::collections::HashMap;

use craft_pdf::syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier, String as PdfString};
use craft_pdf::syntax::{page::Rotation, Pdf};
use craft_pdf::interpret::TransformExt;
use eframe::egui::{Pos2, Rect};
use kurbo::Point;

/// Most links read from a page, bookmarks read from a document, and the depth of a tree.
const MAX_LINKS: usize = 5000;
const MAX_BOOKMARKS: usize = 5000;
const MAX_DEPTH: usize = 16;

/// Where a link or a bookmark goes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// A page (from 0) and the height to show at the top of the window when the destination gives it
    /// (points from the top of the page).
    Page { page: usize, y: Option<f32> },
    Uri(String),
}

/// A clickable area of a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Link {
    pub rect: Rect,
    pub target: Target,
}

/// One bookmark of the outline.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bookmark {
    pub title: String,
    pub level: usize,
    pub target: Option<Target>,
}

type Ids = HashMap<ObjectIdentifier, usize>;

/// The page (from 0) of every page object, to find where a destination goes.
pub(crate) fn page_ids(pdf: &Pdf) -> Ids {
    pdf.pages().iter().enumerate().filter_map(|(i, p)| p.raw().obj_id().map(|id| (id, i))).collect()
}

fn text_of(s: &PdfString<'_>) -> String {
    crate::pdf::decode_text(s).unwrap_or_default()
}

/// A destination: an array `[page /XYZ left top zoom]` (or `/FitH top`...), a name or a string to look
/// up in the names of the document, or a dictionary with a `/D`.
fn destination(pdf: &Pdf, ids: &Ids, object: Object<'_>, depth: usize) -> Option<Target> {
    if depth > 4 {
        return None;
    }
    match object {
        Object::Array(array) => array_destination(pdf, ids, &array),
        Object::Dict(dict) => destination(pdf, ids, dict.get::<Object<'_>>(b"D")?, depth + 1),
        Object::Name(name) => named_destination(pdf, ids, name.as_ref(), depth),
        Object::String(string) => named_destination(pdf, ids, string.as_ref(), depth),
        _ => None,
    }
}

fn array_destination(pdf: &Pdf, ids: &Ids, array: &Array<'_>) -> Option<Target> {
    let mut items = array.raw_iter();
    let page = match items.next()? {
        MaybeRef::Ref(r) => *ids.get(&ObjectIdentifier::from(r))?,
        // some writers give the number of the page instead of its object
        MaybeRef::NotRef(Object::Number(n)) => usize::try_from(n.as_i64()).ok().filter(|p| *p < pdf.pages().len())?,
        MaybeRef::NotRef(_) => return None,
    };
    let kind = match items.next() {
        Some(MaybeRef::NotRef(Object::Name(n))) => n.as_ref().to_vec(),
        _ => Vec::new(),
    };
    let number = |item: Option<MaybeRef<Object<'_>>>| match item {
        Some(MaybeRef::NotRef(Object::Number(n))) => Some(n.as_f32()),
        _ => None,
    };
    // the height asked, in the space of the page: XYZ left top zoom, FitH top, FitBH top
    let top = match kind.as_slice() {
        b"XYZ" => number(items.nth(1)),
        b"FitH" | b"FitBH" => number(items.next()),
        _ => None,
    };
    // a top at the very bottom of the page (0) is what some writers put for "no position": the top of the page
    let y = top.filter(|top| *top > 1.0).and_then(|top| {
        let pages = pdf.pages();
        let page_data = pages.get(page)?;
        // only the pages that are not rotated: the top is read from the top of the crop box
        matches!(page_data.rotation(), Rotation::None).then(|| page_data.crop_box().y1 as f32 - top)
    });
    Some(Target::Page { page, y: y.filter(|y| y.is_finite() && *y >= 0.0) })
}

fn named_destination(pdf: &Pdf, ids: &Ids, name: &[u8], depth: usize) -> Option<Target> {
    let root = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id())?;
    // the dictionary of the old versions
    if let Some(found) = root.get::<Dict<'_>>(b"Dests").and_then(|d| d.get::<Object<'_>>(name)) {
        return destination(pdf, ids, found, depth + 1);
    }
    // the name tree of the new ones
    let tree = root.get::<Dict<'_>>(b"Names")?.get::<Dict<'_>>(b"Dests")?;
    let found = name_tree_find(&tree, name, 0)?;
    destination(pdf, ids, found, depth + 1)
}

fn name_tree_find<'a>(node: &Dict<'a>, key: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(names) = node.get::<Array<'_>>(b"Names") {
        let mut items = names.flex_iter();
        while let (Some(k), Some(v)) = (items.next(), items.next()) {
            let matches = match k {
                Object::String(s) => s.as_ref() == key,
                Object::Name(n) => n.as_ref() == key,
                _ => false,
            };
            if matches {
                return Some(v);
            }
        }
    }
    for kid in node.get::<Array<'_>>(b"Kids")?.iter::<Dict<'_>>() {
        if let Some(found) = name_tree_find(&kid, key, depth + 1) {
            return Some(found);
        }
    }
    None
}

/// What an action or a destination entry of a link or a bookmark says.
fn target_of(pdf: &Pdf, ids: &Ids, dict: &Dict<'_>) -> Option<Target> {
    if let Some(action) = dict.get::<Dict<'_>>(b"A") {
        match action.get::<Name<'_>>(b"S").as_deref().map(|n| n.as_ref()) {
            Some(b"URI") => {
                let uri = action.get::<PdfString<'_>>(b"URI")?;
                let text = String::from_utf8_lossy(&uri).trim().to_owned();
                return (!text.is_empty()).then_some(Target::Uri(text));
            }
            Some(b"GoTo") => return destination(pdf, ids, action.get::<Object<'_>>(b"D")?, 0),
            _ => return None,
        }
    }
    destination(pdf, ids, dict.get::<Object<'_>>(b"Dest")?, 0)
}

/// The links of a page (from 0).
pub(crate) fn links(pdf: &Pdf, ids: &Ids, index: usize) -> Vec<Link> {
    let pages = pdf.pages();
    let Some(page) = pages.get(index) else { return Vec::new() };
    let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else { return Vec::new() };
    let to_view = page.initial_transform(true).to_kurbo();
    let mut out = Vec::new();
    for annot in annots.iter::<Dict<'_>>().take(MAX_LINKS * 4) {
        if out.len() >= MAX_LINKS {
            break;
        }
        if annot.get::<Name<'_>>(b"Subtype").as_deref().map(|n| n.as_ref()) != Some(b"Link".as_slice()) {
            continue;
        }
        let Some(target) = target_of(pdf, ids, &annot) else { continue };
        let Some(rect) = annot.get::<Array<'_>>(b"Rect").map(|a| a.iter::<f64>().collect::<Vec<_>>()).filter(|r| r.len() == 4) else { continue };
        let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[2], rect[3]), (rect[0], rect[3])].map(|(x, y)| to_view * Point::new(x, y));
        let (min_x, max_x) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        let (min_y, max_y) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        if [min_x, max_x, min_y, max_y].iter().all(|v| v.is_finite()) && max_x > min_x && max_y > min_y {
            out.push(Link { rect: Rect::from_min_max(Pos2::new(min_x as f32, min_y as f32), Pos2::new(max_x as f32, max_y as f32)), target });
        }
    }
    out
}

/// The bookmarks of the document, in the order of the outline, with their level (0 for the top).
pub(crate) fn outline(pdf: &Pdf, ids: &Ids) -> Vec<Bookmark> {
    let mut out = Vec::new();
    let Some(root) = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()) else { return out };
    let Some(first) = root.get::<Dict<'_>>(b"Outlines").and_then(|o| o.get::<Dict<'_>>(b"First")) else { return out };
    walk(pdf, ids, first, 0, &mut out);
    out
}

fn walk(pdf: &Pdf, ids: &Ids, first: Dict<'_>, level: usize, out: &mut Vec<Bookmark>) {
    let mut node = Some(first);
    // a hostile file can loop its `/Next`: the count of the bookmarks bounds the walk
    while let Some(item) = node {
        if out.len() >= MAX_BOOKMARKS || level > MAX_DEPTH {
            return;
        }
        let title = item.get::<PdfString<'_>>(b"Title").map(|t| text_of(&t)).unwrap_or_default();
        out.push(Bookmark { title, level, target: target_of(pdf, ids, &item) });
        if let Some(child) = item.get::<Dict<'_>>(b"First") {
            walk(pdf, ids, child, level + 1, out);
        }
        node = item.get::<Dict<'_>>(b"Next");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{open_with_info, sample};
    use crate::pdf_text::{extract_page, fold};
    use std::sync::Arc;

    fn open() -> Pdf {
        let (info, pdf) = open_with_info(Arc::new(sample::pdf()), None);
        assert_eq!((info.pages, info.error), (Some(2), None));
        pdf.expect("the sample opens")
    }

    #[test]
    fn the_text_of_a_page_comes_with_the_place_of_every_character() {
        let pdf = open();
        let text = extract_page(&pdf, 0);
        assert_eq!(text.text(), "Hello world\nSecond line");
        let h = text.chars[0];
        assert_eq!(h.c, 'H');
        // 24 pt text at x = 72, its baseline 92 pt from the top of a 792 pt page
        assert!((h.r[0] - 72.0).abs() < 1.0 && h.r[1] > 60.0 && h.r[1] < 95.0 && h.r[3] > 92.0, "{:?}", h.r);
        assert!(h.real && text.chars[5].real, "a space written in the page is a glyph like another");
        assert!(text.chars.iter().any(|c| c.c == '\n' && !c.real), "the newline between the lines is added");
        assert_eq!(extract_page(&pdf, 1).text(), "Chapter two");
        assert_eq!(extract_page(&pdf, 9).text(), "", "a page that does not exist is empty");
    }

    #[test]
    fn a_search_finds_the_text_and_its_rectangle() {
        let pdf = open();
        let text = extract_page(&pdf, 0);
        let query: Vec<char> = "SECOND".chars().map(fold).collect();
        let found = text.find(&query, false);
        assert_eq!(found.len(), 1);
        let rects = text.rects(found[0].0, found[0].1);
        assert_eq!(rects.len(), 1);
        assert!(rects[0].top() > 100.0 && rects[0].left() > 70.0, "second line is under the first: {:?}", rects[0]);
    }

    #[test]
    fn the_links_of_a_page_go_to_a_page_or_to_an_address() {
        let pdf = open();
        let ids = page_ids(&pdf);
        assert_eq!(ids.len(), 2);
        let links = links(&pdf, &ids, 0);
        assert_eq!(links.len(), 2);
        // the second page, 500 pt from the bottom of a 792 pt page: 292 pt from its top
        assert_eq!(links[0].target, Target::Page { page: 1, y: Some(292.0) });
        assert_eq!(links[1].target, Target::Uri("https://example.com".into()));
        // [72 690 200 720] in a page of 792 pt: from 72 pt to 102 pt from the top
        let r = links[0].rect;
        assert!((r.left() - 72.0).abs() < 0.01 && (r.right() - 200.0).abs() < 0.01 && (r.top() - 72.0).abs() < 0.01 && (r.bottom() - 102.0).abs() < 0.01, "{r:?}");
        assert!(super::links(&pdf, &ids, 1).is_empty());
    }

    #[test]
    fn the_outline_lists_the_bookmarks_with_their_destination() {
        let pdf = open();
        let marks = outline(&pdf, &page_ids(&pdf));
        assert_eq!(marks, vec![Bookmark { title: "Chapter".into(), level: 0, target: Some(Target::Page { page: 1, y: Some(292.0) }) }]);
    }
}
