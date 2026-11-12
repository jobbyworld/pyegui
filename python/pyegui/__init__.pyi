"""Type stub for the pyegui native extension (src/*.rs). Hand-written, kept in sync with the
Rust #[pyfunction]/#[pyclass] signatures. If you add or change a Python-exposed signature in
Rust, update this file too.
"""

from __future__ import annotations

import datetime
from collections.abc import Callable
from typing import Any, Literal

# Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
Color = tuple[int, int, int] | tuple[int, int, int, int] | str
# (width, color)
Stroke = tuple[float, Color]
# False, (blur, color) or (offset_x, offset_y, blur, spread, color)
Shadow = Literal[False] | tuple[int, Color] | tuple[int, int, int, int, Color]
# (left, top, right, bottom) in points, like Response.rect
Rect = tuple[float, float, float, float]

# reactive values ------------------------------------------------------------

class Str:
    """Stores a string value that can be referenced."""

    value: str
    def __init__(self, value: str) -> None: ...

class Bool:
    """Stores a boolean value that can be referenced."""

    value: bool
    def __init__(self, value: bool) -> None: ...

class Int:
    """Stores an integer value that can be referenced."""

    value: int
    def __init__(self, value: int) -> None: ...

class Float:
    """Stores a float value that can be referenced."""

    value: float
    def __init__(self, value: float) -> None: ...

class Date:
    """Stores a date value that can be referenced, see date_picker_button."""

    value: datetime.date
    def __init__(self, value: datetime.date) -> None: ...

class RGB:
    """An rgb color picker value, channels 0-255. See color_edit_button_rgb."""

    r: float
    g: float
    b: float
    def __init__(self, r: float, g: float, b: float) -> None: ...

class RGBA:
    """An rgba color picker value, channels 0-1. See color_edit_button_rgba."""

    r: float
    g: float
    b: float
    a: float
    def __init__(self, r: float, g: float, b: float, a: float = 1.0) -> None: ...
    def to_tuple(self) -> tuple[int, int, int, int]:
        """The color as (r, g, b, a) with channels between 0 and 255, as taken by the style functions."""
        ...

# context ---------------------------------------------------------------

class Context:
    """Controls global aspects of your app. Passed as the argument of update_func."""

    @property
    def is_light_theme(self) -> bool: ...
    @property
    def is_dark_theme(self) -> bool: ...
    def set_light_theme(self) -> None: ...
    def set_dark_theme(self) -> None: ...
    def set_system_theme(self) -> None: ...
    def set_font(self, source: str) -> None: ...
    def open_url(self, url: str) -> None: ...
    def copy_text(self, text: str) -> None: ...
    def set_style(
        self,
        *,
        theme: Literal["light", "dark"] | None = ...,
        rounding: float = ...,
        widget_rounding: float = ...,
        window_rounding: float = ...,
        panel_fill: Color = ...,
        window_fill: Color = ...,
        window_stroke: Stroke = ...,
        window_shadow: Shadow = ...,
        popup_shadow: Shadow = ...,
        button_fill: Color = ...,
        button_hover_fill: Color = ...,
        button_active_fill: Color = ...,
        button_stroke: Stroke = ...,
        button_hover_stroke: Stroke = ...,
        text_color: Color = ...,
        hyperlink_color: Color = ...,
        selection_color: Color = ...,
        extreme_bg_color: Color = ...,
        faint_bg_color: Color = ...,
        code_bg_color: Color = ...,
        item_spacing: tuple[float, float] = ...,
        button_padding: tuple[float, float] = ...,
        window_margin: float = ...,
        menu_margin: tuple[float, float] = ...,
        body_size: float = ...,
        heading_size: float = ...,
        monospace_size: float = ...,
        animation_time: float = ...,
    ) -> None: ...
    def reset_style(self) -> None: ...
    def set_glass_style(
        self,
        tint: Color | None = None,
        accent: Color | None = None,
        mode: Literal["partial", "full"] = "partial",
    ) -> None: ...
    def set_accent(self, color: Color) -> None: ...
    def set_backdrop(self, backdrop: str) -> None: ...

class Backdrop:
    """Names of the backdrop effects, for run_native(backdrop=...) and ctx.set_backdrop()."""

    MICA: str
    MICA_LIGHT: str
    MICA_DARK: str
    TABBED: str
    TABBED_LIGHT: str
    TABBED_DARK: str
    ACRYLIC: str
    ACRYLIC_LIGHT: str
    ACRYLIC_DARK: str
    BLUR: str
    VIBRANCY: str
    LIQUID_GLASS: str
    LIQUID_GLASS_CLEAR: str

# layout / scope ----------------------------------------------------------

class LayoutType:
    """Values used in Layout."""

    Horizontal: LayoutType
    HorizontalCentered: LayoutType
    HorizontalTop: LayoutType
    HorizontalWrapped: LayoutType
    Vertical: LayoutType
    VerticalCentered: LayoutType
    VerticalCenteredJustified: LayoutType
    CenteredAndJustified: LayoutType

class Layout:
    """Context manager specifying the layout of the widgets added inside the with statement."""

    def __init__(self, layout_type: LayoutType) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None: ...

class Scope:
    """Context manager creating a scope for the contents: temporarily change the Style of a sub-region."""

    def __init__(self) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None: ...

class Group:
    """Context manager visually grouping the contents together. Same keyword args as frame()."""

    def __init__(
        self,
        *,
        fill: Color = ...,
        stroke: Stroke = ...,
        rounding: float = ...,
        inner_margin: float = ...,
        outer_margin: float = ...,
        shadow: Shadow = ...,
    ) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None: ...

# responses ---------------------------------------------------------------

class Response:
    """What the user did with a widget during the frame, returned by label_response,
    selectable_label_response and interactive."""

    clicked: bool
    double_clicked: bool
    secondary_clicked: bool
    hovered: bool
    dragged: bool
    drag_started: bool
    drag_stopped: bool
    rect: Rect

# dock ----------------------------------------------------------------------

class DockPane:
    """A pane of a Dock, shown as a tab."""

    title: str
    update_func: Callable[[], Any]
    closable: bool
    def __init__(
        self, title: str, update_func: Callable[[], Any], closable: bool = False
    ) -> None: ...

class DockContainer:
    """A container of a Dock, holding DockPanes and other DockContainers."""

    kind: Literal["tabs", "horizontal", "vertical", "grid"]
    children: list[DockPane | DockContainer]
    def __init__(
        self,
        kind: Literal["tabs", "horizontal", "vertical", "grid"],
        children: list[DockPane | DockContainer],
    ) -> None: ...

class DockTabBar:
    """A tab bar of a Dock, given to the tab_bar_left, tab_bar_trailing and tab_bar_right callbacks."""

    @property
    def titles(self) -> list[str]: ...
    @property
    def active_title(self) -> str | None: ...
    def add_pane(self, pane: DockPane) -> None: ...
    def set_active(self, title: str) -> None: ...
    def close(self, title: str) -> None: ...

class Dock:
    """Stores a layout of panes that the user can resize, rearrange by dragging tabs, and close."""

    def __init__(
        self,
        root: DockPane | DockContainer,
        id: str = "dock",
        *,
        glass: bool = ...,
        tab_bar_height: float = ...,
        tab_bar_color: Color = ...,
        hide_single_tab_bar: bool = ...,
        tab_style: Literal["classic", "pill"] = ...,
        active_tab_fill: Color = ...,
        inactive_tab_fill: Color = ...,
        hovered_tab_fill: Color = ...,
        tab_spacing: float = ...,
        active_tab_text_color: Color = ...,
        inactive_tab_text_color: Color = ...,
        active_tab_stroke: Stroke = ...,
        inactive_tab_stroke: Stroke = ...,
        tab_title_spacing: float = ...,
        gap_width: float = ...,
        resize_stroke: Stroke = ...,
        drag_preview_color: Color = ...,
        drag_preview_stroke: Stroke = ...,
        pane_stroke: Stroke = ...,
        pane_margin: float = ...,
        tab_bar_left: Callable[[DockTabBar], Any] | None = ...,
        tab_bar_trailing: Callable[[DockTabBar], Any] | None = ...,
        tab_bar_right: Callable[[DockTabBar], Any] | None = ...,
    ) -> None: ...
    @property
    def active_title(self) -> str | None: ...
    @property
    def titles(self) -> list[str]: ...
    def set_style(self, **kwargs: Any) -> None:
        """Same keyword args as Dock's constructor (styling ones). Unset options are kept."""
        ...
    def reset_style(self) -> None: ...
    def set_tab_bar(
        self,
        *,
        left: Callable[[DockTabBar], Any] | None = ...,
        trailing: Callable[[DockTabBar], Any] | None = ...,
        right: Callable[[DockTabBar], Any] | None = ...,
    ) -> None: ...
    def add_pane(self, pane: DockPane) -> None: ...
    def set_active(self, title: str) -> bool: ...

def dock_area(dock: Dock) -> None:
    """Shows a Dock, filling the available space."""
    ...

# tree ------------------------------------------------------------------

class TreeNode:
    """A node of a tree view. A directory if children is a list (even empty), a leaf otherwise."""

    id: int
    label: str
    children: list[TreeNode] | None
    icon: str | None
    def __init__(
        self,
        id: int,
        label: str,
        children: list[TreeNode] | None = None,
        icon: str | None = None,
    ) -> None: ...

class TreeView:
    """Stores the state of a tree view: selected nodes, opened directories and user actions."""

    def __init__(
        self,
        id: str,
        multi_select: bool = False,
        drag_and_drop: bool = False,
        default_open: bool = True,
        activatable_dirs: bool = False,
    ) -> None: ...
    selected: list[int]
    @property
    def activated(self) -> list[int]: ...
    @property
    def moved(self) -> tuple[list[int], int, str, int | None] | None: ...
    def set_open(self, id: int, open: bool) -> None: ...
    def is_open(self, id: int) -> bool: ...

def tree_view(tree: TreeView, nodes: list[TreeNode], scroll_to: int | None = None) -> None:
    """Shows a tree of nodes. Directories can be opened and nodes selected, with keyboard navigation."""
    ...

# plot ------------------------------------------------------------------

class PlotLine:
    """A line of a plot."""

    points: list[tuple[float, float]]
    name: str
    color: tuple[int, int, int] | None
    width: float | None
    fill: float | None
    def __init__(
        self,
        points: list[tuple[float, float]] | list[float],
        name: str = "",
        color: tuple[int, int, int] | None = None,
        width: float | None = None,
        fill: float | None = None,
    ) -> None: ...

class PlotPoints:
    """A scatter plot."""

    points: list[tuple[float, float]]
    name: str
    color: tuple[int, int, int] | None
    radius: float | None
    filled: bool
    def __init__(
        self,
        points: list[tuple[float, float]] | list[float],
        name: str = "",
        color: tuple[int, int, int] | None = None,
        radius: float | None = None,
        filled: bool = True,
    ) -> None: ...

class PlotBars:
    """A bar chart."""

    bars: list[tuple[float, float]]
    name: str
    color: tuple[int, int, int] | None
    width: float | None
    horizontal: bool
    def __init__(
        self,
        bars: list[tuple[float, float]] | list[float],
        name: str = "",
        color: tuple[int, int, int] | None = None,
        width: float | None = None,
        horizontal: bool = False,
    ) -> None: ...

class PlotRule:
    """A horizontal line at the given y, or a vertical line at the given x if vertical is True."""

    value: float
    name: str
    color: tuple[int, int, int] | None
    width: float | None
    vertical: bool
    def __init__(
        self,
        value: float,
        name: str = "",
        color: tuple[int, int, int] | None = None,
        width: float | None = None,
        vertical: bool = False,
    ) -> None: ...

def plot(
    id: str,
    items: list[PlotLine | PlotPoints | PlotBars | PlotRule],
    *,
    width: float = ...,
    height: float = ...,
    view_aspect: float = ...,
    data_aspect: float = ...,
    legend: bool = ...,
    x_label: str = ...,
    y_label: str = ...,
    allow_zoom: bool = ...,
    allow_drag: bool = ...,
    allow_scroll: bool = ...,
) -> tuple[float, float] | None:
    """Shows a plot that can be zoomed and dragged. Returns the plot coordinates of the pointer
    when it hovers the plot, None otherwise."""
    ...

# run / entrypoint --------------------------------------------------------

def run_native(
    app_name: str,
    update_func: Callable[[Context], Any],
    *,
    inner_height: float = ...,
    inner_width: float = ...,
    min_inner_height: float = ...,
    min_inner_width: float = ...,
    max_inner_height: float = ...,
    max_inner_width: float = ...,
    fullscreen: bool = ...,
    maximized: bool = ...,
    resizable: bool = ...,
    transparent: bool = ...,
    icon_path: str = ...,
    backdrop: str | None = ...,
    fullsize_content: bool | None = ...,
) -> None:
    """Creates a window and runs update_func. This is an entrypoint for your GUI application."""
    ...

# text ------------------------------------------------------------------

def heading(text: str) -> None: ...
def monospace(text: str) -> None: ...
def small(text: str) -> None: ...
def strong(text: str) -> None: ...
def weak(text: str) -> None: ...
def label(
    text: str, size: float | None = None, truncate: bool = False, selectable: bool | None = None
) -> None: ...
def code(text: str) -> None: ...
def code_editor(text: Str, desired_width: float | None = None, desired_rows: int | None = None) -> None: ...
def text_edit_singleline(text: Str, *, hint_text: str = ...) -> None: ...
def text_edit_multiline(text: Str, *, hint_text: str = ...) -> None: ...
def colored_label(color: Color, text: str) -> None: ...
def hyperlink(url: str) -> None: ...
def hyperlink_to(label: str, url: str) -> None: ...
def link_clicked(label: str) -> bool: ...
def badge(text: str, color: Color | None = None) -> None: ...
def markdown(
    text: str,
    *,
    scrollable: bool = ...,
    id: str = ...,
    max_image_width: int = ...,
    default_width: int = ...,
    mermaid_theme: Literal["default", "dark", "forest", "neutral", "base"] = ...,
) -> None:
    """Shows markdown text: headings, lists, tables, quotes, code blocks, links and images."""
    ...

# buttons / inputs --------------------------------------------------------

def button_clicked(
    text: str,
    *,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    text_color: Color = ...,
    min_size: tuple[float, float] = ...,
    frame: bool = ...,
) -> bool: ...
def small_button_clicked(text: str) -> bool: ...
def selectable_label_clicked(selected: bool, text: str) -> bool: ...
def checkbox(checked: Bool, text: str) -> None: ...
def toggle_value(selected: Bool, text: str) -> None: ...
def radio_value(current_value: Int, alternative: int, text: str) -> None: ...
def radio_clicked(selected: bool, text: str) -> bool: ...
def selectable_value(current_value: Int, alternative: int, text: str) -> None: ...
def combo_box(current_value: Int, alternatives: list[int], names: list[str], label: str) -> None: ...
def switch(value: Bool, text: str | None = None) -> None: ...
def slider_float(value: Float, min: float, max: float, text: str) -> None: ...
def slider_int(value: Int, min: int, max: int, text: str) -> None: ...
def drag_float(value: Float, min: float, max: float, speed: float) -> None: ...
def drag_int(value: Int, min: int, max: int, speed: int) -> None: ...
def drag_angle(value: Float) -> None: ...
def color_edit_button_rgb(rgb: RGB) -> None: ...
def color_edit_button_rgba(rgba: RGBA) -> None: ...
def date_picker_button(selection: Date) -> None: ...
def progress(value: float) -> None: ...
def spinner() -> None: ...

# images / icons ----------------------------------------------------------

def image(
    source: str, *, max_width: int = ..., max_height: int = ..., size: float = ..., tint: Color = ...
) -> None: ...
def image_and_text_clicked(source: str, text: str) -> bool: ...
def image_button_clicked(
    source: str,
    text: str | None = None,
    size: float = 16.0,
    tint: Color | None = None,
    selected: bool = False,
    frame: bool = True,
) -> bool: ...
def menu_image_button(
    source: str, update_func: Callable[[], Any], size: float = 16.0, tint: Color | None = None
) -> None: ...
def menu_image_text_button(
    source: str, text: str, update_func: Callable[[], Any], size: float = 16.0, tint: Color | None = None
) -> None: ...
def file_icon(path: str, size: float = 32.0) -> str | None:
    """The native icon of a file or folder (as the file manager of the platform shows it), for
    image(). Returns None when the platform has no icon to give (only macOS gives icons for now):
    draw your own then."""
    ...

# layout --------------------------------------------------------------------

def horizontal(update_func: Callable[[], Any]) -> None: ...
def horizontal_centered(update_func: Callable[[], Any]) -> None: ...
def horizontal_top(update_func: Callable[[], Any]) -> None: ...
def horizontal_wrapped(update_func: Callable[[], Any]) -> None: ...
def vertical(update_func: Callable[[], Any]) -> None: ...
def vertical_centered(update_func: Callable[[], Any]) -> None: ...
def vertical_centered_justified(update_func: Callable[[], Any]) -> None: ...
def centered_and_justified(update_func: Callable[[], Any]) -> None: ...
def collapsing(heading: str, update_func: Callable[[], Any]) -> None: ...
def indent(update_func: Callable[[], Any]) -> None: ...
def group(
    update_func: Callable[[], Any],
    *,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def scope(update_func: Callable[[], Any]) -> None: ...
def columns(count: int, update_func: Callable[[int], Any]) -> None: ...
def scroll_area_vertical(update_func: Callable[[], Any]) -> None: ...
def scroll_area_horizontal(update_func: Callable[[], Any]) -> None: ...
def scroll_area_both(update_func: Callable[[], Any]) -> None: ...
def scroll_to_cursor(align: Literal["top", "center", "bottom"] | None = None) -> None: ...
def scroll_to_rect(rect: Rect) -> None: ...
def set_width(width: float) -> None: ...
def set_min_width(width: float) -> None: ...
def set_max_width(width: float) -> None: ...
def set_height(height: float) -> None: ...
def set_min_height(height: float) -> None: ...
def set_max_height(height: float) -> None: ...
def add_space(amount: float) -> None: ...
def add_enabled(enabled: bool, update_func: Callable[[], Any]) -> None: ...
def disable() -> None: ...
def set_invisible() -> None: ...
def set_opacity(opacity: float) -> None: ...
def frame(
    update_func: Callable[[], Any],
    *,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...

# panels / windows / menus --------------------------------------------------

def left_panel(
    id: str,
    update_func: Callable[[], Any],
    *,
    resizable: bool = ...,
    show_separator_line: bool = ...,
    default_size: float = ...,
    min_size: float = ...,
    max_size: float = ...,
    exact_size: float = ...,
    expanded: Bool = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def right_panel(
    id: str,
    update_func: Callable[[], Any],
    *,
    resizable: bool = ...,
    show_separator_line: bool = ...,
    default_size: float = ...,
    min_size: float = ...,
    max_size: float = ...,
    exact_size: float = ...,
    expanded: Bool = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def top_panel(
    id: str,
    update_func: Callable[[], Any],
    *,
    resizable: bool = ...,
    show_separator_line: bool = ...,
    default_size: float = ...,
    min_size: float = ...,
    max_size: float = ...,
    exact_size: float = ...,
    expanded: Bool = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def bottom_panel(
    id: str,
    update_func: Callable[[], Any],
    *,
    resizable: bool = ...,
    show_separator_line: bool = ...,
    default_size: float = ...,
    min_size: float = ...,
    max_size: float = ...,
    exact_size: float = ...,
    expanded: Bool = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def window(
    title: str,
    update_func: Callable[[], Any],
    open: Bool | None = None,
    *,
    id: str = ...,
    resizable: bool = ...,
    collapsible: bool = ...,
    movable: bool = ...,
    title_bar: bool = ...,
    scroll: bool = ...,
    default_width: float = ...,
    default_height: float = ...,
    min_width: float = ...,
    min_height: float = ...,
    default_pos: tuple[float, float] = ...,
    fixed_pos: tuple[float, float] = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> None: ...
def modal(
    id: str,
    update_func: Callable[[], Any],
    *,
    backdrop_color: Color = ...,
    fill: Color = ...,
    stroke: Stroke = ...,
    rounding: float = ...,
    inner_margin: float = ...,
    outer_margin: float = ...,
    shadow: Shadow = ...,
) -> bool:
    """A modal dialog. Returns True when the user wants to close it (escape or a click outside)."""
    ...
def grid(
    id: str,
    update_func: Callable[[], Any],
    *,
    striped: bool = ...,
    num_columns: int = ...,
    min_col_width: float = ...,
    min_row_height: float = ...,
    spacing: tuple[float, float] = ...,
) -> None: ...
def end_row() -> None: ...
def menu_bar(update_func: Callable[[], Any]) -> None: ...
def menu_button(text: str, update_func: Callable[[], Any], size: float | None = None) -> None: ...
def close_menu() -> None: ...
def context_menu(update_func: Callable[[], Any], menu_func: Callable[[], Any]) -> None: ...
def tooltip(text: str, update_func: Callable[[], Any]) -> None: ...
def tooltip_ui(update_func: Callable[[], Any], tooltip_func: Callable[[], Any]) -> None: ...

# tables ----------------------------------------------------------------

def table(
    headers: list[str],
    rows: list[list[str]],
    selected: Int | None = None,
    *,
    id: str = ...,
    striped: bool = ...,
    resizable: bool = ...,
    max_height: float = ...,
    scroll_to_row: int | None = ...,
    column_widths: list[float | None | Literal["remainder"]] = ...,
) -> None: ...
def table_ui(
    headers: list[str],
    num_rows: int,
    cell_func: Callable[[int, int], Any],
    *,
    id: str = ...,
    striped: bool = ...,
    resizable: bool = ...,
    max_height: float = ...,
    scroll_to_row: int | None = ...,
    column_widths: list[float | None | Literal["remainder"]] = ...,
) -> None: ...

# interaction / input --------------------------------------------------------

def interactive(update_func: Callable[[], Any], drag: bool = False) -> Response:
    """Runs update_func in a child ui that reacts to the mouse as a whole: the widgets inside keep
    their own clicks, the rest of the area is clickable."""
    ...
def label_response(text: str, size: float | None = None) -> Response: ...
def selectable_label_response(selected: bool, text: str, size: float | None = None) -> Response: ...
def segmented_control(
    selected: Int,
    options: list[tuple[str | None, str | None]],
    size: float = 15.0,
    tint: Color | None = None,
) -> bool: ...
def key_pressed(shortcut: str, when_typing: bool = False) -> bool: ...
def typed_text() -> str: ...
def modifiers() -> dict[Literal["shift", "command", "alt"], bool]: ...
def pointer_pos() -> tuple[float, float] | None: ...
def draw_rect(rect: Rect, fill: Color | None = None, stroke: Stroke | None = None, rounding: float = 0.0) -> None: ...
def dnd_drag_source(id: str, payload: Any, update_func: Callable[[], Any]) -> None: ...
def dnd_drop_zone(update_func: Callable[[], Any]) -> Any | None: ...

# ui state / introspection ----------------------------------------------

def available_width() -> float: ...
def available_height() -> float: ...
def is_enabled() -> bool: ...
def is_visible() -> bool: ...
def ui_contains_pointer() -> bool: ...
def pixels_per_point() -> float: ...
def separator() -> None: ...
