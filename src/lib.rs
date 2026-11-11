#![allow(static_mut_refs)]

use chrono::{Datelike, NaiveDate};
use eframe::egui::{FontData, FontDefinitions, FontFamily};
use eframe::{self, egui};
use egui_extras;
use log::debug;
use pyo3::call::PyCallArgs;
use pyo3::prelude::*;
use pyo3::{
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    types::{PyAny, PyBool, PyDict, PyInt, PyString},
};
use std::sync::{Arc, Mutex};
use std::{fs, ptr};

mod containers;
mod icons;
mod dock;
mod markdown;
mod plot;
mod style;
mod table;
mod tree;
mod widgets;

// state

pub(crate) static mut UI: *mut Vec<*mut egui::Ui> = ptr::null_mut();
static mut APP_MUTEX: Mutex<()> = Mutex::new(());
// central panel is opened lazily, so panels added before any widget take the window's borders
static mut CENTRAL: Option<Box<CentralPanelState>> = None;
// backdrop requested by Context.set_backdrop, applied on the next frame
static PENDING_BACKDROP: Mutex<Option<String>> = Mutex::new(None);

struct CentralPanelState {
    panel_ui: egui::Ui,
    prepared: egui::frame::Prepared,
}

// messages

static APP_MUTEX_ERR: &'static str = "run_simple_native has been called on a separate thread";
pub(crate) static UI_PTR_NULL_ERR: &'static str = "UI ptr is null. This is likely to be a problem with pyegui";
pub(crate) static UI_STACK_ERR: &'static str = "UI stack is empty. This is likely to be a problem with pyegui";
pub(crate) static UI_CALL_OUTSIDE_UPDATE_FUNC: &'static str = "UI functions should be called only within update_fun and on the same thread. update_fun should only be called by run_simple_native";

// classes

/// Context object that controls global aspects of your app
///
/// Usage::
///
///     def update_func(ctx):
///         ctx.set_light_theme
///         heading("Using light theme even if system's is dark")
///
///     run_native("My app", update_func)
#[pyclass]
struct Context(egui::Context);

#[pymethods]
impl Context {
    /// True when theme is light
    #[getter]
    fn is_light_theme(&self) -> bool {
        self.0.theme() == egui::Theme::Light
    }

    /// True when theme is dark
    #[getter]
    fn is_dark_theme(&self) -> bool {
        self.0.theme() == egui::Theme::Dark
    }

    /// Sets light theme. Default is system's
    fn set_light_theme(&self) {
        self.0.set_theme(egui::ThemePreference::Light);
    }

    /// Sets dark theme. Default is system's
    fn set_dark_theme(&self) {
        self.0.set_theme(egui::ThemePreference::Dark);
    }

    /// Sets system's theme if it has been changed.
    fn set_system_theme(&self) {
        self.0.set_theme(egui::ThemePreference::System);
    }

    /// Tell egui which fonts to use.
    ///
    /// The default egui fonts only support latin and cyrillic alphabets, but you can call this to install additional fonts that support e.g. Japanese characters.
    ///
    /// The new fonts will become active at the start of the next pass. This will overwrite the existing fonts.
    ///  
    /// Example::
    ///
    ///   def update_func(ctx):
    ///     ctx.set_font("NotoSansJP-VariableFont_wght.ttf")
    ///     heading("天気の子")
    fn set_font(&self, source: String) -> PyResult<()> {
        let buf = fs::read(&source).map_err(|e| {
            eprintln!("Cannot open '{}': {}", source, e.to_string());
            e
        })?;

        let mut fonts = app_fonts(style::theme_state().glass.is_some());
        fonts.font_data.insert(
            source.clone(),
            Arc::new(
                // .ttf and .otf supported
                FontData::from_owned(buf),
            ),
        );
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push(source.clone());

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, source);

        self.0.set_fonts(fonts);

        Ok(())
    }

    /// Open an URL in a browser.
    fn open_url(&self, url: &str) {
        self.0.open_url(egui::OpenUrl::new_tab(url));
    }

    /// Copy the given text to the system clipboard.
    fn copy_text(&self, text: String) {
        self.0.copy_text(text);
    }

    /// Changes the style of the app. Only the given options are changed.
    ///
    /// The style is kept between frames, so this can be called once, for example on the first frame.
    ///
    /// Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]". Strokes are (width, color).
    /// Shadows are False, (blur, color) or (offset_x, offset_y, blur, spread, color).
    ///
    /// Keyword args:
    ///
    ///     theme (str): "light" or "dark" to change only the style of this theme. Default changes both
    ///
    ///     rounding (float): corner radius of widgets, windows and menus
    ///
    ///     widget_rounding (float), window_rounding (float): corner radius of widgets or windows and menus only
    ///
    ///     panel_fill (color): background of the panels and the central panel
    ///
    ///     window_fill (color), window_stroke (stroke), window_shadow (shadow): look of windows, menus and popups
    ///
    ///     popup_shadow (shadow): shadow of menus, tooltips and combo boxes
    ///
    ///     button_fill (color), button_hover_fill (color), button_active_fill (color): background of buttons and other interactive widgets
    ///
    ///     button_stroke (stroke), button_hover_stroke (stroke): border of interactive widgets
    ///
    ///     text_color (color), hyperlink_color (color), selection_color (color)
    ///
    ///     extreme_bg_color (color): background of text edits, plots and scroll bars
    ///
    ///     faint_bg_color (color): background of striped rows
    ///
    ///     code_bg_color (color): background of code
    ///
    ///     item_spacing (tuple[float, float]), button_padding (tuple[float, float]), window_margin (float)
    ///
    ///     menu_margin (tuple[float, float]): horizontal and vertical space inside the menus
    ///
    ///     body_size (float), heading_size (float), monospace_size (float): font sizes
    ///
    ///     animation_time (float): duration of animations in seconds
    ///
    /// Example::
    ///
    ///     def update_func(ctx):
    ///         ctx.set_style(rounding=12, button_fill="#3b82f6", text_color=(240, 240, 240), body_size=15)
    #[pyo3(signature = (**kwargs))]
    fn set_style(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let options = style::StyleOptions::from_kwargs(kwargs)?;

        match options.theme {
            Some(theme) => self.0.style_mut_of(theme, |s| options.apply(s)),
            None => self.0.all_styles_mut(|s| options.apply(s)),
        }
        Ok(())
    }

    /// Restores the default style of both themes.
    fn reset_style(&self) {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            self.0.set_style_of(theme, style::default_style(theme));
        }
        let was_modern = style::theme_state().glass.is_some();
        if let Ok(mut theme) = style::THEME.lock() {
            *theme = style::ThemeState::default();
        }
        if was_modern {
            self.0.set_fonts(app_fonts(false));
        }
    }

    /// Applies a modern style looking like frosted glass, for both themes: translucent surfaces,
    /// the Inter font with semibold titles, and an accent color for the selection, links, active
    /// tabs and plots. With the system setting "Reduce transparency" (macOS), the surfaces are opaque.
    ///
    /// Works best with a backdrop effect: run_native(..., backdrop=Backdrop.MICA).
    /// It can be customized afterwards with set_style. reset_style removes it.
    ///
    /// Args:
    ///     tint (color): color of the glass. Default is white on light theme and smoked on dark theme
    ///
    ///     accent (color): color of the selection, links, active tabs and plots. Default is blue
    ///
    ///     mode (str): "partial": the panels and menus are glass, the content is an almost opaque
    ///         sheet, which is easier to read. "full": every surface is glass. Default is "partial"
    ///
    /// Example::
    ///
    ///     def update_func(ctx):
    ///         ctx.set_glass_style(accent="#8b5cf6")
    ///         heading("Glass")
    ///
    ///     run_native("Glass", update_func, backdrop=Backdrop.ACRYLIC)
    #[pyo3(signature = (tint=None, accent=None, mode="partial"))]
    fn set_glass_style(
        &self,
        tint: Option<&Bound<'_, PyAny>>,
        accent: Option<&Bound<'_, PyAny>>,
        mode: &str,
    ) -> PyResult<()> {
        let tint = tint.map(style::extract_color).transpose()?;
        let accent = accent
            .map(style::extract_color)
            .transpose()?
            .unwrap_or(style::DEFAULT_ACCENT);
        let mode = match mode {
            "partial" => style::GlassMode::Partial,
            "full" => style::GlassMode::Full,
            other => {
                return Err(PyValueError::new_err(format!(
                    "Unknown glass mode '{}', expected partial or full",
                    other
                )))
            }
        };

        let was_modern = style::theme_state().glass.is_some();
        if let Ok(mut theme) = style::THEME.lock() {
            theme.accent = Some(accent);
            theme.glass = Some(mode);
        }
        if !was_modern {
            self.0.set_fonts(app_fonts(true));
        }
        self.0
            .all_styles_mut(|s| style::modern_glass_style(s, tint, accent, mode));
        Ok(())
    }

    /// Changes the accent color: selection, links, text cursor, active pill tabs and the plot
    /// items without a color.
    ///
    /// Example::
    ///
    ///     ctx.set_accent("#22c55e")
    fn set_accent(&self, color: &Bound<'_, PyAny>) -> PyResult<()> {
        let accent = style::extract_color(color)?;
        if let Ok(mut theme) = style::THEME.lock() {
            theme.accent = Some(accent);
        }
        self.0.all_styles_mut(|s| style::accent_style(s, accent));
        Ok(())
    }

    /// Changes the backdrop effect behind the window, for example to follow the theme.
    ///
    /// The window must have been created with a backdrop: run_native(..., backdrop=Backdrop.MICA).
    ///
    /// Args:
    ///     backdrop (str): same values as the backdrop argument of run_native
    ///
    /// Example::
    ///
    ///     ctx.set_dark_theme()
    ///     ctx.set_backdrop(Backdrop.MICA_DARK)
    fn set_backdrop(&self, backdrop: String) -> PyResult<()> {
        check_backdrop(&backdrop)?;

        *PENDING_BACKDROP
            .lock()
            .map_err(|_| PyRuntimeError::new_err("backdrop lock is poisoned"))? = Some(backdrop);
        self.0.request_repaint();
        Ok(())
    }
}

/// Str stores string value that can be referenced
///
/// Usage::
///
///     data = Str("")
///     
///     def update_func():
///         heading(f"Value of the data is {data.value}")
///         if button_clicked("Add :)"):
///             data.value += ":) "
#[pyclass]
struct Str {
    #[pyo3(get, set)]
    value: String,
}

#[pymethods]
impl Str {
    #[new]
    fn new(value: String) -> Self {
        Str { value }
    }
}

/// Bool stores a boolean value that can be referenced
///
/// Usage::
///
///     data = Bool(False)
///     
///     def update_func():
///         heading(f"Value of the data is {data.value}")
///         # button will be shown only if the checkbox is checked
///         if data.value and button_clicked("set to False"):
///             # hiding the button
///             data.value = False
///         checkbox(data, "Check me")
#[pyclass]
pub(crate) struct Bool {
    #[pyo3(get, set)]
    pub(crate) value: bool,
}

#[pymethods]
impl Bool {
    #[new]
    fn new(value: bool) -> Self {
        Bool { value }
    }
}

/// Int stores integer value that can be referenced
///
/// Usage::
///
///     data = Int(69)
///     
///     def update_func():
///         heading(f"Value of the data is {data.value}")
///         if button_clicked("Increment"):
///             data.value += 1
#[pyclass]
struct Int {
    #[pyo3(get, set)]
    value: i32,
}

#[pymethods]
impl Int {
    #[new]
    fn new(value: i32) -> Self {
        Int { value }
    }
}

/// Float stores float value that can be referenced
///
/// Usage::
///
///     data = Float(69.0)
///     
///     def update_func():
///         heading(f"Value of the data is {data.value}")
///         if button_clicked("Increment"):
///             # what can go wrong?
///             data.value += 0.1
#[pyclass]
pub(crate) struct Float {
    #[pyo3(get, set)]
    pub(crate) value: f32,
}

#[pymethods]
impl Float {
    #[new]
    fn new(value: f32) -> Self {
        Float { value }
    }
}

/// Rgb color picker
///
/// Usage::
///
///     color_rgb = RGB(69, 69, 69)
///     color_edit_button_rgb(color_rgb)
#[pyclass]
struct RGB {
    #[pyo3(get, set)]
    r: f32,
    #[pyo3(get, set)]
    g: f32,
    #[pyo3(get, set)]
    b: f32,
}

#[pymethods]
impl RGB {
    #[new]
    fn new(r: f32, g: f32, b: f32) -> Self {
        RGB { r, g, b }
    }
}

/// Date picker
///
/// Usage::
///
///     date = Date(datetime.datetime.now())
///     date_picker_button(date)
#[pyclass]
struct Date {
    #[pyo3(get, set)]
    value: NaiveDate,
}

#[pymethods]
impl Date {
    #[new]
    fn new(value: NaiveDate) -> Self {
        Date { value }
    }
}

/// Values of this enum are used in Layout
#[pyclass(from_py_object)]
#[derive(Clone)]
enum LayoutType {
    Horizontal,
    HorizontalCentered,
    HorizontalTop,
    HorizontalWrapped,
    Vertical,
    VerticalCentered,
    VerticalCenteredJustified,
    CenteredAndJustified,
}

/// Layout class can be used to specify layout of widgets that go after with statement.
///
/// Usage::
///
///     with Layout(LayoutType.VerticalCentered):
///        heading("This widget will be horizontally centered")
///
/// Layout types:
///
/// **Horizontal**
///
/// Start a ui with horizontal layout.
///
/// Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. Centering is almost always what you want if you are planning to mix widgets or use different types of text.
///
/// If you don’t want the contents to be centered, use HorizontalTop instead.
///
/// **HorizontalCentered**
///
/// Like Horizontal, but allocates the full vertical height and then centers elements vertically.
///
/// **HorizontalTop**
///
/// Like Horizontal, but aligns content with top.
///
/// **HorizontalWrapped**
///
/// Start a ui with horizontal layout that wraps to a new row when it reaches the right edge.
///
/// Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. Centering is almost always what you want if you are planning to mix widgets or use different types of text.
///
/// **Vertical**
///
/// Start a ui with vertical layout. Widgets will be left-justified.
///
/// **VerticalCentered**
///
/// Start a ui with vertical layout. Widgets will be horizontally centered.
///
/// **VerticalCenteredJustified**
///
/// Start a ui with vertical layout. Widgets will be horizontally centered and justified (fill full width).
///
/// **CenteredAndJustified**
///
/// This will make the next added widget centered and justified in the available space.
///
/// Only one child widget is allowed!
#[pyclass]
struct Layout {
    ui: egui::Ui,
}

#[pymethods]
impl Layout {
    #[new]
    fn __new__(layout_type: LayoutType) -> PyResult<Self> {
        unsafe {
            let parent_ui = current_ui(&UI)?;

            // build layout based on scope type
            let layout = match layout_type {
                LayoutType::HorizontalCentered => egui::Layout::left_to_right(egui::Align::Center)
                    .with_cross_align(egui::Align::Center),
                LayoutType::Horizontal => egui::Layout::left_to_right(egui::Align::Center)
                    .with_main_wrap(false)
                    .with_cross_align(egui::Align::Min),
                LayoutType::HorizontalTop => egui::Layout::left_to_right(egui::Align::Center)
                    .with_cross_align(egui::Align::Min),
                LayoutType::HorizontalWrapped => egui::Layout::left_to_right(egui::Align::Center)
                    .with_main_wrap(false)
                    .with_cross_align(egui::Align::Min),
                LayoutType::Vertical => egui::Layout::top_down(egui::Align::Min),
                LayoutType::VerticalCentered => egui::Layout::top_down(egui::Align::Center),
                LayoutType::VerticalCenteredJustified => {
                    egui::Layout::top_down(egui::Align::Center).with_cross_justify(true)
                }
                LayoutType::CenteredAndJustified => {
                    egui::Layout::centered_and_justified(egui::Direction::TopDown)
                }
            };

            let ui = parent_ui.new_child(egui::UiBuilder::new().layout(layout));

            Ok(Self { ui })
        }
    }

    fn __enter__(&mut self) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            ui_stack.push(&raw mut self.ui);
            Ok(())
        }
    }

    fn __exit__(
        &self,
        _exception_type: Bound<'_, PyAny>,
        _exception_value: Bound<'_, PyAny>,
        _exception_traceback: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            let child_ui = ui_stack
                .pop()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?
                .as_mut()
                .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

            let parent_ui = ui_stack
                .last()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?
                .as_mut()
                .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

            parent_ui.advance_cursor_after_rect(child_ui.min_rect());

            Ok(())
        }
    }
}

/// Create a scope for the contents.
/// You can use this to temporarily change the Style of a sub-region
///
/// Usage::
///
///     with Scope():
///        set_opacity(0.5)
///        heading("hi")
///        heading("there")
///
///     heading("normal opacity")
#[pyclass]
struct Scope {
    ui: egui::Ui,
}

#[pymethods]
impl Scope {
    #[new]
    fn __new__() -> PyResult<Self> {
        unsafe {
            let parent_ui = current_ui(&UI)?;
            let ui = parent_ui.new_child(egui::UiBuilder::new());
            Ok(Self { ui })
        }
    }

    fn __enter__(&mut self) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            ui_stack.push(&raw mut self.ui);
            Ok(())
        }
    }

    fn __exit__(
        &self,
        _exception_type: Bound<'_, PyAny>,
        _exception_value: Bound<'_, PyAny>,
        _exception_traceback: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            let child_ui = ui_stack
                .pop()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?
                .as_mut()
                .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

            let parent_ui = ui_stack
                .last()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?
                .as_mut()
                .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

            parent_ui.advance_cursor_after_rect(child_ui.min_rect());

            Ok(())
        }
    }
}

/// Visually groups the contents together.
///
/// Accepts the same keyword args as the frame function: fill, stroke, rounding, inner_margin, outer_margin and shadow.
///
/// Usage::
///
///     with Group():
///        heading("hi")
///        heading("there")
///
///     with Group(fill=(255, 255, 255, 40), rounding=12):
///        label("styled group")
#[pyclass]
struct Group {
    prepared: egui::frame::Prepared,
}

#[pymethods]
impl Group {
    #[new]
    #[pyo3(signature = (**kwargs))]
    fn __new__(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        unsafe {
            let parent_ui = current_ui(&UI)?;

            let frame = style::apply_frame_kwargs(egui::Frame::group(&parent_ui.style()), kwargs)?;
            let prepared = frame.begin(parent_ui);

            Ok(Self { prepared })
        }
    }

    fn __enter__(&mut self) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            ui_stack.push(&raw mut self.prepared.content_ui);
            Ok(())
        }
    }

    fn __exit__(
        &self,
        _exception_type: Bound<'_, PyAny>,
        _exception_value: Bound<'_, PyAny>,
        _exception_traceback: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        unsafe {
            let ui_stack = ui_stack(&UI)?;

            let _ = ui_stack
                .pop()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?;

            let mut parent_ui = ui_stack
                .last()
                .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))?
                .as_mut()
                .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

            self.prepared.paint(&mut parent_ui);
            self.prepared.allocate_space(&mut parent_ui);
            // parent_ui.advance_cursor_after_rect(child_ui.min_rect());

            Ok(())
        }
    }
}

/// The fonts of the app: the fonts of egui, or Inter with the modern theme of set_glass_style.
/// The "bold" family is always defined, so a style can use it in both cases.
pub(crate) fn app_fonts(modern: bool) -> FontDefinitions {
    let mut fonts = default_fonts();
    let proportional = fonts.families[&FontFamily::Proportional].clone();

    if modern {
        for (name, data) in [
            ("Inter-Regular", &include_bytes!("fonts/Inter-Regular.ttf")[..]),
            ("Inter-SemiBold", &include_bytes!("fonts/Inter-SemiBold.ttf")[..]),
        ] {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_static(data)));
        }
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "Inter-Regular".to_owned());
        // the other fonts stay as fallbacks for the characters Inter does not have
        let bold = std::iter::once("Inter-SemiBold".to_owned())
            .chain(proportional)
            .collect();
        fonts.families.insert(style::bold_family(), bold);
    } else {
        fonts.families.insert(style::bold_family(), proportional);
    }
    fonts
}

/// The fonts of egui, plus the icon of the copy button of the markdown code blocks.
///
/// egui_commonmark draws it with 🗐, which was in the emoji font of egui before 0.37 and is in
/// no font of the systems. The glyph is cut from emoji-icon-font, see src/fonts.
fn default_fonts() -> FontDefinitions {
    const COPY_ICON: &str = "copy-icon";

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        COPY_ICON.to_owned(),
        Arc::new(
            FontData::from_static(include_bytes!("fonts/copy-icon.ttf")).tweak(
                egui::FontTweak {
                    scale: 0.9,
                    ..Default::default()
                },
            ),
        ),
    );
    // last, it holds a single glyph
    for family in fonts.families.values_mut() {
        family.push(COPY_ICON.to_owned());
    }
    fonts
}

// Start function

struct PyeguiApp<'py> {
    update_func: Bound<'py, PyAny>,
    transparent: bool,
}

impl eframe::App for PyeguiApp<'_> {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.transparent {
            // let the backdrop effect or the desktop show through
            [0.0; 4]
        } else {
            egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx_r = Context(ui.ctx().clone());

        if let Some(backdrop) = PENDING_BACKDROP.lock().ok().and_then(|mut b| b.take()) {
            if let Err(err) = apply_backdrop(&*frame, &backdrop) {
                log::warn!("Cannot apply backdrop '{}': {}", backdrop, err);
            }
        }

        unsafe {
            debug!("Getting ui stack pointer");
            let ui_stack = UI.as_mut().expect(UI_PTR_NULL_ERR);

            debug!("Push root UI");
            ui_stack.push(&raw mut *ui);

            debug!("Execute update_func");

            Python::attach(|py| {
                if let Err(err) = self.update_func.call1((ctx_r,)) {
                    err.display(py);
                }
            });

            debug!("Executed update_func");

            // the central panel background must be painted even if no widget was added
            if CENTRAL.is_none() {
                open_central_panel(ui_stack).expect(UI_STACK_ERR);
            }

            ui_stack.clear();

            debug!("Clear UI stack");

            if let Some(central) = CENTRAL.take() {
                let CentralPanelState {
                    mut panel_ui,
                    mut prepared,
                } = *central;
                let content_ui = &mut prepared.content_ui;
                content_ui.expand_to_include_rect(content_ui.max_rect());
                let response = prepared.end(&mut panel_ui);
                ui.advance_cursor_after_rect(response.rect);
            }
        }
    }
}

/// Creates a window and runs update_func.
/// This is an entrypoint for your GUI application.
///
/// Args:
///     app_name (str): name displayed at the header bar
///
///     update_func (Callable[[Context], None]): your function that draws UI
///
///     inner_height (float): the desired height of the window
///
///     inner_width (float): the desired width of the window
///
///     min_inner_height (float): min height of the window
///
///     min_inner_width (float): min width of the window
///
///     max_inner_height (float): max height of the window
///
///     max_inner_width (float): max width of the window
///
///     fullscreen (bool): whether to open app in fullscreen
///
///     maximized (bool): whether to open app maximized
///
///     resizable (bool): whether our app is resizable
///
///     transparent (bool): whether our app is transparent
///
///     icon_path (str): path to icon in rgba format
///
///     backdrop (str): blurs what is behind the window, which makes it transparent. Use with ctx.set_glass_style().
///     "mica", "tabbed" (Windows 11), "acrylic", "blur" (Windows 10 and 11). Add "_light" or "_dark" to mica, tabbed
///     and acrylic to force their tint, otherwise it follows the theme of the system,
///     "vibrancy" (macOS), "liquid_glass" and the more see-through "liquid_glass_clear" (macOS 26).
///     The Backdrop class has a constant for each of them, like Backdrop.MICA.
///     Ignored with a warning on other platforms
///
///     fullsize_content (bool): macOS: the content fills the window under a transparent title bar,
///     keeping the window buttons at the top left. Leave about 28 points free at the top of the
///     first panel. Ignored on other platforms
///
/// Examples::
///
///     name = Str("")
///     
///     def update_func(ctx):
///         heading(f"Hello, {name.value}!")
///         text_edit_singleline(name)
///         
///         if button_clicked("click me"):
///             print("clicked")
///     
///     run_native("My app", update_func)
///
#[pyfunction]
#[pyo3(signature = (app_name, update_func, **kwargs))]
unsafe fn run_native(
    app_name: &str,
    update_func: Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    debug!("Trying to get the app lock");
    // ensure thread safety
    let _lock = APP_MUTEX
        .try_lock()
        .map_err(|_| PyRuntimeError::new_err(APP_MUTEX_ERR))?;
    // init UI stack
    debug!("Initialzing UI stack");
    let mut ui_stack = Vec::with_capacity(32);
    UI = &raw mut *&mut ui_stack;
    // parse kwargs
    let mut viewport = egui::viewport::ViewportBuilder::default();
    let mut transparent = false;
    let backdrop = kwarg::<String>(kwargs, "backdrop")?;

    if let Some(backdrop) = &backdrop {
        check_backdrop(backdrop)?;
        transparent = true;
        viewport = viewport.with_transparent(true);
    }

    if let Some(kwargs) = kwargs {
        if let (Some(height), Some(width)) = (
            kwargs.get_item("inner_height")?,
            kwargs.get_item("inner_width")?,
        ) {
            viewport = viewport.with_inner_size([
                width.cast::<PyInt>()?.extract()?,
                height.cast::<PyInt>()?.extract()?,
            ]);
        }

        if let (Some(height), Some(width)) = (
            kwargs.get_item("min_inner_height")?,
            kwargs.get_item("min_inner_width")?,
        ) {
            viewport = viewport.with_min_inner_size([
                width.cast::<PyInt>()?.extract()?,
                height.cast::<PyInt>()?.extract()?,
            ]);
        }

        if let (Some(height), Some(width)) = (
            kwargs.get_item("max_inner_height")?,
            kwargs.get_item("max_inner_width")?,
        ) {
            viewport = viewport.with_max_inner_size([
                width.cast::<PyInt>()?.extract()?,
                height.cast::<PyInt>()?.extract()?,
            ]);
        }

        if let Some(fullscreen) = kwargs.get_item("fullscreen")? {
            viewport = viewport.with_fullscreen(fullscreen.cast::<PyBool>()?.extract()?);
        }

        if let Some(maximized) = kwargs.get_item("maximized")? {
            viewport = viewport.with_maximized(maximized.cast::<PyBool>()?.extract()?);
        }

        if let Some(resizable) = kwargs.get_item("resizable")? {
            viewport = viewport.with_resizable(resizable.cast::<PyBool>()?.extract()?);
        }

        if kwarg::<bool>(Some(kwargs), "fullsize_content")?.unwrap_or(false) {
            // macOS: the content goes under a transparent title bar, with the window buttons on it
            viewport = viewport
                .with_fullsize_content_view(true)
                .with_titlebar_shown(false)
                .with_title_shown(false);
        }

        if let Some(is_transparent) = kwargs.get_item("transparent")? {
            transparent |= is_transparent.cast::<PyBool>()?.extract::<bool>()?;
            viewport = viewport.with_transparent(transparent);
        }

        if let Some(icon_path) = kwargs.get_item("icon_path")? {
            let path = icon_path.cast::<PyString>()?.extract::<String>()?;
            let buf = fs::read(path)?;

            let icon_data = eframe::icon_data::from_png_bytes(&buf)
                .map_err(|e| PyOSError::new_err(format!("Failed to decode png file: {}", e)))?;
            viewport = viewport.with_icon(icon_data);
        }
    }

    let options = eframe::NativeOptions {
        viewport,
        // the wgpu surface often has no transparent alpha mode, OpenGL does
        renderer: if transparent {
            eframe::Renderer::Glow
        } else {
            eframe::Renderer::default()
        },
        ..eframe::NativeOptions::default()
    };
    debug!("Creating a window");
    // create a window
    let result = eframe::run_native(
        app_name,
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_fonts(app_fonts(false));

            if let Some(backdrop) = &backdrop {
                if let Err(err) = apply_backdrop(cc, backdrop) {
                    log::warn!("Cannot apply backdrop '{}': {}", backdrop, err);
                }
            }

            Ok(Box::new(PyeguiApp {
                update_func: update_func,
                transparent,
            }))
        }),
    );

    match result {
        Ok(_) => Ok(()),
        Err(err) => Err(PyRuntimeError::new_err(format!(
            "Cannot create a window: {}",
            err.to_string()
        ))),
    }
}

/// Names of the backdrop effects, for run_native(backdrop=...) and ctx.set_backdrop().
/// They are plain strings, so "mica" works as well as Backdrop.MICA.
///
/// - MICA, TABBED: Windows 11
/// - ACRYLIC, BLUR: Windows 10 and 11
/// - the _LIGHT and _DARK variants of MICA, TABBED and ACRYLIC force the tint instead of following
///   the theme of Windows
/// - VIBRANCY: macOS
/// - LIQUID_GLASS, LIQUID_GLASS_CLEAR (barely blurred): macOS 26
///
/// Usage::
///
///     run_native("Glass", update_func, backdrop=Backdrop.LIQUID_GLASS)
///     ctx.set_backdrop(Backdrop.ACRYLIC_DARK)
#[pyclass(frozen)]
struct Backdrop;

#[pymethods]
impl Backdrop {
    #[classattr]
    const MICA: &'static str = "mica";
    #[classattr]
    const MICA_LIGHT: &'static str = "mica_light";
    #[classattr]
    const MICA_DARK: &'static str = "mica_dark";
    #[classattr]
    const TABBED: &'static str = "tabbed";
    #[classattr]
    const TABBED_LIGHT: &'static str = "tabbed_light";
    #[classattr]
    const TABBED_DARK: &'static str = "tabbed_dark";
    #[classattr]
    const ACRYLIC: &'static str = "acrylic";
    #[classattr]
    const ACRYLIC_LIGHT: &'static str = "acrylic_light";
    #[classattr]
    const ACRYLIC_DARK: &'static str = "acrylic_dark";
    #[classattr]
    const BLUR: &'static str = "blur";
    #[classattr]
    const VIBRANCY: &'static str = "vibrancy";
    #[classattr]
    const LIQUID_GLASS: &'static str = "liquid_glass";
    #[classattr]
    const LIQUID_GLASS_CLEAR: &'static str = "liquid_glass_clear";
}

static BACKDROPS: [&str; 13] = [
    "mica",
    "mica_light",
    "mica_dark",
    "tabbed",
    "tabbed_light",
    "tabbed_dark",
    "acrylic",
    "acrylic_light",
    "acrylic_dark",
    "blur",
    "vibrancy",
    "liquid_glass",
    "liquid_glass_clear",
];

fn check_backdrop(backdrop: &str) -> PyResult<()> {
    if BACKDROPS.contains(&backdrop) {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "Unknown backdrop '{}', expected one of {}",
            backdrop,
            BACKDROPS.join(", ")
        )))
    }
}

/// Applies a native blur effect behind the window
fn apply_backdrop(
    window: &impl raw_window_handle::HasWindowHandle,
    backdrop: &str,
) -> Result<(), String> {
    use window_vibrancy::*;

    // "_light" and "_dark" force the tint, otherwise it follows the theme of the system
    let (name, dark) = match backdrop.rsplit_once('_') {
        Some((name, "light")) => (name, Some(false)),
        Some((name, "dark")) => (name, Some(true)),
        _ => (backdrop, None),
    };

    // on macOS the effect view must sit behind the view egui renders into, see backdrop_host
    #[cfg(target_os = "macos")]
    let host = backdrop_host(window)?;
    #[cfg(target_os = "macos")]
    let window = &host;

    let result = match name {
        "mica" => apply_mica(window, dark),
        "tabbed" => apply_tabbed(window, dark),
        "acrylic" => {
            if let Some(dark) = dark {
                set_dark_mode(window, dark)?;
            }
            apply_acrylic(window, None)
        }
        "blur" => apply_blur(window, None),
        "vibrancy" => {
            apply_vibrancy(window, NSVisualEffectMaterial::UnderWindowBackground, None, None)
        }
        #[cfg(target_os = "macos")]
        "liquid_glass" => apply_liquid_glass(window, LiquidGlassOptions::default()),
        // barely blurred: the windows behind stay readable
        #[cfg(target_os = "macos")]
        "liquid_glass_clear" => apply_liquid_glass(
            window,
            LiquidGlassOptions::new(NSGlassEffectViewStyle::Clear),
        ),
        _ => return Err("not supported on this platform".to_string()),
    };
    result.map_err(|e| e.to_string())
}

/// The window handle given to window-vibrancy on macOS.
///
/// window-vibrancy adds its effect view as a subview of the handle's view. egui draws with
/// OpenGL directly into the layer of winit's view, and AppKit always draws subviews above their
/// parent's own content, so the effect would cover the whole UI. Pointing the handle at the
/// superview (the window frame) puts the effect view behind winit's view instead.
#[cfg(target_os = "macos")]
struct BackdropHost(raw_window_handle::AppKitWindowHandle);

#[cfg(target_os = "macos")]
impl raw_window_handle::HasWindowHandle for BackdropHost {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // the frame view lives as long as the window that lends us its handle
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::AppKit(
                self.0,
            ))
        })
    }
}

#[cfg(target_os = "macos")]
fn backdrop_host(window: &impl raw_window_handle::HasWindowHandle) -> Result<BackdropHost, String> {
    use objc2_app_kit::NSView;

    let raw_window_handle::RawWindowHandle::AppKit(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not an AppKit window".to_string());
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let superview = unsafe { view.superview() }.ok_or("the window has no frame view")?;
    let ptr = std::ptr::NonNull::from(&*superview).cast();
    Ok(BackdropHost(raw_window_handle::AppKitWindowHandle::new(ptr)))
}

/// Sets the dark mode of the window, which changes the tint of the system backdrops
#[cfg(target_os = "windows")]
fn set_dark_mode(window: &impl raw_window_handle::HasWindowHandle, dark: bool) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

    let raw_window_handle::RawWindowHandle::Win32(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not a win32 window".to_string());
    };
    let value: u32 = dark.into();

    let result = unsafe {
        DwmSetWindowAttribute(
            handle.hwnd.get() as _,
            DWMWA_USE_IMMERSIVE_DARK_MODE as _,
            (&raw const value).cast(),
            size_of::<u32>() as _,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("DwmSetWindowAttribute failed with {:#x}", result))
    }
}

#[cfg(not(target_os = "windows"))]
fn set_dark_mode(_window: &impl raw_window_handle::HasWindowHandle, _dark: bool) -> Result<(), String> {
    Err("not supported on this platform".to_string())
}

// helpers

pub(crate) unsafe fn ui_stack(ui: &*mut Vec<*mut egui::Ui>) -> PyResult<&mut Vec<*mut egui::Ui>> {
    ui.as_mut()
        .ok_or(PyRuntimeError::new_err(UI_CALL_OUTSIDE_UPDATE_FUNC))
}

unsafe fn last_ui(ui_stack: &mut Vec<*mut egui::Ui>) -> PyResult<&mut egui::Ui> {
    let last_ui = ui_stack
        .last_mut()
        .ok_or(PyRuntimeError::new_err(UI_STACK_ERR))?;

    last_ui
        .as_mut()
        .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))
}

/// Returns the ui widgets should be added to. Opens the central panel if needed.
pub(crate) unsafe fn current_ui(ui: &*mut Vec<*mut egui::Ui>) -> PyResult<&mut egui::Ui> {
    let ui_stack = ui_stack(ui)?;

    if ui_stack.len() == 1 && CENTRAL.is_none() {
        open_central_panel(ui_stack)?;
    }

    last_ui(ui_stack)
}

/// Returns the ui panels should be added to.
///
/// At the top level, before any widget has been added, it is the root ui of the window,
/// so panels are placed on the window's borders around the central panel.
pub(crate) unsafe fn panel_parent_ui(ui: &*mut Vec<*mut egui::Ui>) -> PyResult<&mut egui::Ui> {
    last_ui(ui_stack(ui)?)
}

/// Same as egui::CentralPanel, but contents are added to the ui stack instead of a closure
unsafe fn open_central_panel(ui_stack: &mut Vec<*mut egui::Ui>) -> PyResult<()> {
    let root = ui_stack
        .first()
        .ok_or(PyRuntimeError::new_err(UI_STACK_ERR))?
        .as_mut()
        .ok_or(PyRuntimeError::new_err(UI_PTR_NULL_ERR))?;

    let outer_rect = root.available_rect_before_wrap();
    let mut panel_ui = root.new_child(
        egui::UiBuilder::new()
            .ui_stack_info(egui::UiStackInfo::new(egui::UiKind::CentralPanel))
            .max_rect(outer_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    panel_ui.shrink_clip_rect(outer_rect);

    let mut frame = egui::Frame::central_panel(root.style());
    // partial glass: the content is an almost opaque sheet over the glass
    if let Some(fill) = style::sheet_fill(&root.style().visuals) {
        frame = frame.fill(fill);
    }
    let prepared = frame.begin(&mut panel_ui);
    let central = CENTRAL.insert(Box::new(CentralPanelState { panel_ui, prepared }));

    ui_stack.push(&raw mut central.prepared.content_ui);
    Ok(())
}

pub(crate) unsafe fn run_nested_update_func(
    ui: &mut egui::Ui,
    update_fun: Bound<'_, PyAny>,
) -> PyResult<()> {
    run_nested_call(ui, &update_fun, ())
}

/// Calls update_fun with args while ui is on top of the ui stack
pub(crate) unsafe fn run_nested_call<'py>(
    ui: &mut egui::Ui,
    update_fun: &Bound<'py, PyAny>,
    args: impl PyCallArgs<'py>,
) -> PyResult<()> {
    let ui_stack = ui_stack(&UI).unwrap_unchecked();

    ui_stack.push(&raw mut *ui);

    if let Err(err) = update_fun.call1(args) {
        Python::attach(|py| {
            err.display(py);
        });
    }

    match ui_stack.pop() {
        Some(_) => Ok(()),
        None => Err(PyRuntimeError::new_err(UI_STACK_ERR)),
    }
}

/// Extracts an optional keyword argument. None values are treated as missing.
pub(crate) fn kwarg<'py, T>(kwargs: Option<&Bound<'py, PyDict>>, key: &str) -> PyResult<Option<T>>
where
    T: pyo3::conversion::FromPyObjectOwned<'py>,
{
    let Some(kwargs) = kwargs else {
        return Ok(None);
    };

    match kwargs.get_item(key)? {
        Some(value) if !value.is_none() => Ok(Some(value.extract::<T>().map_err(Into::into)?)),
        _ => Ok(None),
    }
}

/// Runs f with a copy of the value, so the Python code called by f can still modify it.
/// The value is written back only if f has changed it.
pub(crate) fn with_bool_copy<R>(py: Python<'_>, value: &Py<Bool>, f: impl FnOnce(&mut bool) -> R) -> R {
    let before = value.borrow(py).value;
    let mut copy = before;

    let result = f(&mut copy);

    if copy != before {
        value.borrow_mut(py).value = copy;
    }
    result
}

// UI functions

/// Show large text
///
/// Example::
///
///     heading("hello")
#[pyfunction]
unsafe fn heading(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    if style::theme_state().glass.is_some() {
        // the modern theme gives titles the strongest text color
        ui.label(egui::RichText::new(text).heading().strong());
    } else {
        ui.heading(text);
    }
    Ok(())
}

/// Show monospace (fixed width) text.
///
/// Example::
///
///     monospace("hello")
#[pyfunction]
unsafe fn monospace(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.monospace(text);
    Ok(())
}

/// Show small text.
///
/// Example::
///
///     small("hello")
#[pyfunction]
unsafe fn small(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.small(text);
    Ok(())
}

/// Show text that stand out a bit (e.g. slightly brighter).
///
/// Example::
///
///     strong("hello")
#[pyfunction]
unsafe fn strong(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    // semibold with the modern theme, the "bold" family is the regular font otherwise
    ui.label(egui::RichText::new(text).strong().family(style::bold_family()));
    Ok(())
}

/// Show text that is weaker (fainter color).
///
/// Example::
///
///     weak("hello")
#[pyfunction]
unsafe fn weak(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.weak(text);
    Ok(())
}

/// Show some text.
///
/// Args:
///     text (str): the text
///
///     size (float): font size of this text only. Default is the size of the body text
///
///     truncate (bool): keep the text on one line and end it with … when it is too long for the
///         width, instead of wrapping it. Default is False
///
/// Example::
///
///     label("some text")
///     label("📁", size=48)
///     label(file_name, truncate=True)
#[pyfunction]
#[pyo3(signature = (text, size=None, truncate=false))]
unsafe fn label(text: &str, size: Option<f32>, truncate: bool) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let text = match size {
        Some(size) => egui::RichText::new(text).size(size),
        None => egui::RichText::new(text),
    };
    let label = egui::Label::new(text);
    ui.add(if truncate { label.truncate() } else { label });
    Ok(())
}

/// Show text as monospace with a gray background.
///
/// Example::
///
///     code("print(42 + 27)")
#[pyfunction]
unsafe fn code(text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.code(text);
    Ok(())
}

/// Show a multiline monospace text field for code and update the text
///
/// Args:
///     text (Str): the edited text
///
///     desired_width (float): width of the field. float("inf") fills the available width. Default is the width of egui
///
///     desired_rows (int): minimum number of visible rows. Default is 4
///
/// Example::
///
///     text = Str("print(42 + 27)")
///     # inside update func
///     code_editor(text)
///     code_editor(text, desired_width=float("inf"), desired_rows=20)
#[pyfunction]
#[pyo3(signature = (text, desired_width=None, desired_rows=None))]
unsafe fn code_editor(
    text: &mut Str,
    desired_width: Option<f32>,
    desired_rows: Option<usize>,
) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let mut editor = egui::TextEdit::multiline(&mut text.value).code_editor();
    if let Some(width) = desired_width {
        editor = editor.desired_width(width);
    }
    if let Some(rows) = desired_rows {
        editor = editor.desired_rows(rows);
    }
    ui.add(editor);
    Ok(())
}

/// Show singleline text field and update the text
///
/// Example::
///
///     text = Str("editable")
///     # inside update func
///     text_edit_singleline(text, hint_text="hint me bro")
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn text_edit_singleline(text: &mut Str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let mut w = egui::TextEdit::singleline(&mut text.value);

    if let Some(kwargs) = kwargs {
        if let Some(hint_text) = kwargs.get_item("hint_text")? {
            w = w.hint_text(hint_text.cast::<PyString>()?.extract::<String>()?);
        }
    }

    ui.add(w);
    Ok(())
}

/// Show multiline text field and update the text
///
/// Example::
///
///     text = Str("editable")
///     # inside update func
///     text_edit_multiline(text, hint_text="hint")
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn text_edit_multiline(text: &mut Str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let mut w = egui::TextEdit::multiline(&mut text.value);

    if let Some(kwargs) = kwargs {
        if let Some(hint_text) = kwargs.get_item("hint_text")? {
            w = w.hint_text(hint_text.cast::<PyString>()?.extract::<String>()?);
        }
    }

    ui.add(w);
    Ok(())
}

/// Returns true if the button was clicked this frame
///
/// Keyword args:
///
///     fill (color): background color. Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
///
///     stroke (tuple[float, color]): border width and color
///
///     rounding (float): corner radius
///
///     text_color (color): color of the text
///
///     min_size (tuple[float, float]): minimum width and height
///
///     frame (bool): False shows only the text, until hovered
///
/// Example::
///
///     if button_clicked("click me"):
///       print("click me, my friend")
///
///     button_clicked("Save", fill="#3b82f6", text_color="#ffffff", rounding=20, min_size=(120, 32))
#[pyfunction]
#[pyo3(signature = (text, **kwargs))]
unsafe fn button_clicked(text: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<bool> {
    let ui = current_ui(&UI)?;

    let mut rich_text = egui::RichText::new(text);
    if let Some(color) = style::kwarg_color(kwargs, "text_color")? {
        rich_text = rich_text.color(color);
    }

    let mut button = egui::Button::new(rich_text);
    if let Some(fill) = style::kwarg_color(kwargs, "fill")? {
        button = button.fill(fill);
    }
    if let Some(stroke) = style::kwarg_stroke(kwargs, "stroke")? {
        button = button.stroke(stroke);
    }
    if let Some(rounding) = kwarg::<f32>(kwargs, "rounding")? {
        button = button.corner_radius(rounding);
    }
    if let Some((width, height)) = kwarg::<(f32, f32)>(kwargs, "min_size")? {
        button = button.min_size(egui::vec2(width, height));
    }
    if let Some(frame) = kwarg::<bool>(kwargs, "frame")? {
        button = button.frame(frame);
    }

    Ok(ui.add(button).clicked())
}

/// Returns true if the small button was clicked this frame
///
/// Example::
///
///     if small_button_clicked("click me"):
///       print("click me, my friend")
#[pyfunction]
unsafe fn small_button_clicked(text: &str) -> PyResult<bool> {
    let ui = current_ui(&UI)?;

    Ok(ui.small_button(text).clicked())
}

/// Start a ui with horizontal layout. After you have called this, the function registers the contents as any other widget.
///
/// Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. The initial height is style.spacing.interact_size.y. Centering is almost always what you want if you are planning to mix widgets or use different types of text.
///
/// If you don’t want the contents to be centered, use horizontal_top instead.
///
/// Example::
///
///     def horizontal_update_func():
///       heading("I'm horizontal")
///     
///     horizontal(horizontal_update_func)
#[pyfunction]
unsafe fn horizontal(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .horizontal(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Like horizontal, but allocates the full vertical height and then centers elements vertically.
#[pyfunction]
unsafe fn horizontal_centered(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .horizontal_centered(|ui| run_nested_update_func(ui, update_fun))
        .inner
}
/// Like horizontal, but aligns content with top.
#[pyfunction]
unsafe fn horizontal_top(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .horizontal_top(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Start a ui with horizontal layout that wraps to a new row when it reaches the right edge of the max_size. After you have called this, the function registers the contents as any other widget.
///
/// Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. The initial height is style.spacing.interact_size.y. Centering is almost always what you want if you are planning to mix widgets or use different types of text.
#[pyfunction]
unsafe fn horizontal_wrapped(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .horizontal_wrapped(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Start a ui with vertical layout. Widgets will be left-justified.
#[pyfunction]
unsafe fn vertical(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .vertical(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Start a ui with vertical layout. Widgets will be horizontally centered.
#[pyfunction]
unsafe fn vertical_centered(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .vertical_centered(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Start a ui with vertical layout. Widgets will be horizontally centered and justified (fill full width).
#[pyfunction]
unsafe fn vertical_centered_justified(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .vertical_centered_justified(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// This will make the next added widget centered and justified in the available space.
///
/// Only one widget may be added inside update_func!
#[pyfunction]
unsafe fn centered_and_justified(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .centered_and_justified(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// A CollapsingHeader that starts out collapsed.
///
/// Example::
///
///     def update_func():
///       heading("hi")
///     collapsing("collapsed", update_func)
#[pyfunction]
unsafe fn collapsing(heading: &str, update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?.collapsing(heading, |ui| run_nested_update_func(ui, update_fun));
    Ok(())
}

/// Create a child ui which is indented to the right.
/// Example::
///
///     def update_func():
///       heading("I'm indented")
///     indent(update_func)
#[pyfunction]
unsafe fn indent(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .indent("your mom", |ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Visually groups the contents together.
///
/// Example::
///
///     def update_func():
///       heading("hi")
///       heading("there")
///     
///     group(update_func)
/// Accepts the same keyword args as the frame function: fill, stroke, rounding, inner_margin, outer_margin and shadow.
#[pyfunction]
#[pyo3(signature = (update_fun, **kwargs))]
unsafe fn group(update_fun: Bound<'_, PyAny>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    style::apply_frame_kwargs(egui::Frame::group(ui.style()), kwargs)?
        .show(ui, |ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Create a vertical scroll area.
///
/// Example::
///
///     def update_func():
///       heading("hi")
///       heading("there")
///       # a lot of elements
///     
///     scroll_area_vertical(update_func)
#[pyfunction]
unsafe fn scroll_area_vertical(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    // content wider than the area (long names, wide rows) must not paint over its neighbours
    let clip = ui.available_rect_before_wrap();
    ui.scope(|ui| {
        ui.shrink_clip_rect(clip);
        // fills the width, so the scroll bar is at the edge of the area and not of its contents
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| run_nested_update_func(ui, update_fun))
            .inner
    })
    .inner
}


/// Create a scroll area that scrolls in both directions, with a bar for each.
///
/// Example::
///
///     scroll_area_both(wide_and_tall_ui)
#[pyfunction]
unsafe fn scroll_area_both(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    let ui = current_ui(&UI)?;
    let clip = ui.available_rect_before_wrap();
    ui.scope(|ui| {
        ui.shrink_clip_rect(clip);
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| run_nested_update_func(ui, update_fun)).inner
    })
    .inner
}

/// Create a horizontal scroll area.
///
/// Example::
///
///     def update_func():
///       heading("hi")
///       heading("there")
///       # a lot of elements
///     
///     scroll_area_horizontal(update_func)
#[pyfunction]
unsafe fn scroll_area_horizontal(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    egui::ScrollArea::horizontal().show(current_ui(&UI)?, |ui| run_nested_update_func(ui, update_fun)).inner
}

/// Create a scoped child ui.
///
/// You can use this to temporarily change the Style of a sub-region.
///
/// Example::
///
///     def update_func():
///       heading("0.5 opacity")
///       set_opacity(0.5)
///     
///     heading("normal opacity")
///     scope(update_func)
#[pyfunction]
unsafe fn scope(update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .scope(|ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Control float with a slider.
///
/// Example::
///
///     data = Float(5)
///     # inside update_func
///     slider_float(data, 0, 50, "slide me")
#[pyfunction]
unsafe fn slider_float(value: &mut Float, min: f32, max: f32, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add(egui::Slider::new(&mut value.value, min..=max).text(text));
    Ok(())
}

/// Control int with a slider.
///
/// Example::
///
///     data = Int(5)
///     # inside update_func
///     slider_int(data, 0, 50, "slide me")
#[pyfunction]
unsafe fn slider_int(value: &mut Int, min: i32, max: i32, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add(
        egui::Slider::new(&mut value.value, min..=max)
            .text(text)
            .integer(),
    );
    Ok(())
}

/// Control float by dragging the number.
///
/// Example::
///
///     data = Float(5)
///     # inside update_func
///     drag_float(data, 0, 50, 1.5)
#[pyfunction]
unsafe fn drag_float(value: &mut Float, min: f32, max: f32, speed: f32) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add(
        egui::DragValue::new(&mut value.value)
            .speed(speed)
            .range(min..=max),
    );
    Ok(())
}

/// Control int by dragging the number.
///
/// Example::
///
///     data = Int(5)
///     # inside update_func
///     drag_int(data, 0, 50, 1)
#[pyfunction]
unsafe fn drag_int(value: &mut Int, min: i32, max: i32, speed: i32) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add(
        egui::DragValue::new(&mut value.value)
            .speed(speed)
            .range(min..=max),
    );
    Ok(())
}

/// A clickable hyperlink
///
/// Example::
///
///     hyperlink("https://github.com/emilk/egui")
#[pyfunction]
unsafe fn hyperlink(url: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.hyperlink(url);
    Ok(())
}

/// A clickable hyperlink with label
///
/// Example::
///
///     hyperlink_to("egui on GitHub", "https://www.github.com/emilk/egui/")
#[pyfunction]
unsafe fn hyperlink_to(label: &str, url: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.hyperlink_to(label, url);
    Ok(())
}

/// Clickable text, that looks like a hyperlink.
/// To link to a web page, use hyperlink or hyperlink_to.
///
/// Example::
///
///     if link_clicked("egui on GitHub"):
///       print("clicked on a fake link")
#[pyfunction]
unsafe fn link_clicked(label: &str) -> PyResult<bool> {
    let ui = current_ui(&UI)?;

    Ok(ui.link(label).clicked())
}

/// Show a checkbox.
///
/// Example::
///
///     data = Bool(false)
///     # inside update_func
///     checkbox(data, "check me")
#[pyfunction]
unsafe fn checkbox(checked: &mut Bool, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.checkbox(&mut checked.value, text);
    Ok(())
}

/// Acts like a checkbox, but looks like a selectable label.
///
/// Example::
///
///     data = Bool(false)
///     # inside update_func
///     toggle_value(data, "check me")
#[pyfunction]
unsafe fn toggle_value(selected: &mut Bool, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.toggle_value(&mut selected.value, text);
    Ok(())
}

/// Show a radio button. It is selected if current_value == selected_value. If clicked, selected_value is assigned to current_value.
///
/// Example::
///
///     RED = 0
///     GREEN = 1
///     BLUE = 2
///     
///     c = Int(RED)
///     
///     radio_value(c, RED, "red")
///     radio_value(c, GREEN, "green")
///     radio_value(c, BLUE, "blue")
#[pyfunction]
unsafe fn radio_value(current_value: &mut Int, alternative: i32, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.radio_value(&mut current_value.value, alternative, text);
    Ok(())
}

/// Show selectable text. It is selected if current_value == selected_value. If clicked, selected_value is assigned to current_value.
///
/// Example::
///
///     RED = 0
///     GREEN = 1
///     BLUE = 2
///     
///     c = Int(RED)
///     
///     selectable_value(c, RED, "red")
///     selectable_value(c, GREEN, "green")
///     selectable_value(c, BLUE, "blue")
#[pyfunction]
unsafe fn selectable_value(current_value: &mut Int, alternative: i32, text: &str) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.selectable_value(&mut current_value.value, alternative, text);
    Ok(())
}

/// Shows a combo box with values defined in "alternatives" and their corresponding names
/// defined in "names"
///
/// Example::
///
///     RED = 0
///     GREEN = 1
///     BLUE = 2
///
///     data = Int(RED)
///
///     def update_func(a):
///         combo_box(data, [RED, GREEN, BLUE], ["red", "green", "blue"], "choose your fate")
#[pyfunction]
unsafe fn combo_box(
    current_value: &mut Int,
    alternatives: Vec<i32>,
    names: Vec<String>,
    label: &str,
) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    egui::ComboBox::from_label(label)
        .selected_text(
            names
                .get(current_value.value.try_into().unwrap_or(0))
                .unwrap_or(&"Unknown".to_string()),
        )
        .show_ui(ui, |ui| {
            for i in 0..alternatives.len() {
                ui.selectable_value(
                    &mut current_value.value,
                    alternatives[i],
                    names.get(i).unwrap_or(&"Unknown".to_string()),
                );
            }
        });
    Ok(())
}

/// A simple progress bar.
/// value in the [0, 1] range, where 1 means “completed”.
///
/// Example::
///
///     progress(0.5)
#[pyfunction]
unsafe fn progress(value: f32) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add(egui::widgets::ProgressBar::new(value).show_percentage());
    Ok(())
}

/// A spinner widget used to indicate loading.
///
/// Example::
///
///     spinner()
#[pyfunction]
unsafe fn spinner() -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.spinner();
    Ok(())
}

/// Shows a button with the given color. If the user clicks the button, a full color picker is shown.
///
/// Example::
///
///     color = RGB(69, 69, 69)
///     # inside udpate_func
///     color_edit_button_rgb(color)
///     heading(f"r:{color.r} g:{color.g} b:{color.b}")
#[pyfunction]
unsafe fn color_edit_button_rgb(rgb: &mut RGB) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let mut tmp: [f32; 3] = [rgb.r, rgb.g, rgb.b];

    ui.color_edit_button_rgb(&mut tmp);

    rgb.r = tmp[0];
    rgb.g = tmp[1];
    rgb.b = tmp[2];

    Ok(())
}

/// Show an image available at the given uri.
///
/// Keyword args:
///
///     max_width (int), max_height (int): the image is scaled down to fit
///
///     size (float): shows the image in a square of this side, scaled up or down. The space is
///         kept while the image loads, so the layout does not jump when it appears
///
///     tint (color): multiplies the colors of the image. With a white icon, the icon takes the color
///
/// Example::
///
///     image("https://picsum.photos/480")
///     image("file://icons/folder.svg", size=48)
///     image("file://assets/ferris.png", max_height = 50, max_width = 50)
///     image("file://icons/gear.svg", max_width=16, max_height=16, tint=(120, 120, 128))
#[pyfunction]
#[pyo3(signature = (source, **kwargs))]
unsafe fn image(source: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let mut img = egui::Image::new(source);

    if let Some(kwargs) = kwargs {
        if let Some(height) = kwargs.get_item("max_height")? {
            img = img.max_height(height.cast::<PyInt>()?.extract()?);
        }
        if let Some(width) = kwargs.get_item("max_width")? {
            img = img.max_width(width.cast::<PyInt>()?.extract()?);
        }
    }
    if let Some(size) = kwarg::<f32>(kwargs, "size")? {
        img = img.fit_to_exact_size(egui::vec2(size, size));
    }
    if let Some(tint) = style::kwarg_color(kwargs, "tint")? {
        img = img.tint(tint);
    }
    // an image is decoded in the background: draw again until it is there, or it stays blank
    // until the next mouse move
    if let Ok(egui::load::TexturePoll::Pending { .. }) = img.load_for_size(ui.ctx(), ui.available_size()) {
        ui.ctx().request_repaint();
    }
    ui.add(img);
    Ok(())
}

/// Creates a button with an image to the left of the text
///
/// Example::
///
///     if image_and_text_clicked("https://picsum.photos/480", "click me"):
///       print("clicked")
#[pyfunction]
unsafe fn image_and_text_clicked(source: &str, text: &str) -> PyResult<bool> {
    let ui = current_ui(&UI)?;

    Ok(ui.add(egui::Button::image_and_text(source, text)).clicked())
}

/// A visual separator. A horizontal or vertical line on layout.
///
/// Example::
///
///     separator()
#[pyfunction]
unsafe fn separator() -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.separator();
    Ok(())
}

/// Calling set_invisible() will cause all further widgets to be invisible, yet still allocate space.
///
/// The widgets will not be interactive (set_invisible() implies disable()).
///
/// Once invisible, there is no way to make the Ui visible again.
///
/// Example::
///
///     set_invisible()
///     heading("this will not be visible")
#[pyfunction]
unsafe fn set_invisible() -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.set_invisible();
    Ok(())
}

/// Calling disable() will cause the Ui to deny all future interaction and all the widgets will draw with a gray look.
///
/// Usually it is more convenient to use add_enabled.
///
/// Note that once disabled, there is no way to re-enable the Ui.
///
/// Example::
///
///     disable()
///     if button_clicked("you can't click me"):
///       pass
#[pyfunction]
unsafe fn disable() -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.disable();
    Ok(())
}

/// Add a section that is possibly disabled, i.e. greyed out and non-interactive.
///
/// If you call add_enabled from within an already disabled Ui, the result will always be disabled, even if the enabled argument is true.
///
/// Example::
///
///     add_enabled(False, lambda: button_clicked("you can't click me"))
///     button_clicked("but you can click me")
#[pyfunction]
unsafe fn add_enabled(enabled: bool, update_fun: Bound<'_, PyAny>) -> PyResult<()> {
    current_ui(&UI)?
        .add_enabled_ui(enabled, |ui| run_nested_update_func(ui, update_fun))
        .inner
}

/// Make the widget in this Ui semi-transparent.
///
/// opacity must be between 0.0 and 1.0, where 0.0 means fully transparent (i.e., invisible) and 1.0 means fully opaque.
/// Example::
///
///     set_opacity(0.5)
#[pyfunction]
unsafe fn set_opacity(opacity: f32) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.set_opacity(opacity);
    Ok(())
}

/// Shows a date, and will open a date picker popup when clicked.
///
/// Example::
///
///     date = Date(datetime.datetime.now())
///     # inside update_func
///     date_picker_button(date)
#[pyfunction]
unsafe fn date_picker_button(selection: &mut Date) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    let value = selection.value;
    let year = i16::try_from(value.year())
        .map_err(|_| PyValueError::new_err("Date year is out of the supported range"))?;
    let mut date = jiff::civil::Date::new(year, value.month() as i8, value.day() as i8)
        .map_err(|e| PyValueError::new_err(format!("Unsupported date: {}", e)))?;

    ui.add(egui_extras::DatePickerButton::new(&mut date));

    selection.value = NaiveDate::from_ymd_opt(date.year().into(), date.month() as u32, date.day() as u32)
        .ok_or(PyRuntimeError::new_err("Invalid date returned by date picker"))?;
    Ok(())
}

/// Add extra space before the next widget.
///
/// The direction is dependent on the layout.
/// Example::
///
///     add_space(5)
///     heading("I'm so spaced now")
#[pyfunction]
unsafe fn add_space(amount: f32) -> PyResult<()> {
    let ui = current_ui(&UI)?;

    ui.add_space(amount);
    Ok(())
}

/// Forwards the Rust logs to Python logging, only from the thread running Python.
///
/// Logging to Python takes the GIL. A background thread logging while the Python thread holds
/// the GIL and waits for it deadlocks: the system font loader of egui does that when a label
/// shows a character missing from the default fonts, like an emoji or a symbol.
struct PythonThreadLogger {
    inner: pyo3_log::Logger,
    python_thread: std::thread::ThreadId,
}

impl log::Log for PythonThreadLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if std::thread::current().id() == self.python_thread {
            self.inner.log(record);
        } else if record.level() <= log::Level::Warn {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

#[pymodule]
fn pyegui(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let logger = PythonThreadLogger {
        inner: pyo3_log::Logger::new(m.py(), pyo3_log::Caching::LoggersAndLevels)?,
        python_thread: std::thread::current().id(),
    };
    // fails only if the module is imported a second time, the first logger stays
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    // classes
    m.add_class::<Str>()?;
    m.add_class::<Backdrop>()?;
    m.add_class::<Bool>()?;
    m.add_class::<Int>()?;
    m.add_class::<Float>()?;
    m.add_class::<RGB>()?;
    m.add_class::<Date>()?;
    m.add_class::<Context>()?;
    m.add_class::<Layout>()?;
    m.add_class::<LayoutType>()?;
    m.add_class::<Scope>()?;
    m.add_class::<Group>()?;
    // functions
    m.add_function(wrap_pyfunction!(run_native, m)?)?;
    m.add_function(wrap_pyfunction!(heading, m)?)?;
    m.add_function(wrap_pyfunction!(monospace, m)?)?;
    m.add_function(wrap_pyfunction!(small, m)?)?;
    m.add_function(wrap_pyfunction!(strong, m)?)?;
    m.add_function(wrap_pyfunction!(weak, m)?)?;
    m.add_function(wrap_pyfunction!(label, m)?)?;
    m.add_function(wrap_pyfunction!(code, m)?)?;
    m.add_function(wrap_pyfunction!(code_editor, m)?)?;
    m.add_function(wrap_pyfunction!(text_edit_singleline, m)?)?;
    m.add_function(wrap_pyfunction!(text_edit_multiline, m)?)?;
    m.add_function(wrap_pyfunction!(button_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(small_button_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(horizontal, m)?)?;
    m.add_function(wrap_pyfunction!(horizontal_centered, m)?)?;
    m.add_function(wrap_pyfunction!(horizontal_top, m)?)?;
    m.add_function(wrap_pyfunction!(horizontal_wrapped, m)?)?;
    m.add_function(wrap_pyfunction!(vertical, m)?)?;
    m.add_function(wrap_pyfunction!(vertical_centered, m)?)?;
    m.add_function(wrap_pyfunction!(vertical_centered_justified, m)?)?;
    m.add_function(wrap_pyfunction!(centered_and_justified, m)?)?;
    m.add_function(wrap_pyfunction!(collapsing, m)?)?;
    m.add_function(wrap_pyfunction!(indent, m)?)?;
    m.add_function(wrap_pyfunction!(group, m)?)?;
    m.add_function(wrap_pyfunction!(scroll_area_vertical, m)?)?;
    m.add_function(wrap_pyfunction!(scroll_area_horizontal, m)?)?;
    m.add_function(wrap_pyfunction!(scroll_area_both, m)?)?;
    m.add_function(wrap_pyfunction!(scope, m)?)?;
    m.add_function(wrap_pyfunction!(slider_float, m)?)?;
    m.add_function(wrap_pyfunction!(slider_int, m)?)?;
    m.add_function(wrap_pyfunction!(drag_int, m)?)?;
    m.add_function(wrap_pyfunction!(drag_float, m)?)?;
    m.add_function(wrap_pyfunction!(hyperlink, m)?)?;
    m.add_function(wrap_pyfunction!(hyperlink_to, m)?)?;
    m.add_function(wrap_pyfunction!(link_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(checkbox, m)?)?;
    m.add_function(wrap_pyfunction!(radio_value, m)?)?;
    m.add_function(wrap_pyfunction!(toggle_value, m)?)?;
    m.add_function(wrap_pyfunction!(selectable_value, m)?)?;
    m.add_function(wrap_pyfunction!(combo_box, m)?)?;
    m.add_function(wrap_pyfunction!(progress, m)?)?;
    m.add_function(wrap_pyfunction!(spinner, m)?)?;
    m.add_function(wrap_pyfunction!(color_edit_button_rgb, m)?)?;
    m.add_function(wrap_pyfunction!(crate::image, m)?)?;
    m.add_function(wrap_pyfunction!(image_and_text_clicked, m)?)?;
    m.add_function(wrap_pyfunction!(separator, m)?)?;
    m.add_function(wrap_pyfunction!(set_invisible, m)?)?;
    m.add_function(wrap_pyfunction!(disable, m)?)?;
    m.add_function(wrap_pyfunction!(add_enabled, m)?)?;
    m.add_function(wrap_pyfunction!(set_opacity, m)?)?;
    m.add_function(wrap_pyfunction!(date_picker_button, m)?)?;
    m.add_function(wrap_pyfunction!(add_space, m)?)?;
    containers::register(m)?;
    table::register(m)?;
    tree::register(m)?;
    widgets::register(m)?;
    icons::register(m)?;
    dock::register(m)?;
    plot::register(m)?;
    markdown::register(m)?;
    style::register(m)?;
    Ok(())
}
