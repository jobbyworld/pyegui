# File views and watcher

Ready made views of the disk: `FileListView` (a list with sortable columns), `FileIconView` (a grid of icons) and `FileTreeView` (a tree of folders), drawn by `file_list_view`, `file_icon_view` and `file_tree_view`. They are state objects: create them once, outside the update function. Their folders are listed through a `FileWatcher` that the system notifies of every change, so a file created, renamed, modified or removed by any program shows at the next frame. Give the same watcher to all the views of an app. `path_breadcrumb` and `path_combo` show where a folder is and go to one of its ancestors.

## `FileListView`


The state of a list of the files of a folder, with the columns Name, Date Modified, Size and
Kind. Create it once, outside the update function, and draw it with file_list_view.

The folder is watched while it is shown (see FileWatcher): what any program changes in it
shows at the next frame. A click selects, Cmd (Ctrl) and Shift extend the selection, a drag on
the background draws a rubber band, a double click or Enter opens. The arrows, Home, End, Page
Up and Page Down move, Cmd+A selects all, Cmd+Up enters the parent folder, and typing the start
of a name selects it. A click on a column header sorts by it.

**Args**

- `id` (str): unique id of the view
- `folder` (str): the folder shown, ~ is the home folder. Default is the current folder
- `watcher` (FileWatcher): the watcher of the folders. Default is one shared by the views created without one
- `show_hidden` (bool): shows the hidden items. Default is False
- `multi_select` (bool): several items can be selected. Default is True
- `navigate` (bool): a double click or Enter on a folder enters it. Default is True. With False the folder is in activated, like a file
- `dirs_first` (bool): folders before files. Default is True
- `sort_by` (str): "name", "modified", "size" or "kind". Default is "name"

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
- `FileListView.filter` (attribute): Only the items whose name contains this text are shown (without case). Empty shows all
- `FileListView.focus(focused=True)` (method): Gives the keyboard focus to the view (True, the default) or takes it away (False) at

  the next frame. A click on the view gives it the focus too

- `FileListView.folder` (attribute): The folder shown. Setting it enters another folder, like go
- `FileListView.folder_changed` (attribute): True during the frame the view entered another folder by itself: double click, Enter

  or Cmd+Up on a folder

- `FileListView.forward()` (method): Goes forward in the history, after back
- `FileListView.go(folder)` (method): Enters a folder, kept in the history of back and forward
- `FileListView.items` (attribute): Paths of the items shown at the last frame, in their order
- `FileListView.reveal(path)` (method): Selects one item and scrolls to it
- `FileListView.selected` (attribute): Paths of the selected items, in the order they were selected
- `FileListView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system
- `FileListView.sort_by` (attribute): The order of the items: "name", "modified", "size" or "kind". A click on a column of

  the list view changes it

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
- `sort_by` (str): "name", "modified", "size" or "kind". Default is "name"

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
- `FileIconView.filter` (attribute): Only the items whose name contains this text are shown (without case). Empty shows all
- `FileIconView.focus(focused=True)` (method): Gives the keyboard focus to the view (True, the default) or takes it away (False) at

  the next frame. A click on the view gives it the focus too

- `FileIconView.folder` (attribute): The folder shown. Setting it enters another folder, like go
- `FileIconView.folder_changed` (attribute): True during the frame the view entered another folder by itself: double click, Enter

  or Cmd+Up on a folder

- `FileIconView.forward()` (method): Goes forward in the history, after back
- `FileIconView.go(folder)` (method): Enters a folder, kept in the history of back and forward
- `FileIconView.icon_size` (attribute): The size of the icons, between 16 and 256: the tiles of the grid grow with it, so there

  are fewer of them on a row. Default is 48. Give it a slider to let the user choose, like the
  Finder. The icon_size keyword arg of file_icon_view wins over it

- `FileIconView.items` (attribute): Paths of the items shown at the last frame, in their order
- `FileIconView.reveal(path)` (method): Selects one item and scrolls to it
- `FileIconView.selected` (attribute): Paths of the selected items, in the order they were selected
- `FileIconView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system
- `FileIconView.sort_by` (attribute): The order of the items: "name", "modified", "size" or "kind". A click on a column of

  the list view changes it

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

**Usage:**

```python
tree = FileTreeView("tree", "~/projects/app")

def update_func(ctx):
    file_tree_view(tree)
    for path in tree.activated:
        print("open", path)
```


- `FileTreeView.activated` (attribute): Paths of the files activated during the last frame by a double click or Enter
- `FileTreeView.filter` (attribute): Only the files whose name contains this text are shown (without case), the folders always
- `FileTreeView.focus(focused=True)` (method): Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the

  next frame

- `FileTreeView.is_open(path)` (method): True if the folder is open in the tree
- `FileTreeView.open_folders` (attribute): The paths of the open folders, to save and give back at the next run
- `FileTreeView.reveal(path)` (method): Opens the folders down to a path, selects it and scrolls to it
- `FileTreeView.root` (attribute): The folder at the top of the tree
- `FileTreeView.selected` (attribute): Paths of the selected items
- `FileTreeView.set_open(path, open)` (method): Opens or closes a folder of the tree
- `FileTreeView.show_hidden` (attribute): Shows the hidden items: a name starting with a dot, or hidden by the system

## `file_list_view(view, **kwargs)`

Draws a FileListView: the files of its folder as a list with columns. It takes the space left
in the ui and scrolls; only the visible rows are drawn, so large folders stay fast.

**Args**

- `view` (FileListView): the state of the view

**Keyword args**

- `columns` (list[str]): the columns after the name, among "modified", "size" and "kind". Default is all three. The last ones are hidden when the view is narrow
- `headers` (list[str]): the titles of the columns name, modified, size and kind. Default is ["Name", "Date Modified", "Size", "Kind"]
- `date_format` (str): format of the dates, like strftime. Default is "%d %b %Y %H:%M"
- `row_height` (float): height of a row. Default is 24
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


## `file_icon_view(view, **kwargs)`

Draws a FileIconView: the files of its folder as a grid of icons. It takes the space left in
the ui and scrolls; only the visible tiles are drawn.

**Args**

- `view` (FileIconView): the state of the view

**Keyword args**

- `icon_size` (float): size of the icons, instead of view.icon_size. Default is view.icon_size
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
- `max_width` (float): the widest it can be. Default is the width left in the ui
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
