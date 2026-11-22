//! The icons the file manager of the platform shows for files and folders

use pyegui_core::icons::set_blue as icons_set_blue;
use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};

pub(crate) use pyegui_core::icons::icon_uri;

/// Draws the icons of the folders in blue, in the style of the Finder of macOS, instead of the
/// yellow of the Explorer of Windows. It changes the icons given by file_icon and drawn by the
/// file views. It has no effect on macOS (already blue) nor on Linux. Default is False
///
/// Args:
///     enabled (bool): True for blue folders, False for the native ones
///
/// Example::
///
///     set_blue_folders(True)  # once, before the first frame
#[pyfunction]
#[pyo3(signature = (enabled=true))]
fn set_blue_folders(enabled: bool) {
    icons_set_blue(enabled);
}

/// Returns the native icon of a file or folder, as the file manager of the platform shows it,
/// to draw with image(). Returns None when the platform has no icon to give: draw your own then.
///
/// macOS gives the icons of the Finder and Windows the ones of the Explorer (the shell, so the
/// icons of the file types installed on the machine, thumbnails excluded). Linux gives none.
///
/// Args:
///     path (str): path of the file or folder
///
///     size (float): size of the icon in points, it is rendered for the pixel density of the screen.
///         Default is 32
///
/// Example::
///
///     icon = file_icon("/Applications", 48)
///     if icon:
///         image(icon, max_width=48, max_height=48)
///     else:
///         label("📁", size=40)
#[pyfunction]
#[pyo3(signature = (path, size=32.0))]
unsafe fn file_icon(path: &str, size: f32) -> PyResult<Option<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone(); // no widget: the central panel is not opened
    Ok(icon_uri(&ctx, path, size))
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(file_icon, m)?)?;
    m.add_function(wrap_pyfunction!(set_blue_folders, m)?)?;
    Ok(())
}
