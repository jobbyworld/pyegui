//! pyegui-core: what pyegui does without Python. The Python extension (`pyegui`) wraps these modules
//! in `#[pyfunction]` and `#[pyclass]`; the Rust applications (`finder-rs`) use them directly.

pub mod debuglog;
pub mod gherkin;
#[cfg(feature = "pdf")]
pub mod markdown_pdf;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "pdf")]
pub mod pdf_nav;
#[cfg(feature = "pdf")]
pub mod pdf_text;
pub mod watcher;
pub mod transfer;
pub mod clipboard;
pub mod native_dnd;
pub mod style;
pub mod window;
pub mod titlebar;
pub mod icons;
pub mod path;
pub mod columns;
pub mod meta;
pub mod table;
pub mod date_range;
pub mod date_input;
pub mod human_date;
pub mod files;
pub mod terminal;
pub mod tags;
pub mod dock_draw;
pub mod segmented;
pub mod markdown;
#[cfg(feature = "pdf")]
pub mod pdf_view;
