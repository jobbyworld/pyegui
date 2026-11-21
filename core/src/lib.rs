//! pyegui-core: what pyegui does without Python. The Python extension (`pyegui`) wraps these modules
//! in `#[pyfunction]` and `#[pyclass]`; the Rust applications (`finder-rs`) use them directly.

pub mod gherkin;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "pdf")]
pub mod pdf_nav;
#[cfg(feature = "pdf")]
pub mod pdf_text;
pub mod watcher;
pub mod transfer;
pub mod style;
pub mod window;
pub mod icons;
pub mod path;
pub mod terminal;
pub mod markdown;
#[cfg(feature = "pdf")]
pub mod pdf_view;
