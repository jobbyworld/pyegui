use pyegui_core::markdown::{self as core, MarkdownOptions, Policies, TableFit, MERMAID_THEMES};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{current_ui, kwarg, UI};


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
///     max_width (float): max width of the whole document, in points. A wider area leaves the rest empty.
///         Default is the available width
///
///     align (str): where a document limited by ``max_width`` is placed: "left" (default), "center" or "right"
///
///     table_width (str): how the tables are drawn. "scroll" (default): their natural width, with a
///         horizontal scroll if they are wider than the document. "reformat": the cells wrap their text so
///         that the table fits the document (the width of ``max_width``). "full_width": the same, but the
///         table fits the whole available width, out of the column of ``max_width``. The cells keep bold, code,
///         links and images; the alignment of the columns is not applied
///
///     code_width, image_width, mermaid_width (str): what these blocks do when ``max_width`` limits the
///         document. "reformat" (default): the block is fitted to the column of ``max_width``. "full_width":
///         the block takes the whole available width, out of the column
///
///     mermaid_theme (str): theme of the mermaid diagrams: "default", "dark", "forest", "neutral" or "base".
///     Default follows the theme of the app
///
///     open_links (bool): a click on a link opens it in the browser. False: it is only returned, so
///         that the app opens it itself, e.g. a link to another markdown file. Default is True
///
/// Returns:
///     str | None: the url of the link clicked during this frame, as written in the text (a relative
///     path stays relative), else None
///
/// Code blocks marked ``mermaid`` are drawn as diagrams: flowcharts, sequence, class, state, ER, pie,
/// gantt and more. A diagram that does not parse shows its error and its source.
///
/// With a ``table_width`` other than "scroll", or a "full_width" block, the text is cut around those blocks.
/// Only the blocks at the top level of the text are concerned (not those in a list or a quote), a reference
/// link must be defined in the same part of the text as its use, and with ``scrollable`` the whole text is
/// laid out, not only its visible part (slower for very long texts).
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
///     ~~~python
///     print("hello")
///     ~~~
///
///     ~~~mermaid
///     flowchart LR
///         A[Start] --> B{OK?}
///         B -->|Yes| C[Done]
///     ~~~
///     ''')
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn markdown(text: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    let ui = current_ui(&UI)?;
    let mermaid_theme = match kwarg::<String>(kwargs, "mermaid_theme")? {
        Some(theme) if !MERMAID_THEMES.contains(&theme.as_str()) => {
            return Err(PyValueError::new_err(format!(
                "Unknown mermaid theme '{}', expected one of {}",
                theme,
                MERMAID_THEMES.join(", ")
            )))
        }
        theme => theme,
    };
    let align = match kwarg::<String>(kwargs, "align")?.as_deref() {
        None | Some("left") => 0.0,
        Some("center") => 0.5,
        Some("right") => 1.0,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "Unknown align '{}', expected left, center or right",
                other
            )))
        }
    };
    let policy = |name: &str| -> PyResult<bool> {
        match kwarg::<String>(kwargs, name)?.as_deref() {
            None | Some("reformat") => Ok(false),
            Some("full_width") => Ok(true),
            Some(other) => Err(PyValueError::new_err(format!(
                "Unknown {} '{}', expected reformat or full_width",
                name, other
            ))),
        }
    };
    let table = match kwarg::<String>(kwargs, "table_width")?.as_deref() {
        None | Some("scroll") => TableFit::Scroll,
        Some("reformat") => TableFit::Reformat,
        Some("full_width") => TableFit::FullWidth,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "Unknown table_width '{}', expected scroll, reformat or full_width",
                other
            )))
        }
    };
    let options = MarkdownOptions {
        max_image_width: kwarg::<usize>(kwargs, "max_image_width")?,
        default_width: kwarg::<usize>(kwargs, "default_width")?,
        mermaid_theme,
        scrollable: kwarg::<bool>(kwargs, "scrollable")?.unwrap_or(false),
        id: kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "markdown".to_string()),
        open_links: kwarg::<bool>(kwargs, "open_links")?.unwrap_or(true),
        max_width: kwarg::<f32>(kwargs, "max_width")?.filter(|w| *w > 0.0),
        align,
        policies: Policies {
            table,
            code_wide: policy("code_width")?,
            image_wide: policy("image_width")?,
            mermaid_wide: policy("mermaid_width")?,
        },
    };
    Ok(core::show(ui, text, &options))
}

/// Shows source code with syntax highlighting and line numbers, in a scroll area. The text can be
/// selected and copied.
///
/// Highlighted languages: Python, JavaScript and TypeScript, Rust, shell (bash, zsh, and
/// PowerShell as shell), C and C++, JSON, SQL, TOML and YAML, reStructuredText ("rst": titles, directives, comments, lists, fields and inline markup) and Gherkin ("feature": the keywords of the blocks and of the steps in English and French, tags, comments, tables, doc strings and the parameters of an outline). Another language is shown plain.
///
/// Args:
///     text (str): the code
///
///     language (str): the language or the extension of the file: "python", "py", ".rs", "ps1"...
///
/// Keyword args:
///
///     line_numbers (bool): the numbers of the lines in a column at the left. Default is True
///
///     id (str): id of the scroll area, needed if there are several code views. Default is "code_view"
///
/// Example::
///
///     code_view(Path("app.py").read_text(), "py")
#[pyfunction]
#[pyo3(signature = (text, language="", **kwargs))]
unsafe fn code_view(text: &str, language: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let line_numbers = kwarg::<bool>(kwargs, "line_numbers")?.unwrap_or(true);
    let id = kwarg::<String>(kwargs, "id")?.unwrap_or_else(|| "code_view".to_string());
    core::code_view(ui, text, language, line_numbers, &id);
    Ok(())
}

/// Loads, on a thread of its own, the system fonts that the mermaid diagrams of `markdown` need.
///
/// Without it the first diagram loads them (about two seconds, on a thread too: the text of the diagram
/// shows a placeholder until then). Call it once the app is running to have them ready before the first
/// markdown with a diagram is opened. Does nothing the second time.
///
/// Example::
///
///     preload_markdown_fonts()  # at the start of the app
#[pyfunction]
fn preload_markdown_fonts() {
    core::preload_fonts();
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(preload_markdown_fonts, m)?)?;
    m.add_function(wrap_pyfunction!(markdown, m)?)?;
    m.add_function(wrap_pyfunction!(code_view, m)?)?;
    Ok(())
}
