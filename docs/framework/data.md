# Tables, tree views and plots

Widgets for collections. Tables only draw the visible rows, so they stay fast with many rows. A `TreeView` keeps the selection and the opened folders, the nodes are rebuilt each frame.

## `table(headers, rows, selected=None, **kwargs)`

Shows a table of texts. Only visible rows are drawn, so it is fast even with many rows.

**Args**

- `headers` (list[str]): column names
- `rows` (list[list[str]]): cells of each row
- `selected` (Int): if given, rows can be selected by clicking on them. Value is -1 when nothing is selected

**Keyword args**

- `id` (str): id of the table, needed if there are several tables in the same ui
- `striped` (bool): every other row has a different background color. Default is True
- `resizable` (bool): columns can be resized. Default is True
- `max_height` (float): max height of the table before scrolling
- `scroll_to_row` (int): scrolls the table just enough to show this row. Give it only during the frame the table has to scroll, the user can scroll away afterwards
- `column_widths` (list[float | None | str]): width of each column: a starting width, None to fit
- the contents or "remainder" for the rest of the width. Default fits the contents, the last
- column takes the rest
- `view` (TableView): the state of the table. With it the header is interactive: a click on a title sorts (again to reverse), a drag on a title moves the column, a drag on its border resizes it, the funnel filters it and a right click pins it at the left or at the right (the columns that are not pinned scroll horizontally). `selected` then holds the index of the row in `rows`, whatever the sort and the filters. Widths are numbers or None
- `types` (list[str]): with `view`, the type of the data of each column: "text" (default), "number" (sorted by value, filtered by a minimum and a maximum, "10 KB" is understood), "date" (YYYY-MM-DD) or "choice" (filtered by the values kept)
- `sortable` (bool): with `view`, a click on a title sorts. Default is True
- `pinnable` (bool): with `view`, the right click on a title offers to pin the column. Default is True
- reorderable, filterable (bool): with `view`, the columns can be moved and filtered. Default is True

**Example:**

```python
selected = Int(-1)
rows = [["Alice", "32"], ["Bob", "27"]]

def update_func(ctx):
    table(["Name", "Age"], rows, selected)
    if selected.value >= 0:
        label(f"Selected: {rows[selected.value][0]}")
```


## `TableView`


The state of a table: sort, filters, widths, order and pinned columns. Create it once, outside
the update function, and give it to `table` with `view=`.

The columns are named by their header, so the headers of a table should be different.

**Args**

- `sort_by` (str): header of the column to sort by. Default is "", the order of the rows

**Properties**

- sort_by (str), ascending (bool), column_widths (dict[str, float]), column_order (list[str]),
- column_filters (dict), pinned_left (list[str]), pinned_right (list[str]) are read and
- written, to save at the end of the run and give back at the next one.
- visible_rows (list[int]) is read only: the indexes in `rows` of the rows shown, in order.

**Example:**

```python
view = TableView(sort_by="Age")
view.pinned_left = ["Name"]

def update_func(ctx):
    table(["Name", "Age", "City"], rows, view=view, types=["text", "number", "choice"])
```


- `TableView.ascending` (attribute)
- `TableView.clear_filters()` (method): Removes the filters of all the columns
- `TableView.column_filters` (attribute)
- `TableView.column_order` (attribute)
- `TableView.column_widths` (attribute)
- `TableView.pinned_left` (attribute)
- `TableView.pinned_right` (attribute)
- `TableView.reset_columns()` (method): Gives the columns their default width, order and no pin
- `TableView.sort_by` (attribute)
- `TableView.visible_rows` (attribute)

## `table_ui(headers, num_rows, cell_func, **kwargs)`

Shows a table whose cells are drawn by a function. Only visible rows are drawn.

**Args**

- `headers` (list[str]): column names
- `num_rows` (int): number of rows
- `cell_func` (Callable[[int, int], None]): draws the cell at the given row and column

Accepts the same keyword args as table.

**Example:**

```python
files = ["a.txt", "b.txt"]

def cell(row, col):
    if col == 0:
        label(files[row])
    elif button_clicked("Delete"):
        files.pop(row)

def update_func(ctx):
    table_ui(["File", ""], len(files), cell)
```


## `DataGrid`


A grid of data: rows of cells in columns, flat or as a tree, that is not tied to files. The columns sort, filter,
move, resize, hide and pin like the ones of the file views, the rows are selected with the mouse and the keyboard,
a cell can be edited in place, a branch opens and closes. It keeps no data: give the rows at each frame, and apply
what it tells (`take_edited`, `take_toggled`...). A state object: create it once, outside the update function, and
draw it with `data_grid`.

**Args**

- `columns` (list[dict]): the columns, each a dict with `key` (what the state and the events name it by), `title` (default the key), `type` ("text", "number", "date" or "choice", default "text"), `width` (default 120) and `editable` (a double click on a cell, or F2, edits it)
- `id` (str): tells apart two grids of a ui. Default is "grid"

**Example:**

```python
grid = DataGrid([dict(key="name", title="Name", editable=True), dict(key="age", title="Age", type="number")])
people = {"1": ["Alice", "32"], "2": ["Bob", "27"]}

def update_func(ctx):
    rows = [dict(id=i, cells=cells) for i, cells in people.items()]
    data_grid(grid, rows)
    edit = grid.take_edited()  # (row id, column key, new text)
    if edit:
        row, column, text = edit
        people[row][0 if column == "name" else 1] = text
```


- `DataGrid.ascending` (attribute)
- `DataGrid.collapse(id)` (method): Closes a branch of the tree
- `DataGrid.column_order` (attribute): The keys of the columns in the order the user left them (read and write, to keep between runs)
- `DataGrid.column_widths` (attribute): The widths the user gave to the columns, by key (read and write)
- `DataGrid.cursor` (attribute): The id of the row the keyboard is on, or None
- `DataGrid.expand(id)` (method): Opens a branch of the tree
- `DataGrid.expanded` (attribute): The ids of the branches of the tree that are open (read and write)
- `DataGrid.hidden_columns` (attribute): The keys of the columns the user hid with the eye of the settings (read and write)
- `DataGrid.hovered` (attribute): The id of the row the pointer was over during the last frame, or None
- `DataGrid.scroll_to(id)` (method): Scrolls to a row at the next frame (it must be shown)
- `DataGrid.selected` (attribute): The ids of the rows selected, in the order of the clicks (read and write)
- `DataGrid.shown` (attribute): The ids of the rows shown during the last frame, in order: the rows the filters keep, the branches open
- `DataGrid.sort_by` (attribute): The key of the column the grid is sorted by, and the direction (read and write)
- `DataGrid.start_edit(row, column)` (method): Starts editing a cell, as a double click does. Nothing happens if the column is not editable
- `DataGrid.take_activated()` (method): The ids of the rows opened (double click, Enter) since the last call
- `DataGrid.take_clicked()` (method): The id of the row clicked since the last call, or None
- `DataGrid.take_drag_started()` (method): The ids of the rows a drag started from (the selection) since the last call, or None. Needs `drag=True`
- `DataGrid.take_edited()` (method): (row id, column key, new text) of the cell edited since the last call, or None. Apply it to your data: the

  grid shows what it is given

- `DataGrid.take_toggled()` (method): (row id, opened) of the branch opened or closed since the last call, or None

## `data_grid(grid, rows, **kwargs)`

Draws a `DataGrid`: the rows as a list with columns, or as a tree. It takes the room left in the ui and scrolls;
only the visible rows are drawn.

**Args**

- `grid` (DataGrid): the state
- `rows` (list[dict]): the rows, each a dict with `id` (unique), `cells` (the texts, in the order of the columns, or a dict by column key), `parent` (the id of the parent row, for a tree) and `readonly`

**Keyword args**

- `tree` (bool): the rows are a tree (`parent`), with a chevron and an indentation in the first column. Default is False
- `multi_select` (bool): Cmd and Shift extend the selection. Default is True
- `striped` (bool): every other row has a different background. Default is True
- sortable, resizable, pinnable, reorderable, filterable (bool): what a title lets the user do. Default is True
- `settings` (bool): the icon of the settings at the right of the header, to order, hide and pin the columns
- `max_height` (float): the most the grid takes in height before it scrolls
- `row_height` (float): the height of a row. Default is the one of a button
- `indent` (float): the indentation of a level of the tree. Default is 16
- `drag` (bool): tell the start of a drag of the selection (`take_drag_started`). Default is False
- `context_menu` (callable): called with the list of ids concerned when a row is right-clicked, inside a menu

**Example:**

```python
data_grid(grid, rows, tree=True, settings=True, context_menu=lambda ids: button_clicked(f"{len(ids)} rows"))
```


## `tree_view(tree, nodes, scroll_to=None, context_menu=None)`

Shows a tree of nodes. Directories can be opened and nodes selected, with keyboard navigation.

**Args**

- `tree` (TreeView): the state of the tree
- `nodes` (list[TreeNode]): the nodes
- `scroll_to` (int): id of a node to scroll to, just enough to show it. Give it only during the frame the tree has to scroll. The tree has to be the first thing of a scroll area
- `context_menu` (callable): called with the list of ids concerned when a node is right-clicked, inside a menu: the selected nodes if the node clicked is one of them, else only the node clicked. Build the menu in it, or call close_menu() to act without a menu

**Example:**

```python
tree = TreeView("files")
nodes = [
    TreeNode(0, "src", [TreeNode(1, "main.py")]),
    TreeNode(2, "README.md"),
]

def update_func(ctx):
    tree_view(tree, nodes)
    # a menu on right click
    tree_view(tree, nodes, context_menu=lambda ids: button_clicked(f"{len(ids)} selected"))
```


## `TreeView`


TreeView stores the state of a tree view: selected nodes, opened directories and user actions.

**Args**

- `id` (str): unique id of the tree view
- `multi_select` (bool): allow selecting several nodes with ctrl and shift. Default is False
- `drag_and_drop` (bool): allow moving nodes by dragging them. Default is False
- `default_open` (bool): directories are open the first time they are shown. Default is True. Use False to load the children of a directory only when the user opens it, see is_open
- `activatable_dirs` (bool): a double click or Enter on a directory activates it, like a leaf, see activated. Its arrow still opens and closes it. Default is False: a double click opens and closes the directory

**Usage:**

```python
tree = TreeView("files")

def update_func(ctx):
    tree_view(tree, nodes)
    label(f"selected: {tree.selected}")
    if tree.activated:
        print("double clicked:", tree.activated)
```


- `TreeView.activated` (attribute): Ids of the nodes activated by a double click or enter during the last frame
- `TreeView.focus(focused=True)` (method): Gives the keyboard focus to the tree (True, the default) or takes it away (False) at the

  next tree_view. With the focus the arrows, Enter and the other keys work without a click,
  so an app that manages its own focus can hand it to the tree, or to another part of the
  window

- `TreeView.is_open(id)` (method): True if the directory is open. Useful to load the children of a directory only when it is

  opened: give it an empty list of children until then

- `TreeView.keep_focus` (attribute): If True, the tree takes back the keyboard focus at every tree_view when another widget

  holds it, except a text field being edited. For an app where the tree is the part of the
  window the keyboard acts on: a click on a button or a blank area does not cut the arrows

- `TreeView.moved` (attribute): Nodes dropped during the last frame, if drag_and_drop is enabled.


  It is None or a tuple (source_ids, target_id, position, position_id), where position is
  "first", "last", "before" or "after" and position_id is the id of the node the sources
  are placed before or after. You have to move the nodes yourself.

- `TreeView.selected` (attribute): Ids of the selected nodes
- `TreeView.set_open(id, open)` (method): Opens or closes a directory

## `TreeNode`


A node of a tree view. It is a directory if children is a list, even an empty one, and a leaf otherwise.

Ids must be unique in the tree.

**Args**

- `id` (int): unique id of the node
- `label` (str): the text of the node
- `children` (list[TreeNode]): the nodes of a directory, None for a leaf
- `icon` (str): uri of an image shown before the label, like for image(), e.g. from file_icon
- `badges` (list[tuple]): colors (r, g, b, a) of dots drawn at the right end of the row, e.g. the tags of a file

**Usage:**

```python
nodes = [
    TreeNode(0, "src", [TreeNode(1, "main.py"), TreeNode(2, "utils.py")]),
    TreeNode(3, "README.md", icon=file_icon("README.md", 16)),
]
```


- `TreeNode.badges` (attribute)
- `TreeNode.children` (attribute)
- `TreeNode.icon` (attribute)
- `TreeNode.id` (attribute)
- `TreeNode.label` (attribute)

## `plot(id, items, **kwargs)`

Shows a plot that can be zoomed and dragged.

Returns the plot coordinates (x, y) of the pointer when it hovers the plot, None otherwise.

**Args**

- `id` (str): unique id of the plot
- `items` (list[PlotLine | PlotPoints | PlotBars | PlotRule]): what to draw

**Keyword args**

- width (float), height (float): size of the plot. Default fills the available space
- `view_aspect` (float): width / height ratio of the plot
- `data_aspect` (float): ratio of a unit of x and a unit of y. Use 1 for circles to look round
- `legend` (bool): show a legend with the item names. Default is True
- x_label (str), y_label (str): axis labels
- allow_zoom (bool), allow_drag (bool), allow_scroll (bool): user interactions. Default is True

**Example:**

```python
sin = PlotLine([(x / 10, math.sin(x / 10)) for x in range(100)], name="sin")

def update_func(ctx):
    pos = plot("my plot", [sin], height=200, x_label="x")
    if pos:
        label(f"x: {pos[0]:.2f} y: {pos[1]:.2f}")
```


## `PlotLine`


A line of a plot.

**Args**

- `points` (list[tuple[float, float]] | list[float]): (x, y) points, or y values whose x is their index
- `name` (str): name shown in the legend
- `color` (tuple[int, int, int]): rgb color
- `width` (float): width of the line
- `fill` (float): fill the area between the line and this y value

**Usage:**

```python
PlotLine([(x / 10, math.sin(x / 10)) for x in range(100)], name="sin")
```


- `PlotLine.color` (attribute)
- `PlotLine.fill` (attribute)
- `PlotLine.name` (attribute)
- `PlotLine.points` (attribute): The (x, y) points of the line
- `PlotLine.width` (attribute)

## `PlotPoints`


A scatter plot.

**Args**

- `points` (list[tuple[float, float]] | list[float]): (x, y) points, or y values whose x is their index
- `name` (str): name shown in the legend
- `color` (tuple[int, int, int]): rgb color
- `radius` (float): radius of the markers
- `filled` (bool): fill the markers. Default is True

**Usage:**

```python
PlotPoints([(1, 2), (2, 3.5), (3, 1)], name="measures", radius=4)
```


- `PlotPoints.color` (attribute)
- `PlotPoints.filled` (attribute)
- `PlotPoints.name` (attribute)
- `PlotPoints.points` (attribute): The (x, y) points
- `PlotPoints.radius` (attribute)

## `PlotBars`


A bar chart.

**Args**

- `bars` (list[tuple[float, float]] | list[float]): (x, height) bars, or heights whose x is their index
- `name` (str): name shown in the legend
- `color` (tuple[int, int, int]): rgb color
- `width` (float): width of the bars
- `horizontal` (bool): draw horizontal bars. Default is False

**Usage:**

```python
PlotBars([3, 5, 2, 8], name="sales")
```


- `PlotBars.bars` (attribute): The (x, height) bars
- `PlotBars.color` (attribute)
- `PlotBars.horizontal` (attribute)
- `PlotBars.name` (attribute)
- `PlotBars.width` (attribute)

## `PlotRule`


A horizontal line at the given y, or a vertical line at the given x if vertical is True.

**Usage:**

```python
PlotRule(0.5, name="threshold", color=(255, 0, 0))
```


- `PlotRule.color` (attribute)
- `PlotRule.name` (attribute)
- `PlotRule.value` (attribute)
- `PlotRule.vertical` (attribute)
- `PlotRule.width` (attribute)
