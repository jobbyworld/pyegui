# Changelog

## Unreleased

- `run_native(..., persist_id="app.name")`: the position and size of the window and what egui remembers (scroll, collapsed sections) come back at the next run
- `ctx.save_state(obj)` and `ctx.load_state()`: the choices of the user (theme, folder, view...) as JSON, saved with the window. `examples/finder.py` uses them
- `dock.save()` and `dock.load(layout, panes=None)`: the open panes, tabs, splits and sizes of a `Dock` as plain values, to keep with `ctx.save_state()`

- `scripts/make_shortcut_windows.py` and `scripts/make_app_macos.py`: launchers to start and pin an app (the Finder example by default) on the taskbar or the Dock
- `tree_view(..., context_menu=f)`: `f(ids)` is called on a right click on a node
- `tree.focus(focused=True)` and `tree.keep_focus`: give the keyboard focus to a `TreeView` (once, or at every frame unless a text field is being edited), or take it away, so an app can manage the focus of its zones
- `ctx.request_repaint(after=0.0)`: asks for another call of `update_func`, to run an animation
- `pointer_pressed()`: True in the frame the primary mouse button went down, to know which zone was clicked
- `file_icon(path, size)`: also gives the icons of the Windows Explorer (via `IShellItemImageFactory`)

## 0.6.0 (2026-11-11)

Update to egui 0.37 and pyo3 0.30. No function of 0.5.0 was removed.

### Containers and panels
- Panels: `top_panel`, `bottom_panel`, `left_panel`, `right_panel`
- Windows and popups: `window`, `modal`, `tooltip`, `tooltip_ui`, `context_menu`
- Menus: `menu_bar`, `menu_button`, `menu_image_button`, `menu_image_text_button`, `close_menu`
- `grid` with `end_row`, `columns`, `frame`

### Data
- Tables: `table` for texts, `table_ui` to draw each cell, `column_widths=` to size the columns
- Tree views: `tree_view` with `TreeView` and `TreeNode`, multiple selection, keyboard navigation, drag and drop
  - `TreeView(default_open=False)` and `TreeView.is_open`: read the children of a directory only when it is opened
  - `TreeView(activatable_dirs=True)`: a double click on a directory activates it, the arrow still opens and closes it
  - `TreeNode(icon=uri)`: an image before the label
- Plots: `plot` with `PlotLine`, `PlotPoints`, `PlotBars` and `PlotRule`

### Dock
- `dock_area` with `Dock`, `DockPane` and `DockContainer`: panes that can be resized, rearranged by dragging their tabs and closed
- Styling with `Dock(..., **options)` and `Dock.set_style()`: tab bar, tab fills and text colors, outlines, spacing, gaps, drag preview, `glass=True` preset
- The tabs are rounded with the radius of the theme, the active tab has the background of its pane and a light outline joins them
- Widgets in the tab bars with `tab_bar_left`, `tab_bar_trailing` and `tab_bar_right`, given a `DockTabBar` with `add_pane`, `set_active` and `close`
- Pill tabs with `Dock(tab_style="pill")`: rounded tabs floating in the tab bar, the active one filled with the selection color
- `Dock.set_active` and `Dock.active_title`

### Markdown
- `markdown(text)`: headings, lists, tables, quotes, code blocks, links and images
- ```` ```mermaid ```` code blocks are drawn as diagrams: flowcharts, sequence, class, state, ER, pie, gantt and more. `mermaid_theme=` picks the theme, which follows the app by default

### Style and glass
- `ctx.set_style()` and `ctx.reset_style()`: colors, rounding, spacing, text sizes
- `ctx.set_glass_style()`: a translucent style looking like frosted glass
- Modern glass theme: `set_glass_style(accent=..., mode=...)`. In the partial mode, the default, the panels and menus are glass and the content an almost opaque sheet; the full mode makes everything glass. Titles and strong texts use Inter SemiBold. The macOS setting Reduce transparency makes the surfaces opaque
- `ctx.set_accent()`: the accent colors the selection, links, text cursor, active pill tabs and plot items without a color
- `run_native(fullsize_content=True)`: transparent title bar on macOS, the content goes under the window buttons
- Roomier menus with the modern theme, and a `menu_margin` option of `set_style`
- Native backdrops with `run_native(backdrop=...)` and `ctx.set_backdrop()`: Mica, Tabbed and Acrylic on Windows, Vibrancy and Liquid Glass on macOS. `Backdrop` holds their names, like `Backdrop.LIQUID_GLASS`

### Widgets
- `colored_label`, `selectable_label_clicked`, `radio_clicked`
- `switch` and `badge`
- `label(text, size=None)`; `label_response` and `selectable_label_response` return a `Response` with `clicked`, `double_clicked`, `secondary_clicked` and `hovered`
- `image()` and the image menu buttons take a `tint`, so one white SVG icon can follow the theme; `image_button_clicked(source, text, size, tint, selected, frame)`; `menu_button` takes a size
- `file_icon(path, size)`: the Finder icon of a file on macOS, `None` elsewhere
- `key_pressed("Cmd+ArrowUp", when_typing=False)`: keyboard shortcuts
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
- The macOS wheels are built on macos-15, GitHub no longer runs macos-14
- The build failed on macOS and Linux: `log` and `pyo3-log` were Windows-only dependencies
- Liquid Glass and Vibrancy were drawn over the ui on macOS instead of behind it
- An app could freeze before showing its window when a text had a character missing from the default fonts, like an emoji: the Rust logs of other threads no longer go to Python logging, only their warnings to stderr
- The copy icon of the markdown code blocks was missing

### Examples
- New: `demo.py`, `explorer.py`, `glass.py`, `workspace.py`, `markdown.py`
- `app.py`: Atelier, a demo app with a menu bar, sidebar, pill tabs, badges, switches and a status bar in the modern glass theme
- `finder.py`: a mini Finder written only with the Python API: sidebar, list, icon and tree views, Finder icons, preview, path bar, keyboard shortcuts
- `widget_gallery.py` shows the new widgets and scrolls
