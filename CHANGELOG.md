# Changelog

## Unreleased

- Finder example: a Last Opened column in the list and tree views (the hour today, Yesterday, the day, the year when older), for the files opened with their app or in the viewer and the folders entered; a Recent pane in the dock, the last 30 items opened, the latest first, with when and where: a click shows the item in its folder. `TagStore.opened_dates()` reads all the dates in one query
- `path_breadcrumb` is never wider than `max_width`: the ancestors that do not fit even shortened are left out from the root, then the last folder is cut. A deep path in a narrow bar used to push what followed it out of the row, and a row that measures itself (the bottom bar of the Finder) kept changing its width and repainting
- Finder example: the bottom bar no longer goes back and forth when the breadcrumb is shortened: the space before its right side had a minimum of 4 points that pushed it out of the row at every other frame
- `tags_edit(..., muted=[...])`: tags drawn with their colors desaturated; the Finder mutes the tags an item inherits from its folders, in the tree and in the preview
- `file_tree_view(..., extra_columns=[(title, width, cell)])`: columns drawn by Python at the right end of the rows of the tree, with their titles above it, like those of `file_list_view`
- `tags_edit(..., small=True)`: smaller chips, e.g. in a row of a list or of a tree; the chips have an opaque fill, readable over a selected row
- Finder example: a Tags column in the tree view and in the list view (the tags of the item and of its folders, small chips, 2 then +N), in place of the dots; on the selected row the cell edits the own tags of the item, like the preview (one chip, +N and the field)
- `tags_edit`: with `small=True` the field is small too; after Enter, only the widget that added the tag takes the focus back (two widgets can show the same `Tags`)
- `TagTreeView` and `tag_tree_view(view, tags, counts=...)`: the tags as a tree, a node per level (`projet/web` and `projet/app` under `projet`), each in the color of its path with the full path on hover and an optional count at the right. A click selects a level (returned, and kept in `view.selected`), the arrow or a double click folds it (`view.folded`, `expand_all`, `collapse_all`), a right click changes its color or removes it with the tags under it. `removable=False` keeps only the colors in that menu. In `examples/tags.py` as a sidebar
- Finder example: the Tags of the sidebar are a `tag_tree_view` (a count of the items of each tag, a right click recolors it); the preview edits the own tags of the selected item with `tags_edit` (a new path is created with its levels, suggestions from the known tags) and shows those it inherits from its folders
- `Tags` and `tags_edit(tags)`: tags with several levels (`"projet/web/front"`). A chip shows the last level; the mouse over it shows the full path in the color of each level. Each level has its own color (`Tags(colors={"projet": "#3b82f6", "projet/web": ...})`, `set_color`, a right click on a chip), a color chosen from the path otherwise. A cross removes a tag, a field adds one with Enter, with `suggestions=` offered while typing; Backspace in the empty field removes the last one; `editable=False` only shows them, `compact=True` shows a dot per tag (the full path on hover), e.g. in a row of a list. `max_visible=4` shows the first tags then a `+6` badge: its tooltip lists the others, a click shows them all, `−` the first ones again. The text and the cross of a chip are in the color of its outline. New example `examples/tags.py`
- `Terminal(font="...ttf")` (and `term.font`): draws the terminal with a monospace font file, e.g. a Nerd Font for the icons of a prompt; the characters it lacks come from the font of egui. The font is kept through the changes of style
- `Terminal(light=dict(...), dark=dict(...))` and `term.set_colors(light=, dark=)`: the colors of the terminal with each theme (background, foreground, cursor, selection and the 16 colors of the shell); a change of theme changes them at once
- Finder example: a click in a pane gives it the keyboard zone again (it was tested before the pane was drawn, when its ui has no size, so the tree kept taking the focus back from the terminal); while a terminal has the focus the keys are the shell's, Escape no longer sends the focus back to the files; F6 works from a text field or a terminal. The docstring of `ui_contains_pointer` says it tests what is drawn so far
- Finder example: the terminals use an installed Nerd Font (its Mono variant) when there is one, and the One Light and One Dark colors; the Transparence pane is removed, its settings are constants (opacity 1.2, files and docks at 0.3, blur 32, gap 8), and the code that converted old saved layouts is gone
- `key_pressed`, `typed_text`, `modifiers`, `pointer_pos`, `pointer_pressed` and `file_icon` no longer open the central panel: called before the panels of `update_func`, they used to put the panels inside it, 8 points from the borders of the window
- `code_view(text, language)`: source code with syntax highlighting (Python, JavaScript/TypeScript, Rust, shell, PowerShell as shell, C/C++, JSON, SQL, TOML, YAML), line numbers, selection, in a scroll area
- `markdown(...)` returns the url of the link clicked during the frame; `open_links=False` leaves it to the app instead of the browser, e.g. to open a linked markdown file
- `ctx.set_central_margin(margin)`: the inner margin of the central panel (8 points in egui), e.g. 0 so that the content lays out its own gaps
- Panels: `separator_stroke=(width, color)` sets the line between a panel and the rest of the window; `inner_margin` takes `(left, top, right, bottom)` like `outer_margin`
- Finder example: a double click (or Enter) on a markdown or source file (.py, .js, .ts, .rs, .ps1, .sh...) shows it in the central view, reloaded when saved, its links to other markdown files opened there; Fermer, Escape, Back or another folder go back to the files
- Finder and markdown viewer examples: a segmented control of icons (light, dark, system) in the bottom band of the sidebar; new icons `sun`, `moon` and `theme-system`
- `examples/markdown_finder.py`: a markdown viewer in the unified layout of the Finder example, with a `FileTreeView` of the markdown files of a folder for the navigation; a click on a heading of the outline scrolls the document to it; a link to a markdown file opens it in the viewer, at its heading for `file.md#heading`, `#heading` scrolls the document, another file opens with its app and the web in the browser
- Finder example: the panes menu no longer has the transparency slider
- Finder example: a unified layout like the Claude app. The sidebar runs along the whole height without gap nor corners. It and the body are cut in three bands (top, content, bottom) of the same heights; only the body has lines between its bands. The top of the sidebar has a settings menu (theme, glass, accent, tabs, layout), its bottom the drive and its free space. One gap everywhere (8 points): around the dock, against the lines, the border of the window, and between its panes. The layout of boxes is still there (Disposition unifiée off)

## 0.6.1 (2026-11-14)

- CI: wheels for Windows on ARM64 (`win_arm64`), built natively on GitHub's `windows-11-arm` runner, next to Windows x64 and macOS
- Dock: the outlines of the tabs and panes are concentric with their fill (a thick outline no longer lets the fill show at the corners), a rounding larger than a tab is limited like egui limits the fill, and the title of a tab keeps clear of its corners and outline
- Dock: a top corner of a pane under any tab of its bar (not only the active one) stays square, so no notch of background shows under an inactive tab
- Layout rules: a corner of a box that lies in a corner of the window (gap 0) is rounded like the window, so its outline is no longer cut by the rounded window
- `Backdrop.LIQUID_GLASS` and `LIQUID_GLASS_CLEAR`: the glass is rounded like the corners of the window (read from macOS), no more square corners around the rounded content
- `ctx.set_backdrop_blur(radius)`: the blur of what is behind the window, from 0 (sharp) to about 64, macOS only (a CABackdropLayer behind the content, rounded like the window, the layer of the system glass views; looked up at run time)
- `ctx.set_glass_style(..., opacity=1.0, panels=None, windows=None, content=None)`: the transparency of the glass: `opacity` multiplies the opacity of every surface, the others set the opacity (0 to 1) of the panels, of the windows and menus, and of the sheet under the content in the partial mode; `sidebar`, `bars`, `central`, `dock` and `background` (the glass under the whole window, seen in the gaps between the boxes) give one level per part of the window (left/right panels, top/bottom panels, central area, panes of the docks: given, each pane and its tab are filled on its own, the tabs not active fainter and `DockPane(..., central=True)` takes the central level). `reset_style` followed by `set_glass_style` at each frame (a slider) no longer garbles the text: the fonts are installed once, at the end of the frame, and only when they change
- file views: `badges=f(path)` (colored dots, e.g. tags) on the three views, `extra_columns=[(title, width, f(path))]` on the list, `view.paths` to show items of any folders (a search, a tag), `keep_focus`, `scroll_to(path)`, `FileTreeView(activatable_dirs=True)`
- Terminal example: a Réglages pane sets the layout rules live (gap, rounding, opacity, inner margin, width and opacity of the outline, globally and for the sidebar, the top bar and the dock; the background between the boxes; the modern style (Inter, menus of glass); the blur of the backdrop) and copies the matching `layout=dict(...)`
- Finder example: the views are `FileTreeView`, `FileListView` and `FileIconView` with a `FileWatcher`, the title a `path_combo`, the path bar a `path_breadcrumb`; a dock at the bottom with a shell (following the folder shown) and a Python prompt; a slider for the transparency of the glass in the panes menu, and a Transparence tab to try every level of `set_glass_style` (theme, glass mode, global factor, sidebar, top and bottom bars, files, docks, windows, background), the box of the sidebar (gap to the content, margins left/top/bottom, rounding; all at 0 it fills the left of the window with square corners) and the blur of the backdrop live and copy the matching call
- file views: `FileListView` / `file_list_view` (columns Name, Date Modified, Size, Kind, sorted by a click on a header), `FileIconView` / `file_icon_view` (a grid of native icons) and `FileTreeView` / `file_tree_view` (folders read when opened). Selection with Cmd/Shift, rubber band, keyboard (arrows, Home/End, Enter, Cmd+A, Cmd+Up, type to select), history (`back`, `forward`, `up`), filter, hidden files, context menu. `examples/files.py` shows the three
- `path_breadcrumb(path)`: the folders of a path from the root with their icons, the ancestors shortened when room is missing and shown in full under the mouse; `path_combo(path)`: the name of a folder as a title, with its icon and a menu arrow under the mouse and the list of its ancestors on a click. Both take `on_right_click`, `on_middle_click` and `context_menu`, the breadcrumb also `on_click` and `on_double_click`
- `FileIconView.icon_size`: the size of the icons and of the grid, e.g. from a slider (the demo has one, like the Finder)
- `FileWatcher`: the folders the file views show are watched by the system (FSEvents, ReadDirectoryChangesW, inotify), a change made by any program shows at the next frame; `watch(path)` and `changed(path)` to reload a file an editor saved, `list_dir(folder)` for views of your own, `take_events()` for the raw events (the demo shows them in a Log dock)
- `Terminal(cwd=..., shell=..., args=..., env=...)` and `terminal(term)`: a real terminal (a shell in a pseudo terminal, colors, full screen programs, history, selection, copy and paste) to put in a `DockPane` or anywhere; `term.run(command)`, `term.send(text)`, `term.restart()`, `term.title`, `term.text()`. `examples/terminal.py` shows it in a dock
- layout rules: `run_native(..., layout=dict(gap=8, rounding=12, outline=..., sheet=..., opacity=..., inner_margin=..., background=..., sidebar=dict(...), dock=dict(...)))` or `ctx.set_layout(...)` give the spacing, corners, outline and opacity of every region (`sidebar`, `central`, `dock`, `topbar`, `bottombar`); a region or a call overrides the global rules. `sidebar_panel(f)`, `topbar_panel(f)` and `bottombar_panel(f)` draw a box with these rules, keep the same gap all around the boxes and clear the macOS window buttons; `Dock` and the central area follow the same rules: the tab of a pane has the sheet of the pane, the other tabs a fainter one
- `run_native(..., persist_id="app.name")`: the position and size of the window and what egui remembers (scroll, collapsed sections) come back at the next run
- `ctx.save_state(obj)` and `ctx.load_state()`: the choices of the user (theme, folder, view...) as JSON, saved with the window. `examples/finder.py` uses them
- `dock.save()` and `dock.load(layout, panes=None)`: the open panes, tabs, splits and sizes of a `Dock` as plain values, to keep with `ctx.save_state()`

- `scripts/make_shortcut_windows.py` and `scripts/make_app_macos.py`: launchers to start and pin an app (the Finder example by default) on the taskbar or the Dock
- `tree_view(..., context_menu=f)`: `f(ids)` is called on a right click on a node
- `tree.focus(focused=True)` and `tree.keep_focus`: give the keyboard focus to a `TreeView` (once, or at every frame unless a text field is being edited), or take it away, so an app can manage the focus of its zones
- `ctx.request_repaint(after=0.0)`: asks for another call of `update_func`, to run an animation
- `TreeNode(..., badges=[(r, g, b, a), ...])`: colored dots at the right end of the row of a node
- `DockPane(..., sheet=color)`: with layout rules, the background of the pane and of its tab, to give a dock its own color. The top corner of the pane under its tab is square
- `DockPane(..., hide_tab=True)`: hides the tab bar while the pane is alone in its tabs, for this pane only
- `pointer_pressed()`: True in the frame the primary mouse button went down, to know which zone was clicked
- docs: the channels of `RGB` (`color_edit_button_rgb`) are between 0 and 1, not 0 and 255
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
