# CLAUDE.md — pyegui

Python bindings for the Rust immediate mode GUI library [egui](https://github.com/emilk/egui),
written with PyO3. Personal fork of `gachilord/pyegui`. The Python module is a single compiled
extension (`pyegui`), there is no Python source: everything public is a `#[pyfunction]` or a
`#[pyclass]` in `src/`.

## Build and run

```bash
python3 -m venv .venv && .venv/bin/pip install maturin
.venv/bin/maturin develop --release     # builds the extension into the venv (about 3 minutes)
.venv/bin/python examples/finder.py     # or examples/widget_gallery.py
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
| Text | [text.md](docs/framework/text.md) | headings, labels, code, code_view (syntax highlighting), links, badges, markdown with mermaid, `markdown_to_pdf` (a markdown text to a PDF with the look of `markdown()`: themes, cover, table of contents, bookmarks, header and footer) |
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

Two scripts, nothing else to import (`finder.py` holds its own tags database and PDF reader):

| File | Shows |
|---|---|
| `widget_gallery.py` | most widgets, one after the other |
| `finder.py` | a mini Finder, the reference for a real desktop layout: sidebar, toolbar, list/icon/tree views, preview, a right-hand **Dock** with an **Actions** pane, tags (`Tags`, `tags_edit`, `TagTreeView`) kept in a SQLite file (`TagStore`, shared with `finder-rs`), folders watched by the `FileWatcher`, a PDF viewer (`PdfView`, `pdf_thumbnail`, `pdf_info`), terminals in the dock, layout rules and glass, the window, theme, view, folder and dock layout kept between runs |

The folders `examples/icons` (the SVG icons of the bars) and `examples/app_icon` (the icon of the app) are used by
`finder.py` and by `finder-rs`.

`finder.py` is the reference for a real desktop layout. Its Actions pane works like the Actions
dock of the PyQt app `pyfinder`: one registry (`ACTIONS`: category, label, icon, shortcut, enabled
predicate, run) read by both the panel and the keyboard, so an action is declared once.

## Workspace, `core/` and `finder-rs/`

The repository is a Cargo workspace of three crates:

- `pyegui` (root, `src/`): the Python extension built by maturin. It keeps the ui stack, the kwargs and the
  `#[pyfunction]` / `#[pyclass]` wrappers.
- `pyegui-core` (`core/`): what does not depend on Python, shared with the Rust applications: `watcher`
  (`Core`), `transfer` (`Transfer`, `drop_operation_for`), `pdf`, `pdf_nav`, `pdf_text`, `pdf_view`
  (`PdfDocument`, `PdfView`, `pdf_view`, `pdf_thumbnail`), `terminal` (`Terminal`, `show`), `tags` (`Tags`, `tags_edit`, the chips), `dock_draw` (outlines of tabs and panes, corners smoothed over one pixel, `snap`), `titlebar` (window buttons, edge resize), `markdown`
  (`show`, `code_view`), `gherkin`, `style`, `window` (native backdrops, blur, corner radius), `icons`, `path` (breadcrumb, combo), `columns` and `files` (the file views: `FolderView`, `TreeState`, `draw_folder`, `draw_tree`, `Hooks` with Rust closures for badges and extra columns). A module of `src/` that moved to the core keeps only
  its wrappers (see `src/watcher.rs`, `src/terminal.rs`, `src/pdf_view.rs`); options read from kwargs become
  a struct of the core (`TerminalOptions`, `MarkdownOptions`, `PdfViewOptions`...). Still in `src/` because
  they hold Python callbacks: `dock`, `tags`, `table`, `tree`, `widgets`, `containers`; `files` and `columns` keep only the Python classes, the kwargs and the conversion of Python functions to closures.
  `cargo test -p pyegui-core` runs the tests of the core.
- `finder-rs` (`finder-rs/`): the Finder in pure Rust (binary `finder`), the equivalent of `examples/finder.py`.
  `actions.rs` (one registry of actions read by the pane, the context menu and the shortcuts), `app.rs` (window,
  state, file operations), `dock.rs` (`egui_tiles`), `views.rs` (the file views of the core, fed with the folder, selection and sort of the app), `viewer.rs`
  (markdown, code, image, PDF), `terminals.rs`, `tagstore.rs` (SQLite, same schema as `TagStore` of `examples/finder.py`) and
  `tagsui.rs`, `search.rs`, `model.rs`. Build `cargo build --profile dist -p finder-rs`; on macOS
  `python scripts/bundle_finder_macos.py` makes `dist/Finder.app` and `dist/Finder.dmg`. The CI builds
  and attaches the executables of Windows (x64, ARM64), macOS (x86_64, aarch64) and Linux to the release.
  Not done compared with `finder.py`: drag of files
  out of the window, Windows Explorer integration.

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
| `gherkin.rs` | Gherkin (`.feature`) colors for `code_view`: blocks, steps, tags, comments, tables, doc strings, in English and French |
| `markdown.rs` | markdown and mermaid |
| `markdown_pdf.rs` | `markdown_to_pdf`: the markdown is drawn by egui with no window (same code as `markdown()`), cut into pages between two rows of text, and the shapes (glyphs shaped by `harfrust` like egui does, rectangles, images) are written with `craft-pdf` `write` (krilla); the cover, the table of contents and the header and footer are drawn by egui too; the headings are read with `pulldown-cmark` and found again in the shapes by their text |
| `style.rs` | themes, accent, glass, `set_style` |
| `watcher.rs` | `FileWatcher`: listings of folders cached and watched with `notify`, a change asks for a repaint |
| `files.rs` | `FileListView`, `FileIconView`, `FileTreeView` and their draw functions |
| `path.rs` | `path_breadcrumb` and `path_combo` |
| `tags.rs` | `Tags`, `tags_edit`, `TagTreeView` and `tag_tree_view` |
| `pdf_view.rs` | `PdfDocument`, `PdfView`, `pdf_view`, `pdf_thumbnail`: pages drawn by threads (`craft-pdf` raster), textures kept in a cache with a memory budget, the search thread, selection, rotation |
| `pdf_text.rs` | the text of a page with the box of every character (an interpreter `Device`), search folding, hit tests, selection rectangles |
| `viewport.rs` | `Viewport`: other windows of the app (egui immediate viewports) drawn after the main window by `draw_frame` of `lib.rs`, with the same ui stack one window at a time |
| `titlebar.rs` | `custom_titlebar`: `window_controls`, `window_drag_area` and the resize by the edges of a window without a title bar of the system (`edge_resize`, called at each frame) |
| `cursor.rs` | `set_cursor`: custom mouse cursors from a file (`.cur`, `.ani`, `.ico`, `.png`, `.svg`), on Windows only (the window is subclassed to answer `WM_SETCURSOR`); the open and closed hands of the PDF pan are the SVG of `src/cursors/` |
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

Other documentation: `README.rst` (install and usage), `docs/` (Sphinx site), [docs/finder-database.md](docs/finder-database.md) (the SQLite schema shared by `finder.py` and `finder-rs`: tags, items, files, pdf_info, favorites, notes), `CHANGELOG.md`,
`TODO.md` (egui features not bound yet), `guides/` (small how-to scripts).
