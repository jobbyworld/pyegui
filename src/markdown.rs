use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{current_ui, kwarg, UI};

thread_local! {
    // keeps loaded images and highlighted code between frames
    static CACHE: RefCell<CommonMarkCache> = RefCell::new(CommonMarkCache::default());
    // mermaid diagrams already rendered: hash of (source, theme) -> image uri or error
    static DIAGRAMS: RefCell<HashMap<u64, Result<String, String>>> = RefCell::new(HashMap::new());
    // the system fonts, loaded with the first diagram only
    static SVG_OPTIONS: std::cell::OnceCell<usvg::Options<'static>> = const { std::cell::OnceCell::new() };
}

/// Turns the texts of an svg into paths.
///
/// The svg loader of egui reads no fonts, so it skips texts. Loading the system fonts for every
/// svg would slow down the start of every app, so it is done here, for the diagrams only.
fn texts_to_paths(svg: &str) -> Result<String, String> {
    SVG_OPTIONS.with(|options| {
        let options = options.get_or_init(|| {
            let mut options = usvg::Options::default();
            options.fontdb_mut().load_system_fonts();
            options
        });
        let tree = usvg::Tree::from_str(svg, options).map_err(|e| e.to_string())?;
        // the writer saves the texts as their paths
        Ok(tree.to_string(&usvg::WriteOptions::default()))
    })
}

const MERMAID_THEMES: [&str; 5] = ["default", "dark", "forest", "neutral", "base"];

/// Renders a mermaid diagram once, gives it to the image loaders of egui and returns its uri
fn mermaid_image(ctx: &egui::Context, source: &str, theme: &str) -> Result<String, String> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (source, theme).hash(&mut hasher);
    let key = hasher.finish();

    DIAGRAMS.with_borrow_mut(|diagrams| {
        diagrams
            .entry(key)
            .or_insert_with(|| {
                let theme = mermaid_svg::Theme::by_name(theme).unwrap_or_default();
                let svg = mermaid_svg::render_with(source, &theme).map_err(|e| e.to_string())?;
                let svg = texts_to_paths(&svg)?;
                // the .svg extension picks the svg loader of egui_extras
                let uri = format!("bytes://mermaid-{:016x}.svg", key);
                ctx.include_bytes(uri.clone(), svg.into_bytes());
                Ok(uri)
            })
            .clone()
    })
}

/// Replaces the ```mermaid code blocks with images of their diagrams
fn replace_mermaid<'a>(ctx: &egui::Context, text: &'a str, theme: &str) -> Cow<'a, str> {
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

        match mermaid_image(ctx, &source, theme) {
            Ok(uri) => out.push_str(&format!("\n![mermaid diagram]({})\n\n", uri)),
            Err(err) => out.push_str(&format!(
                "\n```text\nmermaid: {}\n\n{}```\n",
                err.trim(),
                source
            )),
        }
    }
    Cow::Owned(out)
}

/// Shows markdown text: headings, lists, tables, quotes, code blocks, links and images.
///
/// Keyword args:
///
///     scrollable (bool): show the text in a scroll area, drawing only the visible part. Faster for long texts
///
///     id (str): id of the scroll area, needed if there are several scrollable markdown texts
///
///     max_image_width (int): max width of the images
///
///     default_width (int): width of the text, default is the available width
///
///     mermaid_theme (str): theme of the mermaid diagrams: "default", "dark", "forest", "neutral" or "base".
///     Default follows the theme of the app
///
/// Code blocks marked ``mermaid`` are drawn as diagrams: flowcharts, sequence, class, state, ER, pie,
/// gantt and more. A diagram that does not parse shows its error and its source.
///
/// Example::
///
///     markdown('''
///     # Title
///     Some **bold** and *italic* text, a [link](https://github.com/emilk/egui).
///
///     - item 1
///     - item 2
///
///     ```python
///     print("hello")
///     ```
///
///     ```mermaid
///     flowchart LR
///         A[Start] --> B{OK?}
///         B -->|Yes| C[Done]
///     ```
///     ''')
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn markdown(text: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let mut viewer = CommonMarkViewer::new();

    if let Some(width) = kwarg::<usize>(kwargs, "max_image_width")? {
        viewer = viewer.max_image_width(Some(width));
    }
    if let Some(width) = kwarg::<usize>(kwargs, "default_width")? {
        viewer = viewer.default_width(Some(width));
    }

    let theme = match kwarg::<String>(kwargs, "mermaid_theme")? {
        Some(theme) if !MERMAID_THEMES.contains(&theme.as_str()) => {
            return Err(PyValueError::new_err(format!(
                "Unknown mermaid theme '{}', expected one of {}",
                theme,
                MERMAID_THEMES.join(", ")
            )))
        }
        Some(theme) => theme,
        None if ui.visuals().dark_mode => "dark".to_string(),
        None => "default".to_string(),
    };
    let text = replace_mermaid(ui.ctx(), text, &theme);
    let text = text.as_ref();

    let scrollable = kwarg::<bool>(kwargs, "scrollable")?.unwrap_or(false);
    let id = kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "markdown".to_string());

    CACHE.with_borrow_mut(|cache| {
        if scrollable {
            viewer.show_scrollable(
                egui::Id::unique(id),
                ui,
                cache,
                &CommonMarkScrollOptions::default(),
                text,
            );
        } else {
            viewer.show(ui, cache, text);
        }
    });
    Ok(())
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(markdown, m)?)?;
    Ok(())
}
