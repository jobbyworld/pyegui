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

`make develop` is the same without `--release`. Wheels are built by CI (Windows and macOS only)
and attached to the GitHub Release of a `v*` tag.

## The model in five rules

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

Things that bite:

- `key_pressed` returns False while a text field has the focus unless `when_typing=True`. Cmd+C,
  Cmd+X and Cmd+V are turned into egui copy/cut/paste events and may not reach `key_pressed`.
- `scroll_area_vertical` and `table*` clip their content to the width they are given; use
  `scroll_area_both` or the automatic horizontal scroll of `table(column_widths=[...])` to reach
  wider content.
- A `TreeView` and a `Dock` are state objects: create them once, outside `update_func`. Their
  nodes/panes may be rebuilt every frame.
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
| Text | [text.md](docs/framework/text.md) | headings, labels, code, links, badges, markdown with mermaid |
| Buttons and inputs | [inputs.md](docs/framework/inputs.md) | buttons, checkbox, radio, switch, sliders, text edits, color and date pickers, progress |
| Layout | [layout.md](docs/framework/layout.md) | horizontal/vertical, grid, columns, group, frame, sizes, scroll areas, enable/disable |
| Panels and popups | [panels.md](docs/framework/panels.md) | side/top/bottom panels, window, modal, menus, tooltips, context menu |
| Data | [data.md](docs/framework/data.md) | `table`, `table_ui`, `tree_view`, plots |
| Docking | [docking.md](docs/framework/docking.md) | `Dock`, `DockPane`, `DockContainer`, `DockTabBar`, `dock_area` |
| Images and icons | [images.md](docs/framework/images.md) | `image`, image buttons, tinted SVG icons, native `file_icon` |
| Keyboard, mouse, drag and drop | [interaction.md](docs/framework/interaction.md) | `key_pressed`, `typed_text`, `modifiers`, `pointer_pos`, `interactive` (a clickable/draggable area with its `rect`), `draw_rect`, `dnd_drag_source`, `dnd_drop_zone` |
| Styling | [styling.md](docs/framework/styling.md) | light/dark/system theme, accent, `set_style`, fonts, glass and native backdrops |

Index of all files: [docs/framework/README.md](docs/framework/README.md).

## Examples (`examples/`)

| File | Shows |
|---|---|
| `hello_world.py` | the smallest app |
| `widget_gallery.py`, `demo.py` | most widgets, plots |
| `finder.py` | a mini Finder: sidebar, toolbar, list/icon/tree views, preview, and a right-hand **Dock** with an **Actions** pane (collapsible categories, per-selection enabled state, shortcuts, rename modal, copy/cut/paste), multi-selection in the 3 views (Cmd/Shift click, Cmd+A) and a rubber band in the icon view, path bar; watched folders (`finder_watch.py`, with the `watchdog` library: installed by `uv run` through the `examples` dependency group of `pyproject.toml`, or `pip install watchdog`; panes **Dossiers surveillés** and **Activités**, a console of what changes), a SQLite index of folders (`finder_tags.py`; the PDF found are read with pypdf by `finder_pdf.py`: pages, protections, metadata, shown in Preview) and an advanced search pane; panes can be closed, moved by their tab, and brought back from the gear menu; the window, theme, view, folder, dock layout and open tree folders are kept between runs (`persist_id`, `ctx.save_state`, `dock.save`) |
| `app.py` | "Atelier": sidebar, toolbar, tabs and status bar in a glass style |
| `explorer.py` | file explorer in a glass style with a native backdrop, files open in tabs |
| `fileviwer.py` | file browsing with `scandir` |
| `workspace.py`, `python-ide.py`, `pages.py` | docking, panes and pages |
| `glass.py` | glass style and native backdrops |
| `markdown.py` | markdown editor with live preview and mermaid diagrams |

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
| `dock.rs` | docking (`egui_dock`) |
| `markdown.rs` | markdown and mermaid |
| `style.rs` | themes, accent, glass, `set_style` |
| `icons.rs` | `file_icon` (native file icons) |

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
