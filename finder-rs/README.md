# finder-rs

The Finder of `examples/finder.py` in pure Rust: one executable, nothing to install (no Python).

```bash
cargo run --release -p finder-rs -- [folder]          # run
cargo build --profile dist -p finder-rs               # the executable of the release: target/dist/finder
python scripts/bundle_finder_macos.py                 # macOS: dist/Finder.app and dist/Finder.dmg
```

It is built on `pyegui-core` (the logic shared with the Python extension: file watcher, transfers, PDF
viewer, terminal, markdown, styles, icons) and on egui. Its tags are kept in the same SQLite database as the
Python Finder (`tags.db` of the user data folder, `PYEGUI_FINDER_TAGS` overrides the path).

Shortcuts: Cmd+[ and Cmd+] back and forward, Cmd+Up parent, Enter or Cmd+Down open, Backspace parent, Cmd+A,
Cmd+C / X / V, Cmd+D duplicate, Cmd+Backspace trash, F2 rename, Cmd+Shift+N new folder, Cmd+1 / 2 / 3 list, icons,
tree, Cmd+Shift+. hidden files, Cmd+B sidebar, Cmd+J panes, Cmd+T terminal (Ctrl on Windows and Linux).
