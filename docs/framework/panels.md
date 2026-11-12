# Panels, windows, menus and popups

Side, top and bottom panels are placed on the borders of the window, the rest is the central panel. Call the panels first in the update function: a panel added after a widget is placed inside the current ui.

## `left_panel(id, update_fun, **kwargs)`

A panel on the left side of the window. The rest of the window is the central panel.

Panels must be added before any other widget in update_func, otherwise they are placed inside the current ui.

**Keyword args**

- `resizable` (bool): whether the user can resize the panel. Default is True
- `show_separator_line` (bool): show a line between the panel and the rest of the window
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
