# Changelog

## 0.6.0 (unreleased)

Update to egui 0.37 and pyo3 0.30. No function of 0.5.0 was removed.

### Containers and panels
- Panels: `top_panel`, `bottom_panel`, `left_panel`, `right_panel`
- Windows and popups: `window`, `modal`, `tooltip`, `tooltip_ui`, `context_menu`
- Menus: `menu_bar`, `menu_button`, `menu_image_button`, `menu_image_text_button`, `close_menu`
- `grid` with `end_row`, `columns`, `frame`

### Data
- Tables: `table` for texts, `table_ui` to draw each cell
- Tree views: `tree_view` with `TreeView` and `TreeNode`, multiple selection, keyboard navigation, drag and drop
- Plots: `plot` with `PlotLine`, `PlotPoints`, `PlotBars` and `PlotRule`

### Dock
- `dock_area` with `Dock`, `DockPane` and `DockContainer`: panes that can be resized, rearranged by dragging their tabs and closed
- Styling with `Dock(..., **options)` and `Dock.set_style()`: tab bar, tab fills and text colors, outlines, spacing, gaps, drag preview, `glass=True` preset
- The tabs are rounded with the radius of the theme, the active tab has the background of its pane and a light outline joins them
- Widgets in the tab bars with `tab_bar_left`, `tab_bar_trailing` and `tab_bar_right`, given a `DockTabBar` with `add_pane`, `set_active` and `close`

### Markdown
- `markdown(text)`: headings, lists, tables, quotes, code blocks, links and images
- ```` ```mermaid ```` code blocks are drawn as diagrams: flowcharts, sequence, class, state, ER, pie, gantt and more. `mermaid_theme=` picks the theme, which follows the app by default

### Style and glass
- `ctx.set_style()` and `ctx.reset_style()`: colors, rounding, spacing, text sizes
- `ctx.set_glass_style()`: a translucent style looking like frosted glass
- Native backdrops with `run_native(backdrop=...)` and `ctx.set_backdrop()`: Mica, Tabbed and Acrylic on Windows, Vibrancy and Liquid Glass on macOS. `Backdrop` holds their names, like `Backdrop.LIQUID_GLASS`

### Widgets
- `colored_label`, `selectable_label_clicked`, `radio_clicked`
- `RGBA` and `color_edit_button_rgba`: colors with transparency
- `drag_angle`
- Drag and drop of any Python object: `dnd_drag_source` and `dnd_drop_zone`
- `code_editor` takes `desired_width` and `desired_rows`

### Layout and state
- `available_width`, `available_height`
- `set_width`, `set_min_width`, `set_max_width`, `set_height`, `set_min_height`, `set_max_height`
- `scroll_to_cursor`
- `is_enabled`, `is_visible`, `ui_contains_pointer`, `pixels_per_point`

### Fixes
- The build failed on macOS and Linux: `log` and `pyo3-log` were Windows-only dependencies
- Liquid Glass and Vibrancy were drawn over the ui on macOS instead of behind it
- An app could freeze before showing its window when a text had a character missing from the default fonts, like an emoji: the Rust logs of other threads no longer go to Python logging, only their warnings to stderr
- The copy icon of the markdown code blocks was missing

### Examples
- New: `demo.py`, `explorer.py`, `glass.py`, `workspace.py`, `markdown.py`
- `widget_gallery.py` shows the new widgets and scrolls
