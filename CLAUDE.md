# CLAUDE.md — pyegui

Python bindings for the Rust immediate mode GUI library [egui](https://github.com/emilk/egui),
written with PyO3. Personal fork of `gachilord/pyegui`. The Python module is a single compiled
extension (`pyegui`), there is no Python source: everything public is a `#[pyfunction]` or a
`#[pyclass]` in `src/`.

## Build and run

```bash
python3 -m venv .venv && .venv/bin/pip install maturin
.venv/bin/maturin develop --release     # builds the extension into the venv (about 3 minutes)
.venv/bin/python examples/finder.py     # any file of examples/
```

Launchers to start and pin an app (the Finder example by default; any script, name and icon can be
given, see `--help`):

```bash
python scripts/make_shortcut_windows.py   # Windows: Start menu shortcut (--desktop too), needs pywin32
python scripts/make_app_macos.py          # macOS: ~/Applications/Finder.app, drag it to the Dock
```

On Windows the app has to set the same AppUserModelID as the shortcut (see the end of
`examples/finder.py`), or the taskbar groups its window under Python's icon.

`make develop` is the same without `--release`. Wheels are built by CI (Windows x64 and ARM64, macOS)
and attached to the GitHub Release of a `v*` tag.

## The model in six rules

1. **Immediate mode.** `run_native(title, update_func)` calls `update_func(ctx)` about 60 times per
   second. The whole UI is redrawn each call; nothing is created once and kept in a widget tree.
2. **No callbacks.** A button is `if button_clicked("Save"): ...`. A widget that edits a value takes
   a holder (`Str`, `Int`, `Float`, `Bool`, `Date`, `RGB`) created **outside** the update function and
   reads/writes `holder.value`. See [values](docs/framework/values.md).
3. **Nesting = child ui.** `horizontal(f)`, `group(f)`, `frame(f)`, `scope(f)`, `collapsing(t, f)`...
   call `f()` in a child ui with its own layout and style. Lambdas and closures are the normal way
   to write them. See [layout](docs/framework/layout.md).
4. **Panels first.** `left_panel`, `right_panel`, `top_panel`, `bottom_panel` must be called before
   any other widget of `update_func`, otherwise they are placed inside the current ui. The rest of
   the window is the central panel (what you draw after the panels). The first panel called is the
   outermost one. See [panels](docs/framework/panels.md).
5. **GUI thread only.** The ui stack is a global (`static mut`, see `src/lib.rs`): never call a
   widget function from another thread. Long work goes in a thread that sets a holder or a plain
   variable read by `update_func`.

6. **Layout rules.** `run_native(..., layout=dict(gap=8, rounding=12, background=..., sheet=...,
   opacity=..., outline=..., inner_margin=..., dock=dict(...)))` or `ctx.set_layout(...)` give the look
   of the window once. A window is a sidebar, a central area, a dock (`Dock`), a topbar and a
   bottombar: `sidebar_panel(f)`, `topbar_panel(f)` and `bottombar_panel(f)` draw the boxes, the
   `Dock` and the central area follow the same rules. The **background** is what is seen between the
   boxes, the **sheet** what is seen inside the (rounded) boxes and in the tab of a dock pane (the tab
   that is not active has a fainter sheet). A value is looked up in the call, then in the region, then
   in the global rules, then in the defaults; `DockPane(sheet=...)` colors one pane. Without rules
   nothing changes. See [panels](docs/framework/panels.md) and [styling](docs/framework/styling.md).

Things that bite:

- `key_pressed` returns False while a text field has the focus unless `when_typing=True`. Cmd+C,
  Cmd+X and Cmd+V are turned into egui copy/cut/paste events and may not reach `key_pressed`.
- `path_breadcrumb(max_width=...)` takes exactly what it is given: reserve what follows it in the row with
  its measured width and the row spacing (the "…" button of the Finder is 36.9 wide with its spacing). An
  under-estimate pushes the right side out of the row; a row that learns its widths from what fits then
  goes back and forth at every frame at one window width (see `measure` and `EDIT_BUTTON` in `finder.py`).
- `scroll_area_vertical` and `table*` clip their content to the width they are given; use
  `scroll_area_both` or the automatic horizontal scroll of `table(column_widths=[...])` to reach
  wider content.
- A `TreeView` and a `Dock` are state objects: create them once, outside `update_func`. Their
  nodes/panes may be rebuilt every frame.
- `FileListView`, `FileIconView`, `FileTreeView` and `FileWatcher` are state objects too. A view
  without `watcher=` uses one shared default watcher. The context menu function of a view may read
  and change the view (it is called once the view is drawn).
- A `Terminal` is a state object too. Its shell starts the first time it is drawn (a pane in a tab
  that was never shown has no shell yet; `send` and `run` are kept until then). While it has the
  focus the keys go to the shell and `key_pressed` returns False unless `when_typing=True`.
- On macOS the window buttons are drawn over the content when `fullsize_content=True`: leave about
  26 points free at the top of the first panel.
- `ctx.set_*` style methods are meant to be called on the first frame and when the user changes the
  theme, not every frame.

## Reference by theme

Each file lists every function and class of the theme with its signature, its keyword arguments and
an example. They are generated from the docstrings, see "Keeping the reference up to date".

| Theme | File | What is in it |
|---|---|---|
| Application and context | [app.md](docs/framework/app.md) | `run_native`, `Context` (clipboard, open_url), `Backdrop`, `pixels_per_point` |
| Values | [values.md](docs/framework/values.md) | `Str`, `Int`, `Float`, `Bool`, `Date`, `RGB`, `RGBA`, `Response` |
| Text | [text.md](docs/framework/text.md) | headings, labels, code, code_view (syntax highlighting), links, badges, markdown with mermaid |
| Buttons and inputs | [inputs.md](docs/framework/inputs.md) | buttons, checkbox, radio, switch, sliders, text edits, color and date pickers, progress, `Tags`/`tags_edit` (tags with levels and colors), `TagTreeView`/`tag_tree_view` (their tree) |
| Layout | [layout.md](docs/framework/layout.md) | horizontal/vertical, grid, columns, group, frame, sizes, scroll areas, enable/disable |
| Panels and popups | [panels.md](docs/framework/panels.md) | `sidebar_panel`/`topbar_panel`/`bottombar_panel`, side/top/bottom panels, window, modal, menus, tooltips, context menu |
| Data | [data.md](docs/framework/data.md) | `table`, `table_ui`, `tree_view`, plots |
| Docking | [docking.md](docs/framework/docking.md) | `Dock`, `DockPane`, `DockContainer`, `DockTabBar`, `dock_area` |
| Terminal | [terminal.md](docs/framework/terminal.md) | `Terminal` (a shell in a pseudo terminal: `run`, `send`, `restart`, `title`, `text`, `set_colors` for light and dark, `font`), `terminal` to draw it, e.g. in a `DockPane` |
| File views | [files.md](docs/framework/files.md) | `FileListView`, `FileIconView`, `FileTreeView` and their draw functions, `FileWatcher` (folders kept up to date with the disk), `path_breadcrumb` and `path_combo` |
| Images and icons | [images.md](docs/framework/images.md) | `image`, image buttons, tinted SVG icons, native `file_icon` |
| PDF | [pdf.md](docs/framework/pdf.md) | `pdf_info` (metadata, dates, rights: replaces pypdf), `PdfDocument` (async open, `text`, `outline`, `render_png`), `PdfView`/`pdf_view` (viewer: scroll, zoom, fit, rotation, dark, search, text selection, links) and `pdf_thumbnail`; Cargo feature `pdf`, on by default |
| Keyboard, mouse, drag and drop | [interaction.md](docs/framework/interaction.md) | `key_pressed`, `typed_text`, `modifiers`, `pointer_pos`, `interactive` (a clickable/draggable area with its `rect`), `draw_rect`, `dnd_drag_source`, `dnd_drop_zone`, and the drag and drop with the Finder: `dropped_files`, `hovered_files`, `start_file_drag`, `dragged_files`, `system_modifiers` (the copy and move of what is dropped: `drop_operation`, `FileTransfer` in [files.md](docs/framework/files.md)) |
| Styling | [styling.md](docs/framework/styling.md) | light/dark/system theme, accent, `set_style`, `set_layout` (layout rules), fonts, glass and native backdrops |

Index of all files: [docs/framework/README.md](docs/framework/README.md).

## Examples (`examples/`)

| File | Shows |
|---|---|
| `hello_world.py` | the smallest app |
| `widget_gallery.py`, `demo.py` | most widgets, plots |
| `finder.py` | a mini Finder: sidebar, toolbar, list/icon/tree views, preview, and a right-hand **Dock** with an **Actions** pane (collapsible categories, per-selection enabled state, shortcuts, rename modal, copy/cut/paste), multi-selection in the 3 views (Cmd/Shift click, Cmd+A) and a rubber band in the icon view, path bar; panes can be closed, moved by their tab, and brought back from the gear menu; a Tags column and a Last Opened column in the list and tree views, a Recent pane of the items last opened; folders watched by the `FileWatcher` of pyegui (panes **Dossiers surveillés** and **Activités**, a console of what changes); a SQLite index of folders (`finder_tags.py`; the PDF found are read with `pdf_info` of pyegui by `finder_pdf.py`: pages and protections in the Pages and Prot. columns of the list, metadata in Preview) and an advanced search pane (**Recherche**, with PDF filters); the window, theme, view, folder, dock layout and open tree folders are kept between runs (`persist_id`, `ctx.save_state`, `dock.save`) |
| `app.py` | "Atelier": sidebar, toolbar, tabs and status bar in a glass style |
| `explorer.py` | file explorer in a glass style with a native backdrop, files open in tabs |
| `fileviwer.py` | file browsing with `scandir` |
| `workspace.py`, `python-ide.py`, `pages.py` | docking, panes and pages |
| `glass.py` | glass style and native backdrops |
| `markdown.py` | markdown editor with live preview and mermaid diagrams |
| `dock_colors.py` | layout rules in a glass style: a blue dock and a green dock, semi transparent (`DockPane(sheet=...)`) |
| `markdown_viewer_bars.py`, `markdown_viewer_glass.py`, `markdown_viewer_bars_glass.py` | the same app with a top and a bottom bar, with a glass backdrop, with both: the layout rules (`layout=dict(...)`, `sidebar_panel`, `topbar_panel`, `bottombar_panel`) |
| `terminal.py` | a terminal in a dock: a sidebar of files, a viewer and a bottom dock with a shell and a Python prompt; buttons write commands to the shell (`term.run`); a Réglages pane changes the layout rules (`ctx.set_layout`) and the blur of the backdrop live |
| `files.py` | the three file views (tree, list, icons) sharing one `FileWatcher`, a preview reloaded when the file changes on the disk, a Log dock of the notify events, a `path_combo` title, a `path_breadcrumb` path bar and a slider for the size of the icons |
| `markdown_finder.py` | a markdown viewer in the unified layout of `finder.py`: a `FileTreeView` of the `.md` files of a folder in the sidebar (filtered by the search field), the document in the middle, reloaded when saved, its outline and source in a dock, the path in the bottom band |
| `tags.py` | `Tags` and `tags_edit`: tags with several levels, the last one shown, the full path in the colors of its levels on hover, suggestions while typing, compact dots, `max_visible`; a `TagTreeView` of all the tags in the sidebar |
| `table_columns.py`, `file_columns.py` | `table(view=TableView())` and the file views with an interactive header: sort, filter by type, move, resize and pin columns (right click on a title) |
| `pdf_viewer.py` | a PDF viewer: thumbnails and bookmarks in the sidebar, the pages in the middle (`PdfView`), a toolbar (page, zoom, fit, rotation, dark, hand), a search with its matches marked, text selection, password, drop a file on the window |
| `markdown_viewer.py` | minimal app: a sidebar listing markdown files, a viewer in the middle and three docks on the right (outline, source, infos) |

`finder.py` is the reference for a real desktop layout. Its Actions pane works like the Actions
dock of the PyQt app `pyfinder`: one registry (`ACTIONS`: category, label, icon, shortcut, enabled
predicate, run) read by both the panel and the keyboard, so an action is declared once.

## Source map (`src/`)

| File | Content |
|---|---|
| `lib.rs` | the ui stack, `run_native`, `Context`, value holders, layout and text functions, scroll areas |
| `widgets.rs` | buttons, inputs, `key_pressed`, drag and drop, size helpers |
| `containers.rs` | panels, windows, modals, menus, tooltips, grid, frame |
| `table.rs`, `tree.rs`, `plot.rs` | data widgets (`egui_extras`, `egui_ltreeview`, `egui_plot`) |
| `layout.rs` | layout rules: tokens, regions, `sidebar_panel`, `topbar_panel`, `bottombar_panel`, the central frame |
| `dock.rs` | docking (`egui_dock`) |
| `terminal.rs` | `Terminal` and `terminal`: the pseudo terminal and its grid (`alacritty_terminal`), drawn cell by cell, keys and mouse sent to the shell |
| `markdown.rs` | markdown and mermaid |
| `style.rs` | themes, accent, glass, `set_style` |
| `watcher.rs` | `FileWatcher`: listings of folders cached and watched with `notify`, a change asks for a repaint |
| `files.rs` | `FileListView`, `FileIconView`, `FileTreeView` and their draw functions |
| `path.rs` | `path_breadcrumb` and `path_combo` |
| `tags.rs` | `Tags`, `tags_edit`, `TagTreeView` and `tag_tree_view` |
| `pdf_view.rs` | `PdfDocument`, `PdfView`, `pdf_view`, `pdf_thumbnail`: pages drawn by threads (`craft-pdf` raster), textures kept in a cache with a memory budget, the search thread, selection, rotation |
| `pdf_text.rs` | the text of a page with the box of every character (an interpreter `Device`), search folding, hit tests, selection rectangles |
| `pdf_nav.rs` | the links of a page and the bookmarks of a document, read from the objects of the file |
| `pdf.rs` | `pdf_info`, with `craft-pdf` (git dependency of `storytold/craft-libs` pinned by its rev, with the `[patch.crates-io]` of `Cargo.toml` at the same rev; feature `pdf`) |
| `icons.rs` | `file_icon` (native file icons) |
| `transfer.rs` | `drop_operation`, `same_volume` and `FileTransfer`: copy or move dropped files in a thread, with a progress |
| `native_dnd.rs` | drag and drop with the system: `dropped_files`, `hovered_files`, `start_file_drag` (a drag session of AppKit on macOS), `dragged_files`, the pointer read from the system |

Conventions when adding a function: a `#[pyfunction]` with a docstring in the same style as the
others (summary, `Args:` / `Keyword args:` sections, an `Example::` block), registered in the
`register` function of its file. Optional arguments use `**kwargs` read with `kwarg::<T>(kwargs, "name")?`.
Get the ui with `current_ui(&UI)?` and run nested update functions with `run_nested_update_func`.

## Keeping the reference up to date

```bash
.venv/bin/maturin develop --release
.venv/bin/python scripts/gen_framework_docs.py
```

`scripts/gen_framework_docs.py` rewrites `docs/framework/*.md` from the docstrings of the built
module. The themes and their introductions are in its `THEMES` list. A public name that is in no
theme makes the script fail, so add a new function to a theme there. Never edit the generated
files by hand: fix the docstring in `src/` instead.

Other documentation: `README.rst` (install and usage), `docs/` (Sphinx site), `CHANGELOG.md`,
`TODO.md` (egui features not bound yet), `guides/` (small how-to scripts).
