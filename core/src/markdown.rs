use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui;
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};

thread_local! {
    // keeps loaded images and highlighted code between frames
    static CACHE: RefCell<CommonMarkCache> = RefCell::new(CommonMarkCache::default());
    // mermaid diagrams already rendered: hash of (source, theme) -> image uri or error
    static DIAGRAMS: RefCell<HashMap<u64, Result<String, String>>> = RefCell::new(HashMap::new());
}

/// The state of a diagram that is rendered on a thread of its own: the window goes on while it is drawn
enum Slot {
    Pending,
    /// the svg (texts already turned into paths) or the error
    Done(Result<std::sync::Arc<Vec<u8>>, String>),
}

/// The diagrams rendered or being rendered, for every window and thread; what a diagram needs is only its
/// source and its theme, the svg is given to the image loaders of the context that asks for it
static SLOTS: std::sync::Mutex<Option<HashMap<u64, Slot>>> = std::sync::Mutex::new(None);

/// The system fonts, loaded once (about two seconds) by the first diagram, on its thread
static FONTS: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();

/// What `mermaid_image` found
enum Diagram {
    Ready(String),
    /// still being rendered: asked again at the next frame, a repaint is requested when it is done
    Pending,
    Failed(String),
}

/// Turns the texts of an svg into paths.
///
/// The svg loader of egui reads no fonts, so it skips texts. Loading the system fonts for every
/// svg would slow down the start of every app, so it is done here, for the diagrams only.
fn system_fonts() -> std::sync::Arc<usvg::fontdb::Database> {
    FONTS
        .get_or_init(|| {
            let started = std::time::Instant::now();
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            crate::dlog!("mermaid", "system fonts loaded in {:.0?} ({} faces)", started.elapsed(), db.len());
            std::sync::Arc::new(db)
        })
        .clone()
}

/// Loads the system fonts that the diagrams need on a thread of its own, once the app is running: the first
/// diagram then does not wait for them (about two seconds). Harmless to call twice.
pub fn preload_fonts() {
    std::thread::spawn(|| {
        system_fonts();
    });
}

fn texts_to_paths(svg: &str) -> Result<String, String> {
    let mut options = usvg::Options::default();
    options.fontdb = system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    // the writer saves the texts as their paths
    Ok(tree.to_string(&usvg::WriteOptions::default()))
}

pub const MERMAID_THEMES: [&str; 5] = ["default", "dark", "forest", "neutral", "base"];

/// Renders the svg of a diagram (the work that takes time, free of any context)
fn render_svg(source: &str, theme: &str, key: u64) -> Result<std::sync::Arc<Vec<u8>>, String> {
    let started = std::time::Instant::now();
    let result = (|| {
        let theme = mermaid_svg::Theme::by_name(theme).unwrap_or_default();
        let svg = mermaid_svg::render_with(source, &theme).map_err(|e| e.to_string())?;
        Ok(std::sync::Arc::new(texts_to_paths(&svg)?.into_bytes()))
    })();
    match &result {
        Ok(_) => crate::dlog!("mermaid", "diagram {key:016x} pre-rendered ({} chars of source, theme {theme}) in {:.0?}", source.len(), started.elapsed()),
        Err(e) => crate::dlog!("mermaid", "diagram {key:016x} failed (theme {theme}) after {:.0?}: {e}", started.elapsed()),
    }
    result
}

/// Gives the diagram to the image loaders of egui and returns its uri. A diagram that is not rendered yet
/// is rendered on a thread (`wait` false: the window shows a placeholder and is repainted when it is
/// ready) or here (`wait` true: what has no window to repaint, like the export to PDF).
fn mermaid_image(ctx: &egui::Context, source: &str, theme: &str, wait: bool) -> Diagram {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (source, theme).hash(&mut hasher);
    let key = hasher.finish();

    // already given to the loaders of this thread's window
    if let Some(done) = DIAGRAMS.with_borrow(|d| d.get(&key).cloned()) {
        return match done {
            Ok(uri) => Diagram::Ready(uri),
            Err(e) => Diagram::Failed(e),
        };
    }
    let known = {
        let mut slots = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        let slots = slots.get_or_insert_with(HashMap::new);
        match slots.get(&key) {
            Some(Slot::Pending) if !wait => return Diagram::Pending,
            Some(Slot::Done(result)) => Some(result.clone()),
            // wait for what a thread is rendering: render it here too, the first to finish is kept
            _ => {
                if !wait {
                    slots.insert(key, Slot::Pending);
                }
                None
            }
        }
    };
    let result = match known {
        Some(result) => result,
        None if wait => {
            let result = render_svg(source, theme, key);
            SLOTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, Slot::Done(result.clone()));
            result
        }
        None => {
            let (ctx, source, theme) = (ctx.clone(), source.to_string(), theme.to_string());
            std::thread::spawn(move || {
                let result = render_svg(&source, &theme, key);
                SLOTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, Slot::Done(result));
                ctx.request_repaint();
            });
            return Diagram::Pending;
        }
    };
    let outcome = result.map(|svg| {
        // the .svg extension picks the svg loader of egui_extras
        let uri = format!("bytes://mermaid-{:016x}.svg", key);
        ctx.include_bytes(uri.clone(), svg.as_ref().clone());
        uri
    });
    DIAGRAMS.with_borrow_mut(|d| d.insert(key, outcome.clone()));
    match outcome {
        Ok(uri) => Diagram::Ready(uri),
        Err(e) => Diagram::Failed(e),
    }
}

/// Replaces the ```mermaid code blocks with images of their diagrams
///
/// The diagrams are rendered on threads: until one is ready its place holds a short line of text, and the
/// window is repainted when it is done. With `wait` (no window to repaint) the call renders them itself.
pub fn replace_mermaid<'a>(ctx: &egui::Context, text: &'a str, theme: &str, wait: bool) -> Cow<'a, str> {
    if !text.contains("mermaid") {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut lines = text.split_inclusive('\n');

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let fence_char = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let fence_len = fence_char.map_or(0, |c| trimmed.chars().take_while(|x| *x == c).count());
        let info = trimmed[fence_len..].trim();

        if indent > 3 || fence_len < 3 || info.split_whitespace().next() != Some("mermaid") {
            out.push_str(line);
            continue;
        }

        // the closing fence is made of at least as many of the same character
        let fence = fence_char.unwrap().to_string().repeat(fence_len);
        let mut source = String::new();
        let mut closed = false;
        let mut block = line.to_string();
        for line in lines.by_ref() {
            block.push_str(line);
            let trimmed = line.trim();
            if trimmed.starts_with(&fence) && trimmed.trim_start_matches(fence_char.unwrap()).is_empty() {
                closed = true;
                break;
            }
            source.push_str(line);
        }
        if !closed {
            // not a diagram until it is closed, e.g. while it is typed
            out.push_str(&block);
            continue;
        }

        match mermaid_image(ctx, &source, theme, wait) {
            Diagram::Ready(uri) => out.push_str(&format!("\n![mermaid diagram]({})\n\n", uri)),
            Diagram::Pending => out.push_str("\n*Rendering the diagram…*\n\n"),
            Diagram::Failed(err) => out.push_str(&format!(
                "\n```text\nmermaid: {}\n\n{}```\n",
                err.trim(),
                source
            )),
        }
    }
    Cow::Owned(out)
}

/// How a table is drawn
#[derive(Clone, Copy, PartialEq)]
pub enum TableFit {
    /// its natural width, a scroll area if it is wider than the column
    Scroll,
    /// the cells wrap their text so that the table fits the column
    Reformat,
    /// the cells wrap their text so that the table fits the whole available width, out of the column
    FullWidth,
}

/// Which blocks of the text are not drawn as the rest of it
pub struct Policies {
    pub table: TableFit,
    pub code_wide: bool,
    pub image_wide: bool,
    pub mermaid_wide: bool,
}

/// A part of the text, cut around the blocks that have their own width
enum Part {
    Column(std::ops::Range<usize>),
    /// a block drawn by the viewer in the whole available width
    Wide(std::ops::Range<usize>),
    /// a table drawn with wrapped cells, in the column (false) or in the whole available width (true)
    Table(std::ops::Range<usize>, bool),
}

/// Cuts the text around the top level blocks that a policy takes out of the column: the tables, the code
/// blocks, the paragraphs that are only an image, and those that are only a mermaid diagram (an image that
/// `replace_mermaid` made). `can_break_out` is false when the column is the whole width: nothing is wider.
/// An empty result means that the viewer draws the text as it is.
fn split_parts(text: &str, policies: &Policies, can_break_out: bool) -> Vec<Part> {
    use pulldown_cmark::{Event, Options, Parser, Tag};

    struct Para {
        range: std::ops::Range<usize>,
        only_image: bool,
        mermaid: bool,
        images: usize,
    }
    let wide_table = policies.table == TableFit::FullWidth;
    let wide_blocks = can_break_out && (policies.code_wide || policies.image_wide || policies.mermaid_wide);
    if policies.table == TableFit::Scroll && !wide_blocks {
        return Vec::new();
    }
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut blocks: Vec<(std::ops::Range<usize>, Option<bool>)> = Vec::new(); // (range, table in full width)
    let mut depth = 0usize;
    let mut para: Option<Para> = None;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    match tag {
                        Tag::Table(_) if policies.table != TableFit::Scroll => blocks.push((range, Some(wide_table))),
                        Tag::CodeBlock(_) if can_break_out && policies.code_wide => blocks.push((range, None)),
                        Tag::Paragraph => {
                            para = Some(Para { range, only_image: true, mermaid: false, images: 0 })
                        }
                        _ => {}
                    }
                } else if depth == 1 {
                    if let Some(p) = para.as_mut() {
                        match &tag {
                            Tag::Image { dest_url, .. } if p.images == 0 => {
                                p.mermaid = dest_url.starts_with("bytes://mermaid-");
                                p.images += 1;
                            }
                            _ => p.only_image = false,
                        }
                    }
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(p) = para.take() {
                        let wide = if p.mermaid { policies.mermaid_wide } else { policies.image_wide };
                        if can_break_out && p.only_image && p.images == 1 && wide {
                            blocks.push((p.range, None));
                        }
                    }
                }
            }
            _ => {
                if depth == 1 {
                    if let Some(p) = para.as_mut() {
                        p.only_image = false; // a text next to the image
                    }
                }
            }
        }
    }
    let mut parts = Vec::new();
    let mut pos = 0;
    for (range, table) in blocks {
        if range.start > pos {
            parts.push(Part::Column(pos..range.start));
        }
        pos = range.end;
        parts.push(match table {
            Some(wide) => Part::Table(range, wide),
            None => Part::Wide(range),
        });
    }
    if !parts.is_empty() && pos < text.len() {
        parts.push(Part::Column(pos..text.len()));
    }
    parts
}

/// The source of the cells of a table: the header, then the rows
fn table_cells(src: &str) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let (mut head, mut rows, mut row) = (Vec::new(), Vec::new(), Vec::new());
    for (event, range) in Parser::new_ext(src, options).into_offset_iter() {
        match event {
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => row.clear(),
            Event::End(TagEnd::TableHead) => head = std::mem::take(&mut row),
            Event::End(TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
            Event::Start(Tag::TableCell) => row.push(src[range].trim().to_string()),
            _ => {}
        }
    }
    (!head.is_empty()).then_some((head, rows))
}

/// The widths of the columns of a table that must fit `avail`: what the texts want when it fits, else
/// the narrowest widths (the longest word) and the room that is left shared like what the texts want more
fn fit_columns(desired: &[f32], min: &[f32], avail: f32) -> Vec<f32> {
    if desired.iter().sum::<f32>() <= avail {
        return desired.to_vec();
    }
    let min_sum: f32 = min.iter().sum();
    if min_sum >= avail {
        return min.iter().map(|w| w * avail / min_sum).collect();
    }
    let extra: f32 = desired.iter().zip(min).map(|(d, m)| d - m).sum();
    let room = avail - min_sum;
    desired
        .iter()
        .zip(min)
        .map(|(d, m)| m + room * (d - m) / extra.max(f32::EPSILON))
        .collect()
}

/// The words of the source of a cell as they are drawn (no markers of bold, code or link), each with
/// whether it is code: a link keeps its text, not its address
fn cell_words(cell: &str) -> Vec<(String, bool)> {
    cell.split_whitespace()
        .filter_map(|token| {
            let code = token.contains('`');
            let token = match token.find("](") {
                Some(end) => &token[..end], // the end of a link: its address is not drawn
                None => token,
            };
            let word: String = token.chars().filter(|c| !matches!(c, '*' | '`' | '[' | ']')).collect();
            (!word.is_empty()).then_some((word, code))
        })
        .collect()
}

/// The width of a cell without wrapping, and of its longest word, measured with the fonts of the ui
fn cell_widths(ui: &egui::Ui, cell: &str, bold: bool) -> (f32, f32) {
    let width = |text: &str, code: bool| {
        let font = if code {
            egui::TextStyle::Monospace.resolve(ui.style())
        } else {
            egui::TextStyle::Body.resolve(ui.style())
        };
        let galley = ui.painter().layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
        // a code has its own margins, and a bold text is wider than the regular one
        galley.size().x * if bold && !code { 1.1 } else { 1.0 } + if code { 6.0 } else { 0.0 }
    };
    let space = width(" ", false);
    let words = cell_words(cell);
    let mut total = 0.0;
    let mut longest: f32 = 0.0;
    for (i, (word, code)) in words.iter().enumerate() {
        let w = width(word, *code);
        total += w + if i > 0 { space } else { 0.0 };
        longest = longest.max(w);
    }
    (total, longest)
}

/// Draws a table whose cells wrap their text, so that it fits the width of `ui`. Every cell is drawn by the
/// viewer, so it can hold bold, code, links and images. The columns are not aligned as the table says.
fn wrapped_table(ui: &mut egui::Ui, cache: &mut CommonMarkCache, viewer: &dyn Fn() -> CommonMarkViewer<'static>, src: &str) {
    let Some((head, rows)) = table_cells(src) else {
        viewer().show(ui, cache, src);
        return;
    };
    let n = head.len();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let spacing = ui.spacing().item_spacing.x;
        let avail = (ui.available_width() - spacing * (n - 1) as f32).max(n as f32 * 30.0);
        let cell = |row: &Vec<String>, i: usize| row.get(i).map(String::as_str).unwrap_or("").to_string();

        let (mut desired, mut min) = (vec![0f32; n], vec![0f32; n]);
        for (r, row) in std::iter::once(&head).chain(rows.iter()).enumerate() {
            for i in 0..n {
                let text = cell(row, i);
                let (all, longest) = cell_widths(ui, &text, r == 0 || text.contains("**"));
                desired[i] = desired[i].max(all);
                min[i] = min[i].max(longest);
            }
        }
        for i in 0..n {
            // a little room: a word wider than its column does not wrap, it makes the column wider
            min[i] = (min[i] + 4.0).clamp(30.0, (desired[i] + 4.0).max(30.0));
            desired[i] = (desired[i] + 4.0).max(min[i]);
        }
        let widths = fit_columns(&desired, &min, avail);

        egui::Grid::new(("pyegui_md_table", ui.scope_id())).striped(true).min_col_width(0.0).show(ui, |ui| {
            for (r, row) in std::iter::once(&head).chain(rows.iter()).enumerate() {
                for i in 0..n {
                    let text = cell(row, i);
                    ui.vertical(|ui| {
                        ui.set_width(widths[i]);
                        if text.is_empty() {
                            return;
                        }
                        let text = if r == 0 && !text.contains('*') { format!("**{}**", text) } else { text };
                        viewer().show(ui, cache, &text);
                    });
                }
                ui.end_row();
            }
        });
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Rst {
    Plain,
    Heading,
    Directive,
    Comment,
    Marker,
    Literal,
    Strong,
    Emph,
    Role,
    Link,
}

/// The spans of the inline markup of a line of reStructuredText: ``literal``, **strong**, *emphasis*,
/// :role:`text` and `link`_
fn rst_inline(line: &str) -> Vec<(usize, usize, Rst)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &line[i..];
        let mut found = None;
        if rest.starts_with("``") {
            if let Some(e) = rest[2..].find("``") {
                found = Some((i + e + 4, Rst::Literal));
            }
        } else if rest.starts_with("**") {
            if let Some(e) = rest[2..].find("**") {
                found = Some((i + e + 4, Rst::Strong));
            }
        } else if b[i] == b':' {
            let name = rest[1..].bytes().take_while(|c| c.is_ascii_alphanumeric() || b"_.+-".contains(c)).count();
            if name > 0 && rest[1 + name..].starts_with(":`") {
                if let Some(e) = rest[name + 3..].find('`') {
                    found = Some((i + name + 3 + e + 1, Rst::Role));
                }
            }
        } else if b[i] == b'`' {
            if let Some(e) = rest[1..].find('`') {
                let mut end = i + e + 2;
                if line[end..].starts_with("__") {
                    end += 2;
                } else if line[end..].starts_with('_') {
                    end += 1;
                }
                found = Some((end, Rst::Link));
            }
        } else if b[i] == b'*' && b.get(i + 1).is_some_and(|c| !c.is_ascii_whitespace() && *c != b'*') {
            if let Some(e) = rest[1..].find('*') {
                if !rest[1..1 + e].ends_with(char::is_whitespace) && e > 0 {
                    found = Some((i + e + 2, Rst::Emph));
                }
            }
        }
        match found {
            Some((end, kind)) => {
                out.push((i, end, kind));
                i = end;
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    out
}

/// Colors a reStructuredText source: titles and their adornments, directives, comments and targets,
/// list markers, field names and the inline markup
fn rst_job(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    use egui::{text::LayoutJob, Color32, TextFormat};
    let dark = ui.visuals().dark_mode;
    let pick = |d: (u8, u8, u8), l: (u8, u8, u8)| {
        let c = if dark { d } else { l };
        Color32::from_rgb(c.0, c.1, c.2)
    };
    let color = |k: Rst| match k {
        Rst::Plain => ui.visuals().text_color(),
        Rst::Heading => pick((235, 180, 90), (150, 90, 0)),
        Rst::Directive => pick((200, 130, 225), (130, 40, 160)),
        Rst::Comment => ui.visuals().weak_text_color(),
        Rst::Marker => pick((100, 180, 230), (0, 100, 160)),
        Rst::Literal => pick((150, 200, 120), (40, 120, 40)),
        Rst::Strong => pick((240, 160, 120), (170, 60, 20)),
        Rst::Emph => pick((130, 200, 200), (0, 110, 110)),
        Rst::Role => pick((230, 200, 110), (130, 100, 0)),
        Rst::Link => pick((110, 160, 240), (20, 70, 190)),
    };
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = LayoutJob::default();
    let put = |job: &mut LayoutJob, s: &str, k: Rst| {
        if !s.is_empty() {
            job.append(s, 0.0, TextFormat::simple(font.clone(), color(k)));
        }
    };
    let adornment = |l: &str| {
        let t = l.trim_end();
        let first = t.chars().next();
        t.chars().count() >= 3 && first.is_some_and(|c| c.is_ascii_punctuation()) && t.chars().all(|c| Some(c) == first)
    };
    let lines: Vec<&str> = text.split('\n').collect();
    for (n, full) in lines.iter().enumerate() {
        let line = full.trim_end_matches('\r');
        let tail = &full[line.len()..];
        let indent = line.len() - line.trim_start().len();
        let body = &line[indent..];
        put(&mut job, &line[..indent], Rst::Plain);
        let next = lines.get(n + 1).map(|l| l.trim_end_matches('\r'));
        if adornment(body) && indent == 0 || (!body.is_empty() && indent == 0 && next.is_some_and(|l| adornment(l) && l.trim_end().chars().count() >= body.trim_end().chars().count().min(3)) && !adornment(body)) {
            put(&mut job, body, Rst::Heading);
        } else if let Some(after) = body.strip_prefix(".. ") {
            let name = after.find("::").filter(|e| after[..*e].bytes().all(|c| c.is_ascii_alphanumeric() || b"_-:+".contains(&c)));
            if let Some(e) = name {
                put(&mut job, &body[..3 + e + 2], Rst::Directive);
                for (a, z, k) in rst_segments(&after[e + 2..]) {
                    put(&mut job, &after[e + 2..][a..z], k);
                }
            } else if after.starts_with('_') || after.starts_with('[') {
                put(&mut job, body, Rst::Link);
            } else {
                put(&mut job, body, Rst::Comment);
            }
        } else {
            // a list marker, or a field name (:name:), then the inline markup
            let marker = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") || body.starts_with("• ") {
                body.chars().next().map_or(0, char::len_utf8)
            } else {
                let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                if digits > 0 && (body[digits..].starts_with(". ") || body[digits..].starts_with(") ")) { digits + 1 } else { 0 }
            };
            let mut rest = body;
            if marker > 0 {
                put(&mut job, &body[..marker], Rst::Marker);
                rest = &body[marker..];
            } else if body.starts_with(':') && !body.starts_with(":`") {
                if let Some(e) = body[1..].find(':').filter(|e| body[1 + e + 1..].starts_with(' ') || body[1 + e + 1..].is_empty()) {
                    put(&mut job, &body[..e + 2], Rst::Marker);
                    rest = &body[e + 2..];
                }
            }
            for (a, z, k) in rst_segments(rest) {
                put(&mut job, &rest[a..z], k);
            }
        }
        put(&mut job, tail, Rst::Plain);
        if n + 1 < lines.len() {
            put(&mut job, "\n", Rst::Plain);
        }
    }
    job
}

/// The line cut in (start, end, kind) pieces that cover it, plain where there is no markup
fn rst_segments(line: &str) -> Vec<(usize, usize, Rst)> {
    let mut out = Vec::new();
    let mut at = 0;
    for (a, z, k) in rst_inline(line) {
        if a > at {
            out.push((at, a, Rst::Plain));
        }
        out.push((a, z, k));
        at = z;
    }
    if at < line.len() {
        out.push((at, line.len(), Rst::Plain));
    }
    out
}

/// The language given to the highlighter of egui_extras: a name or the extension of a file
fn highlight_language(language: &str) -> &str {
    match language.trim_start_matches('.').to_lowercase().as_str() {
        // no PowerShell in egui_extras: the shell one knows its comments, strings and $variables
        "ps1" | "psm1" | "psd1" | "powershell" | "pwsh" => "sh",
        "mjs" | "cjs" => "js",
        "pyw" => "py",
        _ => language.trim_start_matches('.'),
    }
}

/// What `show` takes: the options of `markdown()`, already checked
pub struct MarkdownOptions {
    pub max_image_width: Option<usize>,
    pub default_width: Option<usize>,
    /// one of `MERMAID_THEMES`; None follows the theme of the ui
    pub mermaid_theme: Option<String>,
    pub scrollable: bool,
    pub id: String,
    /// a click on a link is also given to the system, which opens the browser
    pub open_links: bool,
    pub max_width: Option<f32>,
    /// 0 left, 0.5 center, 1 right
    pub align: f32,
    pub policies: Policies,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        MarkdownOptions {
            max_image_width: None,
            default_width: None,
            mermaid_theme: None,
            scrollable: false,
            id: "markdown".to_string(),
            open_links: true,
            max_width: None,
            align: 0.0,
            policies: Policies { table: TableFit::Scroll, code_wide: false, image_wide: false, mermaid_wide: false },
        }
    }
}

/// Shows a markdown text. Returns the url of the link clicked, if any.
pub fn show(ui: &mut egui::Ui, text: &str, options: &MarkdownOptions) -> Option<String> {
    let max_image_width = options.max_image_width;
    let default_width = options.default_width;
    // a viewer is used up by one call: one is made for every part of the text
    let viewer = || {
        let mut viewer = CommonMarkViewer::new();
        if let Some(width) = max_image_width {
            viewer = viewer.max_image_width(Some(width));
        }
        if let Some(width) = default_width {
            viewer = viewer.default_width(Some(width));
        }
        viewer
    };

    let theme = match &options.mermaid_theme {
        Some(theme) => theme.clone(),
        None if ui.visuals().dark_mode => "dark".to_string(),
        None => "default".to_string(),
    };
    let text = replace_mermaid(ui.ctx(), text, &theme, false);
    let text = text.as_ref();

    let scrollable = options.scrollable;
    let id = options.id.clone();
    let open_links = options.open_links;
    // a click on a link asks egui to open its url: the commands added while the text is drawn
    let before = ui.ctx().output(|o| o.commands.len());
    let max_width = options.max_width.filter(|w| *w > 0.0);
    let align = options.align;
    let policies = &options.policies;

    let draw = |ui: &mut egui::Ui, text: &str, scrollable: bool| {
        CACHE.with_borrow_mut(|cache| {
            if scrollable {
                viewer().show_scrollable(
                    egui::Id::unique(id.clone()),
                    ui,
                    cache,
                    &CommonMarkScrollOptions::default(),
                    text,
                );
            } else {
                viewer().show(ui, cache, text);
            }
        });
    };
    let avail = ui.available_width();
    let column_width = max_width.map_or(avail, |max| max.min(avail));
    let can_break_out = column_width < avail;
    // a part of the text in the column: a child ui of the width, placed in the available width
    let in_column = |ui: &mut egui::Ui, f: &mut dyn FnMut(&mut egui::Ui)| {
        if !can_break_out {
            return f(ui);
        }
        let area = ui.available_rect_before_wrap();
        let left = area.min.x + (area.width() - column_width) * align;
        let rect = egui::Rect::from_min_max(
            egui::pos2(left, area.min.y),
            egui::pos2(left + column_width, area.max.y),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| f(ui));
    };
    let parts = split_parts(text, policies, can_break_out);
    if parts.is_empty() {
        in_column(ui, &mut |ui| draw(ui, text, scrollable));
    } else {
        // blocks with a width of their own: the text is cut around them. To keep one scroll area for
        // all the parts, it is made here, and the viewer draws every part without its own
        let draw_parts = |ui: &mut egui::Ui| {
            for (index, part) in parts.iter().enumerate() {
                ui.push_id(index, |ui| match part {
                    Part::Column(range) => in_column(ui, &mut |ui| draw(ui, &text[range.clone()], false)),
                    Part::Wide(range) => draw(ui, &text[range.clone()], false),
                    Part::Table(range, wide) => {
                        let mut table = |ui: &mut egui::Ui| {
                            CACHE.with_borrow_mut(|cache| wrapped_table(ui, cache, &viewer, &text[range.clone()]))
                        };
                        if *wide {
                            table(ui);
                        } else {
                            in_column(ui, &mut table);
                        }
                    }
                });
            }
        };
        if scrollable {
            egui::ScrollArea::vertical()
                .id_salt(egui::Id::unique(format!("markdown_parts_{id}")))
                .auto_shrink([false, false])
                .show(ui, draw_parts);
        } else {
            draw_parts(ui);
        }
    }

    let mut clicked = None;
    ui.ctx().output_mut(|o| {
        let mut index = 0;
        o.commands.retain(|command| {
            let added = index >= before;
            index += 1;
            match command {
                egui::OutputCommand::OpenUrl(open) if added => {
                    clicked.get_or_insert_with(|| open.url.clone());
                    open_links // kept for the browser, or left to the app
                }
                _ => true,
            }
        });
    });
    clicked
}


/// Shows source code with syntax highlighting and line numbers, in a scroll area
pub fn code_view(ui: &mut egui::Ui, text: &str, language: &str, line_numbers: bool, id: &str) {

    let style = ui.style().clone();
    let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(&style);
    // memoized by egui_extras: cheap to call at every frame
    let lang = language.trim_start_matches('.').to_lowercase();
    let mut job = if matches!(lang.as_str(), "rst" | "rest") {
        rst_job(ui, text)
    } else if matches!(lang.as_str(), "feature" | "gherkin") {
        crate::gherkin::job(ui, text)
    } else {
        egui_extras::syntax_highlighting::highlight(ui.ctx(), &style, &theme, text, highlight_language(language))
    };
    job.wrap.max_width = f32::INFINITY; // code is not wrapped: the area scrolls sideways

    egui::ScrollArea::both()
        .id_salt(id.to_owned())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                if line_numbers {
                    let count = text.lines().count().max(1);
                    let numbers = (1..=count).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    let color = ui.visuals().weak_text_color();
                    ui.add(egui::Label::new(egui::RichText::new(numbers).font(font).color(color)).selectable(false));
                    ui.add_space(8.0);
                }
                ui.add(egui::Label::new(job).selectable(true).extend());
            });
        });
}


#[cfg(test)]
mod rst_tests {
    use super::*;

    #[test]
    fn inline_markup_is_cut_without_panicking() {
        for line in ["``a`` **b** *c* :ref:`d` `e`_ é `é`__ ``", "* ", "**", ":x:`", "`", "é*é*é", ":é:`x`", "a ``b"] {
            let segs = rst_segments(line);
            let joined: String = segs.iter().map(|(a, z, _)| &line[*a..*z]).collect();
            assert_eq!(joined, line);
        }
        let kinds: Vec<_> = rst_segments("a ``b`` **c**").iter().map(|s| s.2 == Rst::Literal || s.2 == Rst::Strong).collect();
        assert_eq!(kinds, vec![false, true, false, true]);
    }
}


#[cfg(test)]
mod mermaid_tests {
    use super::*;

    const TEXT: &str = "# t\n\n```mermaid\ngraph TD; A-->B\n```\n\nafter\n";

    #[test]
    fn a_diagram_is_rendered_on_a_thread_and_replaces_its_placeholder() {
        let ctx = egui::Context::default();
        let first = replace_mermaid(&ctx, TEXT, "default", false);
        assert!(first.contains("Rendering the diagram"), "{first}");
        let started = std::time::Instant::now();
        loop {
            let text = replace_mermaid(&ctx, TEXT, "default", false);
            if text.contains("bytes://mermaid-") {
                break;
            }
            assert!(started.elapsed().as_secs() < 30, "the diagram never came: {text}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn waiting_gives_the_diagram_at_once() {
        let ctx = egui::Context::default();
        let text = replace_mermaid(&ctx, "```mermaid\ngraph LR; X-->Y\n```\n", "dark", true);
        assert!(text.contains("bytes://mermaid-"), "{text}");
    }
}
