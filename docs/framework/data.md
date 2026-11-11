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

**Example:**

```python
selected = Int(-1)
rows = [["Alice", "32"], ["Bob", "27"]]

def update_func(ctx):
    table(["Name", "Age"], rows, selected)
    if selected.value >= 0:
        label(f"Selected: {rows[selected.value][0]}")
```


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


## `tree_view(tree, nodes, scroll_to=None)`

Shows a tree of nodes. Directories can be opened and nodes selected, with keyboard navigation.

**Args**

- `tree` (TreeView): the state of the tree
- `nodes` (list[TreeNode]): the nodes
- `scroll_to` (int): id of a node to scroll to, just enough to show it. Give it only during the frame the tree has to scroll. The tree has to be the first thing of a scroll area

**Example:**

```python
tree = TreeView("files")
nodes = [
    TreeNode(0, "src", [TreeNode(1, "main.py")]),
    TreeNode(2, "README.md"),
]

def update_func(ctx):
    tree_view(tree, nodes)
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
- `TreeView.is_open(id)` (method): True if the directory is open. Useful to load the children of a directory only when it is

  opened: give it an empty list of children until then

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

**Usage:**

```python
nodes = [
    TreeNode(0, "src", [TreeNode(1, "main.py"), TreeNode(2, "utils.py")]),
    TreeNode(3, "README.md", icon=file_icon("README.md", 16)),
]
```


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
