"""The columns of a file list and of a file tree: resize, reorder, sort and filter.

    python examples/file_columns.py [folder]

- drag the border of a title to resize a column (a double click gives it back its width)
- drag a title to move the column
- click a title to sort by it, again to reverse the order
- right click a title to pin the column at the left or at the right: the others scroll
  horizontally
- click the funnel at the right of a title (it shows when the mouse is over it) to filter: a text
  for a name or a column of text, a minimum and a maximum for a size or a number, a period for a
  date, the values to keep for a kind

The "Rating" and "Words" columns are drawn by this script: with a `value` function they sort and
filter like the others, as a number here, and "Words" as a text.
"""

import sys
from pathlib import Path

from pyegui import *

FOLDER = Path(sys.argv[1]).expanduser() if len(sys.argv) > 1 else Path.home()

watcher = FileWatcher()
files = FileListView("list", FOLDER, watcher=watcher)
tree = FileTreeView("tree", FOLDER, watcher=watcher)
mode = Int(0)  # 0 list, 1 tree
ratings = {}  # path -> number of stars, to show a column of your own


def rating(path):
    # a stable fake rating from the name: the demo needs numbers to sort and filter
    return ratings.setdefault(path, sum(map(ord, Path(path).name)) % 6)


def rating_cell(path):
    label("★" * rating(path) or "·")


def words_value(path):
    return " ".join(Path(path).stem.replace("_", " ").replace("-", " ").split())


EXTRAS = [
    ("Rating", 80, rating_cell, {"type": "number", "value": rating}),
    ("Words", 140, lambda path: weak(words_value(path)), {"type": "text", "value": words_value}),
]


def toolbar():
    view = files if mode.value == 0 else tree
    radio_value(mode, 0, "List")
    radio_value(mode, 1, "Tree")
    add_space(16)
    filters = ", ".join(f"{key}: {value}" for key, value in view.column_filters.items())
    small(f"sorted by {view.sort_by}" + (f" | {filters}" if filters else ""))
    if filters and button_clicked("Clear filters"):
        view.clear_filters()


def update_func(ctx):
    horizontal(toolbar)
    separator()
    if mode.value == 0:
        file_list_view(files, extra_columns=EXTRAS)
    else:
        file_tree_view(tree, columns=["modified", "size", "kind"], extra_columns=EXTRAS)


run_native("File columns", update_func)
