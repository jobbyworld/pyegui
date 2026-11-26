# File views and watcher

Ready made views of the disk: `FileListView` (a list whose columns can be resized, moved, sorted and filtered by type), `FileIconView` (a grid of icons) and `FileTreeView` (a tree of folders), drawn by `file_list_view`, `file_icon_view` and `file_tree_view`. They are state objects: create them once, outside the update function. Their folders are listed through a `FileWatcher` that the system notifies of every change, so a file created, renamed, modified or removed by any program shows at the next frame. Give the same watcher to all the views of an app. `path_breadcrumb` and `path_combo` show where a folder is and go to one of its ancestors.

## `FileListView`


The state of a list of the files of a folder, with the columns Name, Date Modified, Size and
Kind. Create it once, outside the update function, and draw it with file_list_view.

The folder is watched while it is shown (see FileWatcher): what any program changes in it
shows at the next frame. A click selects, Cmd (Ctrl) and Shift extend the selection, a drag on
the background draws a rubber band, a double click or Enter opens. The arrows, Home, End, Page
Up and Page Down move, Cmd+A selects all, Cmd+Up enters the parent folder, and typing the start
of a name selects it.

The header of the columns is interactive: a click on a title sorts by the column (a second click
reverses the order), a drag on a title moves the column, a drag on the border of a title
resizes it (a double click gives it its default width back), and the funnel at the right of a
title filters it, with a filter that fits the type of the column: a text for the name, a minimum
and a maximum for the size, a period for the date, the values to keep for the kind. Everything
is also a property of the view (sort_by, ascending, column_widths, column_order,
column_filters), to save at the end of the run and give back at the next one.

**Args**

- `id` (str): unique id of the view
- `folder` (str): the folder shown, ~ is the home folder. Default is the current folder
- `watcher` (FileWatcher): the watcher of the folders. Default is one shared by the views created without one
- `show_hidden` (bool): shows the hidden items. Default is False
- `multi_select` (bool): several items can be selected. Default is True
- `navigate` (bool): a double click or Enter on a folder enters it. Default is True. With False the folder is in activated, like a file
- `dirs_first` (bool): folders before files. Default is True
- `sort_by` (str): "name", "modified", "size", "kind" or the title of a column of your own that has a value function. Default is "name"

**Usage:**

```python
files = FileListView("files", "~/Documents")

def update_func(ctx):
    if button_clicked("⬆"):
        files.up()
    label(files.folder)
    file_list_view(files)
    for path in files.activated:
        ctx.open_url(Path(path).as_uri())
```


- `FileListView.activated` (attribute): Paths activated during the last frame: double click or Enter on files (on folders too

  when navigate is False)

- `FileListView.ascending` (attribute): False reverses the order
- `FileListView.back()` (method): Goes back to the previous folder of the history
- `FileListView.can_go_back` (attribute): True if back has a folder to go to
- `FileListView.can_go_forward` (attribute): True if forward has a folder to go to
- `FileListView.clear_filters()` (method): Removes the filters of all the columns, the name included
- `FileListView.column_filters` (attribute): The filters of the columns, by column key: a text for a column of text (the items whose value

  contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
  size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
  the header sets them too. Assign a dict to replace them all

- `FileListView.column_order` (attribute): The keys of the columns in the order the user gave them by dragging their title. Empty while

  the order is the default one. Save and give it back at the next run

- `FileListView.column_widths` (attribute): The widths the user gave to the columns by dragging their border, by column key ("name",

  "modified", "size", "kind" or the title of a column of your own). Save and give them back at
  the next run. A column without width has its default one

- `FileListView.drop_target` (attribute): The folder of the view under the pointer while files are dragged over it from the Finder

  (``hovered_files``) or on the frame they are dropped on it (``dropped_files``), else
  None. The folder is drawn framed while files hover it

- `FileListView.filter` (attribute): Only the items whose name contains this text are shown (without case). Empty shows all.

  It is the filter of the column "name"

- `FileListView.focus(focused=True)` (method): Gives the keyboard focus to the view (True, the default) or takes it away (False) at

  the next frame. A click on the view gives it the focus too

- `FileListView.folder` (attribute): The folder shown. Setting it enters another folder, like go
- `FileListView.folder_changed` (attribute): True during the frame the view entered another folder by itself: double click, Enter

  or Cmd+Up on a folder

- `FileListView.forward()` (method): Goes forward in the history, after back
- `FileListView.go(folder)` (method): Enters a folder, kept in the history of back and forward
- `FileListView.icon_clicked()` (method): The key of the column whose icon was clicked since the last call (the title of a column of your own,

  see the "icon" of extra_columns), or None. The call forgets it

- `FileListView.items` (attribute): Paths of the items shown at the last frame, in their order
- `FileListView.keep_focus` (attribute): If True, the view takes back the keyboard focus at every frame when another widget

  holds it, except a text field being edited. For an app where the view is the part of
  the window the keyboard acts on

- `FileListView.paths` (attribute): The items shown instead of the items of the folder, e.g. the result of a search or

  the files of a tag, from any folders. They stay up to date like a folder. None (the
  default) shows the folder; entering a folder sets it back to None

- `FileListView.pinned_left` (attribute): The columns pinned at the left (by column key): they stay in place while the others scroll

  horizontally. The user pins a column with a right click on its title

- `FileListView.pinned_right` (attribute): The columns pinned at the right (by column key)
- `FileListView.refresh_columns()` (method): Asks the functions `value` of the columns of your own again at the next frame, e.g. after the

  tags changed. They are otherwise kept for a second

- `FileListView.reveal(path)` (method): Selects one item and scrolls to it
- `FileListView.scroll_to(path)` (method): Scrolls just enough to show an item, at the next frame
- `FileListView.selected` (attribute): Paths of the selected items, in the order they were selected
- `FileListView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system
- `FileListView.sort_by` (attribute): The order of the items: "name", "modified", "size", "kind", or the title of a column of your

  own that has a value function. A click on a title of the header changes it

- `FileListView.up()` (method): Enters the parent folder and selects the folder we come from. Cmd+Up does the same

## `FileIconView`


The state of a grid of the files of a folder, with large icons and the name under them. Create
it once, outside the update function, and draw it with file_icon_view.

It works like FileListView: the same arguments, selection, rubber band, keyboard (the four
arrows move in the grid) and the folder is watched while it is shown.

**Args**

- `id` (str): unique id of the view
- `folder` (str): the folder shown, ~ is the home folder. Default is the current folder
- `watcher` (FileWatcher): the watcher of the folders. Default is one shared by the views created without one
- `show_hidden` (bool): shows the hidden items. Default is False
- `multi_select` (bool): several items can be selected. Default is True
- `navigate` (bool): a double click or Enter on a folder enters it. Default is True
- `dirs_first` (bool): folders before files. Default is True
- `sort_by` (str): "name", "modified", "size", "kind" or the title of a column of your own that has a value function. Default is "name"

The size of the grid is icon_size, which can change at any time, e.g. from a slider.

**Usage:**

```python
icons = FileIconView("icons", "~/Pictures")
size = Float(64)

def update_func(ctx):
    slider_float(size, 16, 256, "")
    icons.icon_size = size.value  # the grid follows the slider
    file_icon_view(icons)
    if icons.selected:
        label(f"{len(icons.selected)} selected")
```


- `FileIconView.activated` (attribute): Paths activated during the last frame: double click or Enter on files (on folders too

  when navigate is False)

- `FileIconView.ascending` (attribute): False reverses the order
- `FileIconView.back()` (method): Goes back to the previous folder of the history
- `FileIconView.can_go_back` (attribute): True if back has a folder to go to
- `FileIconView.can_go_forward` (attribute): True if forward has a folder to go to
- `FileIconView.clear_filters()` (method): Removes the filters of all the columns, the name included
- `FileIconView.column_filters` (attribute): The filters of the columns, by column key: a text for a column of text (the items whose value

  contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
  size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
  the header sets them too. Assign a dict to replace them all

- `FileIconView.column_order` (attribute): The keys of the columns in the order the user gave them by dragging their title. Empty while

  the order is the default one. Save and give it back at the next run

- `FileIconView.column_widths` (attribute): The widths the user gave to the columns by dragging their border, by column key ("name",

  "modified", "size", "kind" or the title of a column of your own). Save and give them back at
  the next run. A column without width has its default one

- `FileIconView.drop_target` (attribute): The folder of the view under the pointer while files are dragged over it from the Finder

  (``hovered_files``) or on the frame they are dropped on it (``dropped_files``), else
  None. The folder is drawn framed while files hover it

- `FileIconView.filter` (attribute): Only the items whose name contains this text are shown (without case). Empty shows all.

  It is the filter of the column "name"

- `FileIconView.focus(focused=True)` (method): Gives the keyboard focus to the view (True, the default) or takes it away (False) at

  the next frame. A click on the view gives it the focus too

- `FileIconView.folder` (attribute): The folder shown. Setting it enters another folder, like go
- `FileIconView.folder_changed` (attribute): True during the frame the view entered another folder by itself: double click, Enter

  or Cmd+Up on a folder

- `FileIconView.forward()` (method): Goes forward in the history, after back
- `FileIconView.go(folder)` (method): Enters a folder, kept in the history of back and forward
- `FileIconView.icon_clicked()` (method): The key of the column whose icon was clicked since the last call (the title of a column of your own,

  see the "icon" of extra_columns), or None. The call forgets it

- `FileIconView.icon_size` (attribute): The size of the icons, between 16 and 256: the tiles of the grid grow with it, so there

  are fewer of them on a row. Default is 48. Give it a slider to let the user choose, like the
  Finder. The icon_size keyword arg of file_icon_view wins over it

- `FileIconView.items` (attribute): Paths of the items shown at the last frame, in their order
- `FileIconView.keep_focus` (attribute): If True, the view takes back the keyboard focus at every frame when another widget

  holds it, except a text field being edited. For an app where the view is the part of
  the window the keyboard acts on

- `FileIconView.paths` (attribute): The items shown instead of the items of the folder, e.g. the result of a search or

  the files of a tag, from any folders. They stay up to date like a folder. None (the
  default) shows the folder; entering a folder sets it back to None

- `FileIconView.pinned_left` (attribute): The columns pinned at the left (by column key): they stay in place while the others scroll

  horizontally. The user pins a column with a right click on its title

- `FileIconView.pinned_right` (attribute): The columns pinned at the right (by column key)
- `FileIconView.refresh_columns()` (method): Asks the functions `value` of the columns of your own again at the next frame, e.g. after the

  tags changed. They are otherwise kept for a second

- `FileIconView.reveal(path)` (method): Selects one item and scrolls to it
- `FileIconView.scroll_to(path)` (method): Scrolls just enough to show an item, at the next frame
- `FileIconView.selected` (attribute): Paths of the selected items, in the order they were selected
- `FileIconView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system
- `FileIconView.sort_by` (attribute): The order of the items: "name", "modified", "size", "kind", or the title of a column of your

  own that has a value function. A click on a title of the header changes it

- `FileIconView.up()` (method): Enters the parent folder and selects the folder we come from. Cmd+Up does the same

## `FileTreeView`


The state of a tree of the folders and files under a root folder. Create it once, outside
the update function, and draw it with file_tree_view.

A folder is read when it is opened, and watched while it is open (see FileWatcher): what any
program changes in it shows at the next frame. The keyboard works like in TreeView: the
arrows move, Left and Right close and open a folder, Enter or a double click on a file
activates it.

**Args**

- `id` (str): unique id of the view
- `root` (str): the folder at the top, ~ is the home folder. Default is the current folder
- `watcher` (FileWatcher): the watcher of the folders. Default is one shared by the views created without one
- `show_hidden` (bool): shows the hidden items. Default is False
- `multi_select` (bool): several items can be selected with Cmd (Ctrl) and Shift. Default is True
- `show_root` (bool): the root folder is the first node and the rest is in it. Default is False: the items of the root are at the top
- `dirs_first` (bool): folders before files. Default is True
- `activatable_dirs` (bool): a double click or Enter on a folder activates it, like a file (see activated). Its arrow still opens and closes it. Default is False: a double click opens and closes the folder

**Usage:**

```python
tree = FileTreeView("tree", "~/projects/app")

def update_func(ctx):
    file_tree_view(tree)
    for path in tree.activated:
        print("open", path)
```


- `FileTreeView.activated` (attribute): Paths activated during the last frame by a double click or Enter: files, and folders when

  activatable_dirs is True

- `FileTreeView.ascending` (attribute): False reverses the order
- `FileTreeView.clear_filters()` (method): Removes the filters of all the columns, the name included
- `FileTreeView.column_filters` (attribute): The filters of the columns, by column key: a text for a column of text (the items whose value

  contains it, without case), a (min, max) tuple for a number or a date (None for no bound; a
  size is "10 KB", a date "2024-05-01"), a list of values for a column of choices. The funnel of
  the header sets them too. Assign a dict to replace them all

- `FileTreeView.column_order` (attribute): The keys of the columns in the order the user gave them by dragging their title. Empty while

  the order is the default one. Save and give it back at the next run

- `FileTreeView.column_widths` (attribute): The widths the user gave to the columns by dragging their border, by column key ("name",

  "modified", "size", "kind" or the title of a column of your own). Save and give them back at
  the next run. A column without width has its default one

- `FileTreeView.drop_target` (attribute): The folder of the tree under the pointer while files are dragged over it from the Finder or

  from the window itself, or on the frame they are dropped on it, else None. The folder is
  drawn framed while files hover it

- `FileTreeView.filter` (attribute): Only the files whose name contains this text are shown (without case), the folders always.

  It is the filter of the column "name"

- `FileTreeView.focus(focused=True)` (method): Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the

  next frame

- `FileTreeView.icon_clicked()` (method): The key of the column whose icon was clicked since the last call (the title of a column of your own, see

  the "icon" of extra_columns), or None. The call forgets it

- `FileTreeView.is_open(path)` (method): True if the folder is open in the tree
- `FileTreeView.items` (attribute): Paths of the rows shown at the last frame, in their order: the items of the open folders

  follow their folder, and sort and filters are applied

- `FileTreeView.keep_focus` (attribute): If True, the tree takes back the keyboard focus at every frame when another widget holds

  it, except a text field being edited

- `FileTreeView.open_folders` (attribute): The paths of the open folders, to save and give back at the next run
- `FileTreeView.paths` (attribute): The items at the top of the tree instead of the items of the root, e.g. the result of a

  search or the files of a tag, from any folders; their folders still open. None (the
  default) shows the root

- `FileTreeView.pinned_left` (attribute): The columns pinned at the left (by column key): they stay in place while the others scroll

  horizontally. The user pins a column with a right click on its title

- `FileTreeView.pinned_right` (attribute): The columns pinned at the right (by column key)
- `FileTreeView.refresh_columns()` (method): Asks the functions `value` of the columns of your own again at the next frame, e.g. after the

  tags changed. They are otherwise kept for a second

- `FileTreeView.reveal(path)` (method): Opens the folders down to a path, selects it and scrolls to it
- `FileTreeView.root` (attribute): The folder at the top of the tree
- `FileTreeView.scroll_to(path)` (method): Scrolls just enough to show an item, at the next frame. It has to be shown: its folders open
- `FileTreeView.selected` (attribute): Paths of the selected items
- `FileTreeView.set_open(path, open)` (method): Opens or closes a folder of the tree
- `FileTreeView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system
- `FileTreeView.sort_by` (attribute): The order of the items: "name", "modified", "size", "kind", or the title of a column of your

  own that has a value function. A click on a title of the header changes it


## `file_list_view(view, **kwargs)`

Draws a FileListView: the files of its folder as a list with columns. It takes the space left
in the ui and scrolls; only the visible rows are drawn, so large folders stay fast.

**Args**

- `view` (FileListView): the state of the view

**Keyword args**

- `columns` (list[str]): the columns after the name, among "modified", "size" and "kind". Default is all three. A view narrower than its columns scrolls horizontally
- `headers` (list[str]): the titles of the columns name, modified, size and kind. Default is ["Name", "Date Modified", "Size", "Kind"]
- `date_format` (str): format of the dates, like strftime, or "human" for the dates in French words ("il y a 5 min", "Hier à 09:10", "12 mars"). Default is "human"
- `row_height` (float): height of a row. Default is 24
- `extra_columns` (list[tuple]): columns of your own, after the name: a title, a width and a function called with the path of the row inside the cell, to draw it with the usual widgets. They sort and filter when a fourth item, a dict, gives the type of their data and a function that returns the value of a row: {"type": "text" | "number" | "date" | "choice", "value": function(path) -> value}. A date is seconds since 1970 or a datetime. The dict can also hold "icon": the path of an SVG file drawn at the left of the title, in the color of the title. The values are kept for a second (see refresh_columns). The function runs while the view is drawn: it must not read or change the view itself
- `resizable` (bool): the border of a column can be dragged. Default is True
- `reorderable` (bool): the title of a column can be dragged to another place. Default is True
- `drag_out` (bool): a drag of the selection starts a drag of the files to the Finder (the Explorer, another application), see start_file_drag. Default is True
- `filterable` (bool): the funnel of the header opens the filter of the column. Default is True
- `badges` (callable): called with the path of each row shown, returns a list of colors drawn as dots at the right end of the name, e.g. the tags of a file
- `context_menu` (callable): called with the list of paths concerned on a right click, inside a menu: the selection if the item clicked is in it, else the item clicked, and [] on the background. Build the menu in it

**Example:**

```python
files = FileListView("files", "~")

def menu(paths):
    if paths and button_clicked("Show in Finder"):
        ...
    if button_clicked("Show hidden files"):
        files.show_hidden = not files.show_hidden

def update_func(ctx):
    file_list_view(files, columns=["modified", "size"], context_menu=menu)
```

**A column of your own that sorts and filters:**

```python
def stars(path):
    rating = ratings.get(path, 0)
    label("★" * rating)

file_list_view(
    files,
    extra_columns=[("Rating", 80, stars, {"type": "number", "value": lambda p: ratings.get(p, 0)})],
)
files.column_filters = {"size": ("1 MB", None), "kind": ["PDF document"]}
```


## `file_icon_view(view, **kwargs)`

Draws a FileIconView: the files of its folder as a grid of icons. It takes the space left in
the ui and scrolls; only the visible tiles are drawn.

**Args**

- `view` (FileIconView): the state of the view

**Keyword args**

- `icon_size` (float): size of the icons, instead of view.icon_size. Default is view.icon_size
- `badges` (callable): called with the path of each tile shown, returns a list of colors drawn as dots on the top left corner of the icon, e.g. the tags of a file
- `context_menu` (callable): called with the list of paths concerned on a right click, inside a menu, like for file_list_view

**Example:**

```python
icons = FileIconView("icons", "~/Desktop")

def update_func(ctx):
    file_icon_view(icons)
```


## `file_tree_view(view, **kwargs)`

Draws a FileTreeView: the folders and files under its root, as a tree. The folders are read
when they are opened and watched while they are open.

**Args**

- `view` (FileTreeView): the state of the view

**Keyword args**

- `context_menu` (callable): called with the list of paths concerned when an item is right-clicked, inside a menu: the selection if the item clicked is in it, else the item clicked. Build the menu in it
- `badges` (callable): called with the path of each item, returns a list of colors drawn as dots at the right end of its row, e.g. the tags of a file
- `columns` (list[str]): columns of the view at the right end of the rows, among "modified", "size" and "kind". Default is none, the tree is then only names
- `headers` (list[str]): the titles of the columns name, modified, size and kind. Default is ["Name", "Date Modified", "Size", "Kind"]
- `date_format` (str): format of the dates, like strftime, or "human" for the dates in French words ("il y a 5 min", "Hier à 09:10", "12 mars"). Default is "human"
- `extra_columns` (list[tuple]): columns of your own at the right end of the rows: a title (shown above the tree), a width, and a function called with the path of each row to draw its cell, e.g. ("Tags", 160, lambda path: tags_edit(...)). A fourth item, a dict, makes the column sort and filter: {"type": "text" | "number" | "date" | "choice", "value": function(path) -> value, "icon": path of an SVG file}. A date is seconds since 1970 or a datetime. The icon is drawn at the left of the title, in its color
- `header` (bool): draws the header of the columns above the tree, pinned at the top while the tree scrolls. Default is True when there are columns
- `resizable` (bool): the border of a column can be dragged. Default is True
- `reorderable` (bool): the title of a column can be dragged to another place. Default is True
- `drag_out` (bool): a drag of the selection starts a drag of the files to the Finder (the Explorer, another application), see start_file_drag. Default is True
- `pinnable` (bool): a right click on a title offers to pin the column at the left (after the name) or at the right: the other columns then scroll horizontally. Default is True
- `filterable` (bool): the funnel of the header opens the filter of the column, which fits its type: a text, a minimum and a maximum, a period, or the values to keep. The folders stay, only the files are filtered. Default is True

A click on a title sorts the items of each folder by the column, a second click reverses the
order. See sort_by, ascending, column_widths, column_order and column_filters of the view to
read, save and set them.

**Example:**

```python
tree = FileTreeView("tree", "~/projects")

def update_func(ctx):
    left_panel("side", lambda: scroll_area_both(lambda: file_tree_view(tree)))
    label(f"selected: {tree.selected}")
```


## `FileWatcher`


Keeps the file views up to date with the disk: the folders they show are watched by the
system, and a file created, removed, renamed or modified by any program, inside or outside the
app, shows at the next frame. A new frame is asked for when something changes, so the window
follows even when the mouse does not move.

The views create one for themselves: give the same FileWatcher to all of them (and use it for
your own files) to share the listings and the watches. A folder is watched while a view shows
it, and a few seconds more; watch() keeps a path watched until unwatch().

**Usage:**

```python
watcher = FileWatcher()
files = FileListView("files", "~/Documents", watcher=watcher)
tree = FileTreeView("tree", "~/Documents", watcher=watcher)
notes = Path("~/notes.md").expanduser()
watcher.watch(notes)
text = notes.read_text()

def update_func(ctx):
    global text
    if watcher.changed(notes):  # saved by an editor: read it again
        text = notes.read_text()
    file_list_view(files)
    label(text)
```


- `FileWatcher.changed(path)` (method): True once after each change of the path (the file itself, or an item of the folder) seen

  by the system, False otherwise. The path has to be watched: by watch(), or shown by a view.
  The first call returns False and starts counting

- `FileWatcher.error` (attribute): The reason the system refused to watch, None when it works. The views still list the

  folders, but no longer follow the changes made outside the app

- `FileWatcher.list_dir(folder, hidden=False)` (method): The items of a folder, read again only when it changed, and the folder watched. Each one is

  a dict: path, name, is_dir, size (bytes), modified (timestamp or None), hidden. For a view
  of your own that stays up to date like the file views

  **Args**

  - `folder` (str): the folder
  - `hidden` (bool): with the hidden items (a name starting with a dot, or hidden by the system). Default is False

- `FileWatcher.take_events()` (method): The events the system sent since the last call, oldest first, for debugging: each one is a

  dict time (timestamp), kind (the kind of notify, e.g. "Create(File)", "Modify(Name(From))",
  "Remove(Any)") and paths (list[str]). All events are there, also the ones the views ignore
  (Access). Only the last 1000 are kept between two calls

- `FileWatcher.unwatch(path)` (method): Stops watching a path given to watch
- `FileWatcher.version` (attribute): A number that increases at each change of a path the watcher watches: compare it with the

  one of the last frame to know that something changed on the disk

- `FileWatcher.watch(path)` (method): Watches a file or a folder until unwatch, even when no view shows it. See changed
- `FileWatcher.watched` (attribute): The paths watched now, the ones the views show included

## `path_breadcrumb(path, **kwargs)`

Shows a path as the list of its folders from the root, like the path bar of the Finder: the
icon and the name of each one, separated by ›. When the room is missing the ancestors are
shortened, the longest names first, and the mouse over a shortened one shows it in full. A
click on a folder returns its path.

**Args**

- `path` (str): the path shown, ~ is the home folder

**Keyword args**

- `root` (str): the first folder shown, an ancestor of path. Default is the root of the disk
- `max_width` (float): the widest it can be. Default is the width left in the ui. The ancestors are shortened first, then those that still do not fit are left out from the root, so the breadcrumb never takes more. Give it the width left minus what follows it in the row, as drawn (measured, not guessed, and with the spacing of the row): a row that measures itself and gets an under-estimate pushes its right side out, sees that it does not fit, widens, fits again, and redraws at every frame
- `icons` (bool): shows the icons of the folders. Default is True
- `id` (str): id of the breadcrumb, needed if there are several in the same ui
- `on_click` (callable): called with the path of the element clicked
- `on_double_click` (callable): called with the path of the element double clicked (after on_click for its first click)
- `on_right_click` (callable): called with the path of the element right-clicked
- `on_middle_click` (callable): called with the path of the element clicked with the middle button, e.g. to open it in a new tab
- `context_menu` (callable): called with the path of the element right-clicked, inside a menu: build the menu in it, like for file_list_view

Returns the path clicked during this frame, None otherwise: the return value or on_click,
whichever reads better.

**Example:**

```python
folder = Str(str(Path.home() / "Documents"))

def update_func(ctx):
    clicked = path_breadcrumb(folder.value)
    if clicked:
        folder.value = clicked

    # or with functions, and a menu on right click
    def menu(path):
        if button_clicked("Copy path"):
            ctx.copy_text(path)
            close_menu()

    path_breadcrumb(folder.value, on_double_click=open_folder, context_menu=menu)
```


## `path_combo(path, **kwargs)`

Shows the name of a folder as a title that is also a menu of its ancestors, like the title of
a Finder window: under the mouse the icon of the folder and the arrow of a menu show, and a
click opens the list of the folder and its ancestors up to the root, the folder checked.
Choosing one returns its path.

**Args**

- `path` (str): the folder shown, ~ is the home folder

**Keyword args**

- `size` (float): size of the text. Default is 17
- `root` (str): the last ancestor of the list. Default is the root of the disk
- `id` (str): id of the combo, needed if there are several in the same ui
- `max_width` (float): the widest the title may be: a longer name ends with an ellipsis (the whole name is in the tooltip). Default is the room left in the row
- `always_icon` (bool): shows the icon of the folder and the arrow of the menu all the time, not only under the mouse. Default is False
- `on_right_click` (callable): called with the path of the folder when the title is right-clicked
- `on_middle_click` (callable): called with the path of the folder when the title is clicked with the middle button
- `context_menu` (callable): called with the path of the folder on a right click on the title, inside a menu: build the menu in it. A left click opens the list of the ancestors

Returns the path chosen in the list during this frame, None otherwise.

**Example:**

```python
folder = Str(str(Path.home() / "Documents"))

def update_func(ctx):
    chosen = path_combo(folder.value)
    if chosen:
        folder.value = chosen
```


## `FileTransfer`


Copies or moves files into a folder in a thread, and tells how far it is, so that the window
does not freeze on a big file and can draw a progress bar. A name that exists is not replaced:
the new one is "name copy", "name copy 2"... A move on the same volume is a rename, between two
volumes it is a copy followed by the removal of the original. Create it once, outside the update
function, and read it at each frame.

**Usage:**

```python
transfer = FileTransfer()

def update_func(ctx):
    sources, folder = dropped_files(), files.drop_target or current
    if sources:
        transfer.start(sources, folder, operation=drop_operation(sources, folder))
    if transfer.running:
        progress(transfer.progress)
    for folder, target in transfer.finished():
        refresh(folder)
    for error in transfer.errors():
        print(error)
```


- `FileTransfer.done` (attribute): The bytes done
- `FileTransfer.errors()` (method): The failures since the last call, as "source: reason" texts. Each is returned once
- `FileTransfer.finished()` (method): The items done since the last call, as (folder, new path) pairs, oldest first. Each is

  returned once: it is the moment to refresh the folder and to select the new items

- `FileTransfer.progress` (attribute): How far it is, from 0 to 1, for ``progress``
- `FileTransfer.running` (attribute): The number of items not done yet. 0 when nothing is going on
- `FileTransfer.start(sources, folder, operation='copy')` (method): Starts to copy or move the files into the folder and returns at once. Files started while

  others are still going add to the same progress

  **Args**

  - `sources` (list[str]): the files and folders
  - `folder` (str): the folder to put them in
  - `operation` (str): "copy" (the default) or "move", see ``drop_operation``

- `FileTransfer.total` (attribute): The bytes to copy or move since the transfer started (it grows as the items are measured)

## `drop_operation(sources, folder)`

What a drop of files on a folder does by the rules of the file managers, with the keys held now:
on the same volume the files are moved, and Option (macOS) or Ctrl (Windows, Linux) copies them
instead; on two volumes they are copied, and Cmd (macOS) or Shift (Windows, Linux) moves them
instead. The keys are read from the system, so it is right while files are dragged over the
window. Call it when the files hover (to say it) and again on the frame they are dropped.

**Args**

- `sources` (list[str]): the files dropped, see ``dropped_files`` and ``hovered_files``
- `folder` (str): the folder they are dropped on

**Returns**

- `str`: "move" or "copy", the ``operation`` to give to ``FileTransfer.start``

**Example:**

```python
for_folder = view.drop_target or current_folder
if hovered_files():
    label(f"Drop to {drop_operation(hovered_files(), for_folder)} here")
```


## `same_volume(a, b)`

True if two paths are on the same disk (the same volume): a drop between them moves, else it
copies. A path that does not exist yet counts for its first parent that does.

**Args**

- `a` (str): a path
- `b` (str): another path

**Returns**

- `bool`: True if both are on the same volume

**Example:**

```python
same_volume("/Users/me/a.txt", "/Volumes/Backup/")  # False
```
