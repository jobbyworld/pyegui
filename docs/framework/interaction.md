# Keyboard, mouse and drag and drop

Shortcuts are read with `key_pressed`; what the user did to a widget is read from its `Response` or from the value the function returns.

## `key_pressed(shortcut, when_typing=False)`

True if the key was pressed during the frame with these modifiers. Cmd and Ctrl must match,
Shift and Alt pressed in addition are ignored, so "Enter" is also Shift+Enter.

**Args**

- `shortcut` (str): a key with optional modifiers: "Enter", "ArrowDown", "Cmd+ArrowUp", "Shift+Tab". Cmd is the command key on macOS and ctrl elsewhere
- `when_typing` (bool): also when a text field has the focus. Default is False, so the arrows move the cursor of the text field instead of triggering the shortcut

**Example:**

```python
if key_pressed("Enter"):
    open_selection()
if key_pressed("Cmd+ArrowUp"):
    go_to_parent()
```


## `typed_text()`

The text typed during the frame, or an empty string. Nothing while a text field has the focus,
so it is for shortcuts that are letters: typing the start of a name to jump to it.

**Example:**

```python
typed = ""
def update_func(ctx):
    global typed
    typed += typed_text()
    select_first_starting_with(typed)
```


## `modifiers()`

The modifier keys held during the frame, as a dict with the keys shift, command and alt.
command is the command key on macOS and ctrl elsewhere.

**Example:**

```python
if response.clicked:
    if modifiers()["command"]:
        toggle(item)
    elif modifiers()["shift"]:
        select_range(item)
    else:
        select_only(item)
```


## `pointer_pos()`

Where the mouse is on the window in points, as (x, y), or None when it is outside. Compare it
with Response.rect.


## `pointer_pressed()`

True during the frame in which the primary mouse button went down, wherever the mouse is.
With ui_contains_pointer it tells which part of the window was clicked, even when the click
lands on no widget, to move the keyboard focus there.

**Example:**

```python
if pointer_pressed() and ui_contains_pointer():
    focused_zone.value = "sidebar"
```


## `ui_contains_pointer()`

True if the mouse is over the current ui, e.g. to show buttons on hover.

It tests the part of the ui drawn so far: at the start of a nested function the ui has no
size yet and this is False. To know whether a click fell in a whole area, call it after its
widgets.

**Example:**

```python
def row():
    label(name)
    if ui_contains_pointer():
        small_button_clicked("✕")
horizontal(row)
```


## `interactive(update_fun, drag=False)`

Runs update_fun in a child ui that reacts to the mouse as a whole: the widgets inside keep
their own clicks, the rest of the area is clickable. Returns a Response with the position of
the area, so a click on an icon and its name is one click on a tile, and the tiles can be
tested against a rectangle drawn with the mouse.

**Args**

- `update_fun` (Callable[[], None]): draws the contents
- `drag` (bool): also react to dragging, shown in dragged, drag_started and drag_stopped. Default is False

**Example:**

```python
def tile():
    image(uri, max_width=56, max_height=56)
    label(name)

response = interactive(tile)
if response.clicked:
    select(name)
```


## `draw_rect(rect, fill=None, stroke=None, rounding=0.0)`

Draws a rectangle over what was drawn before, e.g. the rectangle of a selection made with the
mouse.

**Args**

- `rect` (tuple[float, float, float, float]): (left, top, right, bottom) in points, like Response.rect
- `fill` (color): inside of the rectangle. Default is transparent
- `stroke` (tuple[float, color]): outline as (width, color). Default is no outline
- `rounding` (float): radius of the corners. Default is 0


## `dnd_drag_source(id, payload, update_fun)`

Makes the widgets drawn by update_func draggable. When they are dropped on a dnd_drop_zone,
the zone returns payload.

**Args**

- `id` (str): unique id of the drag source
- `payload` (Any): any Python object, returned by the drop zone
- `update_func` (Callable[[], None]): draws the dragged widgets

**Example:**

```python
for i, fruit in enumerate(fruits):
    dnd_drag_source(f"fruit {i}", fruit, lambda: label(fruit))
```


## `dnd_drop_zone(update_fun)`

Draws update_func in a frame that lights up when something is dragged over it.
Returns the payload of the dnd_drag_source dropped on it this frame, or None.

**Example:**

```python
basket = []

def update_func(ctx):
    dnd_drag_source("apple", "apple", lambda: label("🍎 apple"))
    dropped = dnd_drop_zone(lambda: label(f"Basket: {basket}"))
    if dropped is not None:
        basket.append(dropped)
```


## `dropped_files()`

The files dropped on the window this frame, from the Finder (the Explorer or another
application). Empty most of the time. The drop is a copy request: it is up to the app to copy
or move them, see ``hovered_files`` to show where they would go.

**Returns**

- list[str]: the full paths

**Example:**

```python
for path in dropped_files():
    shutil.copy(path, current_folder)
```


## `dragged_files()`

The files of the last drag started by this window (``start_file_drag``, the drag of the file
views). When ``dropped_files`` gives these same files, the drop comes from the window itself:
the app moves them instead of copying them, as the Finder does inside a disk.

**Returns**

- list[str]: the full paths, empty before a first drag

**Example:**

```python
dropped = dropped_files()
if dropped and sorted(dropped) == sorted(dragged_files()):
    move(dropped, folder)
```


## `hovered_files()`

The files being dragged over the window, from the Finder (the Explorer or another application),
not dropped yet. Empty when nothing is dragged over the window. It is the moment to show a
frame or a message "Drop here".

**Returns**

- list[str]: the full paths

**Example:**

```python
if hovered_files():
    draw_rect(rect, stroke=(accent, 2))
```


## `start_file_drag(paths)`

Starts a drag of files out of the window, to the Finder or to any application that takes files.
Call it on the frame a drag starts on an item (``response.drag_started`` of an
``interactive`` area, or a drag of the file views with ``drag_out=True``): the mouse button has
to be down. The drag goes on by itself, with the icons of the files under the pointer, and the
drop is done by the receiver (a copy in the Finder, a move with Cmd held). The window gets no
pointer event until the mouse button is released. Works on macOS, returns False elsewhere or if
there is nothing to drag.

**Args**

- `paths` (list[str]): the files and folders to drag

**Returns**

- `bool`: True if the drag started

**Example:**

```python
r = interactive("row", lambda: label(name))
if r.drag_started:
    start_file_drag([path])
```
