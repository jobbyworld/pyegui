# Panels, windows, menus and popups

Side, top and bottom panels are placed on the borders of the window, the rest is the central panel. Call the panels first in the update function: a panel added after a widget is placed inside the current ui. `sidebar_panel`, `topbar_panel` and `bottombar_panel` are the panels of the layout rules: boxes with the same gap, rounding, outline and sheet, laid out for you (see `Context.set_layout`).

## `sidebar_panel(update_fun, **kwargs)`

The sidebar: a box at the left of the window, as tall as the window, that can be resized. On macOS
with fullsize_content the room of the window buttons is kept at its top.

**Keyword args**

- `id` (str): id of the panel. Default is "sidebar"
- default_size (float), min_size (float), max_size (float), exact_size (float), resizable (bool): the width, see left_panel
- `expanded` (Bool): the panel is shown when expanded.value is True
- gap, rounding, outline, sheet, opacity, inner_margin: the layout rules of this region, see Context.set_layout. They are looked up in the call, then in the rules of the sidebar, then in the global rules

**Example:**

```python
def update_func(ctx):
    sidebar_panel(lambda: label("Files"), default_size=200)
    topbar_panel(lambda: label("Title"))
    heading("Content")
```


## `topbar_panel(update_fun, **kwargs)`

The top bar: a box at the top of the window, after the sidebar if there is one. Accepts the same
keyword args as sidebar_panel, the size being the height. Default id is "topbar".

**Example:**

```python
def update_func(ctx):
    topbar_panel(lambda: heading("Title"))
```


## `bottombar_panel(update_fun, **kwargs)`

The bottom bar: a box at the bottom of the window. Accepts the same keyword args as
sidebar_panel, the size being the height. Default id is "bottombar".

**Example:**

```python
def update_func(ctx):
    bottombar_panel(lambda: small("Ready"))
```


## `window_controls(**kwargs)`

The three buttons of the window at the left of the bar, like macOS: close (red), minimize
(yellow) and maximize or restore (green). Their symbols appear when the pointer is over them.

Put it first in a horizontal row of the top bar, then `window_drag_area()` for the rest of the
row. Use it with `run_native(..., custom_titlebar=True)`. On macOS the window keeps the buttons of
the system (`fullsize_content`): nothing is drawn and False is returned.

**Keyword args**

- `size` (float): diameter of a button. Default is 12
- `gap` (float): space between the buttons. Default is 8

**Returns**

- `bool`: True when the buttons were drawn

**Example:**

```python
pyegui.run_native("App", update, custom_titlebar=True, transparent=True)

def update(ctx):
    def bar():
        pyegui.window_controls()
        pyegui.window_drag_area(title="My app")
    pyegui.topbar_panel(lambda: pyegui.horizontal(bar))
```


## `Viewport`


A window of the app besides the main one, with its own update function.

The windows share the process: the values (`Str`, `Int`...) and the state objects are those of the
app, so two windows can show the same data, or each its own. A window closed by the user (its
close button) is hidden, `open()` shows it again. Closing the main window hides it while other
windows are open: the app ends with the last one.

**Args**

- `title` (str): the title of the window
- `update_func` (Callable[[Context], None]): draws the window, like the one of run_native

**Keyword args**

- inner_width (float), inner_height (float): the size of the window. Default is 640 x 480
- min_inner_width (float), min_inner_height (float): the smallest size of the window
- `resizable` (bool): Default is True
- `fullsize_content` (bool): the content goes under the title bar of macOS. Default is False
- `decorations` (bool): False removes the title bar and the borders of the system (draw them with window_controls and window_drag_area). Default is True

**Methods**

- `open` (): shows the window (it is created at the next frame)
- `close` (): hides it
- `toggle` (): open or close
- `is_open` (bool): True while the window is shown

**Example:**

```python
def inspector(ctx):
    heading("Inspector")

window = Viewport("Inspector", inspector, inner_width=360, inner_height=480)

def update(ctx):
    if button_clicked("Inspector"):
        window.toggle()

run_native("App", update)
```


- `Viewport.close()` (method): Hides the window.
- `Viewport.is_open` (attribute)
- `Viewport.open()` (method): Shows the window.
- `Viewport.title` (attribute)
- `Viewport.toggle()` (method): Shows the window if it is hidden, hides it otherwise.

## `window_drag_area(**kwargs)`

The part of the bar that moves the window: dragging it moves the window, a double click maximizes
or restores it. It takes the rest of the width of the row it is in (or `width`), and can show the title of the
window in its middle.

**Keyword args**

- `title` (str): a text drawn in the middle of the area. Default is none
- `width` (float): width of the area. Default is the rest of the row
- `height` (float): height of the area. Default is the height of a row of the ui
- `fill_height` (bool): take all the height left in the ui, to make a whole band (the top band of a sidebar, a toolbar) move the window. Default is False

The double click maximizes the window, and restores the size and the position it had before when
it is maximized (the system remembers them).

**Example:**

```python
pyegui.horizontal(lambda: (pyegui.window_controls(), pyegui.window_drag_area(title="My app")))
```


## `left_panel(id, update_fun, **kwargs)`

A panel on the left side of the window. The rest of the window is the central panel.

Panels must be added before any other widget in update_func, otherwise they are placed inside the current ui.

**Keyword args**

- `resizable` (bool): whether the user can resize the panel. Default is True
- `show_separator_line` (bool): show a line between the panel and the rest of the window
- `separator_stroke` (tuple[float, color]): width and color of that line. Default is the line of the theme
- `default_size` (float): the initial width of the panel
- `min_size` (float): min width of the panel
- `max_size` (float): max width of the panel
- `exact_size` (float): fixed width of the panel
- `expanded` (Bool): show or hide the panel with an animation
- fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the panel, see the frame function

**Example:**

```python
def update_func(ctx):
    left_panel("navigation", lambda: label("Navigation"), default_size=150)
    heading("Content")
```


## `right_panel(id, update_fun, **kwargs)`

A panel on the right side of the window. Accepts the same keyword args as left_panel.

**Example:**

```python
def update_func(ctx):
    right_panel("properties", lambda: label("Properties"))
    heading("Content")
```


## `top_panel(id, update_fun, **kwargs)`

A panel on the top of the window. Accepts the same keyword args as left_panel, sizes being heights.

**Example:**

```python
def update_func(ctx):
    top_panel("menu", lambda: menu_bar(menus))
    heading("Content")
```


## `bottom_panel(id, update_fun, **kwargs)`

A panel on the bottom of the window. Accepts the same keyword args as left_panel, sizes being heights.

**Example:**

```python
def update_func(ctx):
    bottom_panel("status", lambda: small("Ready"))
    heading("Content")
```


## `window(title, update_fun, open=None, **kwargs)`

A floating window which can be moved, resized and collapsed.

**Args**

- `title` (str): title of the window, also used as its id
- `update_func` (Callable[[], None]): draws the contents of the window
- `open` (Bool): if given, the window shows a close button and is shown only when open.value is True

**Keyword args**

- `id` (str): id of the window, if the title is not unique or changes
- resizable (bool), collapsible (bool), movable (bool), title_bar (bool), scroll (bool)
- default_width (float), default_height (float), min_width (float), min_height (float)
- `default_pos` (tuple[float, float]): initial position of the window
- `fixed_pos` (tuple[float, float]): the window cannot be moved from this position
- `anchor` (str): the window is anchored to a corner or a side of the screen and cannot be moved: one of "left_top", "center_top", "right_top", "left_center", "center_center", "right_center", "left_bottom", "center_bottom", "right_bottom"
- `anchor_offset` (tuple[float, float]): the distance from the anchor, e.g. (-8, -40) from "right_bottom" puts the window 8 points from the right edge and 40 above the bottom one
- fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the window, see the frame function

**Example:**

```python
settings_open = Bool(True)

def update_func(ctx):
    toggle_value(settings_open, "Settings")
    window("Settings", lambda: checkbox(dark, "Dark mode"), open=settings_open)
```


## `modal(id, update_fun, **kwargs)`

A modal dialog. It blocks the interaction with the rest of the app.

Returns True when the user wants to close it, by pressing escape or clicking outside of the modal.

**Keyword args**

- `backdrop_color` (color): color covering the rest of the app
- fill, stroke, rounding, inner_margin, outer_margin, shadow: look of the modal, see the frame function

**Example:**

```python
confirm = Bool(False)

def confirm_ui():
    heading("Are you sure?")
    if button_clicked("Yes"):
        confirm.value = False

def update_func(ctx):
    if button_clicked("Delete"):
        confirm.value = True
    if confirm.value and modal("confirm", confirm_ui):
        confirm.value = False
```


## `SettingsWindow`


The state of a settings window: the width of its list, that the person can change with the splitter.
Create it once, outside the update function, and draw it with settings_window().

Attributes (read and write):
    list_width (float): the width of the list at the left, from 150 points

**Usage:**

```python
window_state = SettingsWindow()
```


- `SettingsWindow.list_width` (attribute)

## `settings_window(state, items, detail, **kwargs)`

A configuration window with the same frame everywhere: a title with its close cross, a list at the left
(rows that can be moved by drag and drop, a splitter to resize it, a button at its foot to add one), the
panel of the item chosen at the right, and a foot with the buttons of the window (the close button is
always at the right). It is a modal window: draw it only while it must be open.

It returns what happened, to be applied by the caller: `("select", id)`, `("move", from, to)` (the item at
index `from` goes to index `to`: reorder the items), `("add",)` (the panel must show a new item: pass
`adding=True` until it is created or given up) and `("close",)` (the cross, the close button or Escape:
stop drawing the window).

**Args**

- `state` (SettingsWindow): the state, created once
- `items` (list[dict]): the rows of the list, each a dict with `id`, `label` and an optional `hint` (a small text at the right of the row)
- `detail` (callable): draws the panel at the right: the item chosen, or the form of a new one

**Keyword args**

- `selected` (str): the id of the row chosen
- `footer` (callable): draws the left of the foot (a note, a secondary button)
- title (str), subtitle (str): the head of the window
- `list_title` (str): the small heading over the list
- `add_label` (str): the text of the button at the foot of the list, "Nouveau" by default
- `close_label` (str): the text of the close button, "Fermer" by default
- `empty_text` (str): shown when the list has no row
- width (float), height (float): the size of the window (720) and of the list and the panel (400)
- `reorderable` (bool): the rows can be moved by drag and drop. True by default
- `adding` (bool): the panel is the one of a new item, the add button is lit. False by default
- `id` (str): tells apart two windows
- fill, stroke, rounding, inner_margin, shadow: look of the window, see the frame function

**Example:**

```python
win = SettingsWindow()
names = ["Statut", "Client"]
chosen = Str("Statut")
adding = Bool(False)

def detail():
    label(chosen.value)

def update_func(ctx):
    if open_window.value:
        items = [dict(id=n, label=n, hint="texte") for n in names]
        for event in settings_window(win, items, detail, selected=chosen.value, title="Propriétés",
                                     add_label="Nouvelle propriété", adding=adding.value):
            if event[0] == "select":
                chosen.value = event[1]
            elif event[0] == "move":
                names.insert(event[2], names.pop(event[1]))
            elif event[0] == "close":
                open_window.value = False
```


## `tooltip(text, update_fun)`

Shows a text when the contents are hovered.

**Example:**

```python
tooltip("Saves the file", lambda: button_clicked("Save"))
```


## `tooltip_ui(update_fun, tooltip_fun)`

Shows a custom ui when the contents are hovered.

**Example:**

```python
def tooltip_ui():
    heading("Save")
    weak("Ctrl+S")

tooltip_ui(lambda: button_clicked("Save"), tooltip_ui)
```


## `context_menu(update_fun, menu_fun)`

Shows a menu when the contents are right-clicked.

**Example:**

```python
def menu():
    if button_clicked("Delete"):
        print("deleted")

context_menu(lambda: label("Right click me"), menu)
```


## `menu_bar(update_fun)`

A horizontal bar of menus, usually shown in a top_panel.

**Example:**

```python
def file_menu():
    if button_clicked("Quit"):
        exit()

def menus():
    menu_button("File", file_menu)

def update_func(ctx):
    top_panel("menu", lambda: menu_bar(menus))
```


## `menu_button(text, update_fun, size=None)`

A button showing a menu when clicked. Inside another menu, it shows a sub menu.

The menu is closed when a button inside it is clicked.

**Example:**

```python
def edit_menu():
    if button_clicked("Copy"):
        print("copy")
    menu_button("More", lambda: button_clicked("Paste special"))

menu_button("Edit", edit_menu)
menu_button("⚙", settings_menu, size=18)
```

**Args**

- `text` (str): the text of the button
- `update_func` (Callable[[], None]): draws the menu
- `size` (float): font size of the text of the button. Default is the size of the buttons


## `menu_image_button(source, update_fun, size=16.0, tint=None)`

A button with an image, showing a menu when clicked. See menu_button.

**Args**

- `source` (str): uri of the image, like for image()
- `update_func` (Callable[[], None]): draws the menu
- `size` (float): size of the image. Default is 16
- `tint` (color): multiplies the colors of the image. With a white icon, the icon takes the color

**Example:**

```python
menu_image_button("file://assets/settings.png", settings_menu)
```


## `menu_image_text_button(source, text, update_fun, size=16.0, tint=None)`

A button with an image and a text, showing a menu when clicked. See menu_button.

**Example:**

```python
menu_image_text_button("file://assets/settings.png", "Settings", settings_menu)
```


## `close_menu()`

Closes the menu, popup or modal the current ui is in.

**Example:**

```python
def menu():
    checkbox(data, "Option")
    if button_clicked("Done"):
        close_menu()
```
