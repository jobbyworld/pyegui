# Buttons and input widgets

Widgets the user acts on. Buttons and toggles return True during the frame they are clicked, the other widgets write into the holder they were given.

## `button_clicked(text, **kwargs)`

Returns true if the button was clicked this frame

**Keyword args**

- `fill` (color): background color. Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
- `stroke` (tuple[float, color]): border width and color
- `rounding` (float): corner radius
- `text_color` (color): color of the text
- `min_size` (tuple[float, float]): minimum width and height
- `frame` (bool): False shows only the text, until hovered

**Example:**

```python
if button_clicked("click me"):
  print("click me, my friend")

button_clicked("Save", fill="#3b82f6", text_color="#ffffff", rounding=20, min_size=(120, 32))
```


## `small_button_clicked(text)`

Returns true if the small button was clicked this frame

**Example:**

```python
if small_button_clicked("click me"):
  print("click me, my friend")
```


## `checkbox(checked, text)`

Show a checkbox.

**Example:**

```python
data = Bool(false)
# inside update_func
checkbox(data, "check me")
```


## `radio_clicked(selected, text)`

Shows a radio button that looks checked when selected is True. Returns True when it is clicked.

**Example:**

```python
dark = Bool(False)
# inside update_func
if radio_clicked(not dark.value, "Light"):
    dark.value = False
if radio_clicked(dark.value, "Dark"):
    dark.value = True
```


## `radio_value(current_value, alternative, text)`

Show a radio button. It is selected if current_value == selected_value. If clicked, selected_value is assigned to current_value.

**Example:**

```python
RED = 0
GREEN = 1
BLUE = 2

c = Int(RED)

radio_value(c, RED, "red")
radio_value(c, GREEN, "green")
radio_value(c, BLUE, "blue")
```


## `selectable_label_clicked(selected, text)`

Shows a text that looks selected when selected is True, like a tab or an item of a list.
Returns True when it is clicked.

**Example:**

```python
selected = Bool(False)
# inside update_func
if selectable_label_clicked(selected.value, "Bold"):
    selected.value = not selected.value
```


## `selectable_label_response(selected, text, size=None)`

Shows a text that looks selected when selected is True, and returns a Response: clicked,
double_clicked, secondary_clicked and hovered. A double click is also a click.

**Args**

- `selected` (bool): show the text as selected
- `text` (str): the text
- `size` (float): font size of the text. Default is the size of the body text

**Example:**

```python
response = selectable_label_response(name == selected.value, name)
if response.clicked:
    selected.value = name
if response.double_clicked:
    open_file(name)
```


## `selectable_value(current_value, alternative, text)`

Show selectable text. It is selected if current_value == selected_value. If clicked, selected_value is assigned to current_value.

**Example:**

```python
RED = 0
GREEN = 1
BLUE = 2

c = Int(RED)

selectable_value(c, RED, "red")
selectable_value(c, GREEN, "green")
selectable_value(c, BLUE, "blue")
```


## `toggle_value(selected, text)`

Acts like a checkbox, but looks like a selectable label.

**Example:**

```python
data = Bool(false)
# inside update_func
toggle_value(data, "check me")
```


## `switch(value, text=None)`

An on/off switch, the modern look of a checkbox. The track takes the accent color when on.

**Args**

- `value` (Bool): the state of the switch
- `text` (str): a label on the right of the switch

**Example:**

```python
notifications = Bool(True)
# inside update_func
switch(notifications, "Notifications")
```


## `segmented_control(selected, options, size=15.0, tint=None)`

A row of buttons in one rounded track, where exactly one is selected: the view switch of a
file manager (list, icons, tree). The selected one is filled with the accent color.

**Args**

- `selected` (Int): index of the selected option, written when the user picks another
- `options` (list[tuple[str | None, str | None]]): one (icon, text) per option. The icon is the uri of an image, like for image(), the text may be None for an icon only option
- `size` (float): size of the icons. Default is 15
- `tint` (color): color of the icons of the options that are not selected. The selected one is white. Default is the color of the icons as they are

Returns True during the frame the selection changed.

**Example:**

```python
view = Int(0)

def update_func(ctx):
    segmented_control(view, [("file://icons/list.svg", "List"), ("file://icons/grid.svg", "Icons")])
    if view.value == 0:
        list_view()
```


## `combo_box(current_value, alternatives, names, label)`

Shows a combo box with values defined in "alternatives" and their corresponding names
defined in "names"

**Example:**

```python
RED = 0
GREEN = 1
BLUE = 2

data = Int(RED)

def update_func(a):
    combo_box(data, [RED, GREEN, BLUE], ["red", "green", "blue"], "choose your fate")
```


## `slider_int(value, min, max, text)`

Control int with a slider.

**Example:**

```python
data = Int(5)
# inside update_func
slider_int(data, 0, 50, "slide me")
```


## `slider_float(value, min, max, text)`

Control float with a slider.

**Example:**

```python
data = Float(5)
# inside update_func
slider_float(data, 0, 50, "slide me")
```


## `drag_int(value, min, max, speed)`

Control int by dragging the number.

**Example:**

```python
data = Int(5)
# inside update_func
drag_int(data, 0, 50, 1)
```


## `drag_float(value, min, max, speed)`

Control float by dragging the number.

**Example:**

```python
data = Float(5)
# inside update_func
drag_float(data, 0, 50, 1.5)
```


## `drag_angle(value)`

Edits an angle by dragging. The value is in radians, it is shown in degrees.

**Example:**

```python
angle = Float(0.0)
# inside update_func
drag_angle(angle)
label(f"{math.degrees(angle.value):.0f}°")
```


## `text_edit_singleline(text, **kwargs)`

Show singleline text field and update the text

**Keyword args**

- `hint_text` (str): grey text shown when the field is empty
- `request_focus` (bool): give the field the keyboard focus this frame, for example to let the user type into it right as it appears, like a search field in a popup. Default is False
- `select_all` (bool): select the whole text this frame, so that typing or pasting replaces it, e.g. together with request_focus when a field opens on a value. Default is False

**Example:**

```python
text = Str("editable")
# inside update func
text_edit_singleline(text, hint_text="hint me bro")
```


## `text_edit_multiline(text, **kwargs)`

Show multiline text field and update the text

**Example:**

```python
text = Str("editable")
# inside update func
text_edit_multiline(text, hint_text="hint")
```


## `code_editor(text, desired_width=None, desired_rows=None)`

Show a multiline monospace text field for code and update the text

**Args**

- `text` (Str): the edited text
- `desired_width` (float): width of the field. float("inf") fills the available width. Default is the width of egui
- `desired_rows` (int): minimum number of visible rows. Default is 4

**Example:**

```python
text = Str("print(42 + 27)")
# inside update func
code_editor(text)
code_editor(text, desired_width=float("inf"), desired_rows=20)
```


## `date_picker_button(selection)`

Shows a date, and will open a date picker popup when clicked.

**Example:**

```python
date = Date(datetime.datetime.now())
# inside update_func
date_picker_button(date)
```


## `color_edit_button_rgb(rgb)`

Shows a button with the given color. If the user clicks the button, a full color picker is shown.

**Example:**

```python
color = RGB(0.27, 0.27, 0.27)
# inside udpate_func
color_edit_button_rgb(color)
heading(f"r:{color.r} g:{color.g} b:{color.b}")
```


## `color_edit_button_rgba(rgba)`

Shows a button with the given color. If the user clicks the button, a full color picker
with an opacity slider is shown.

**Example:**

```python
color = RGBA(0.2, 0.5, 1.0, 0.5)
# inside update_func
color_edit_button_rgba(color)
colored_label(color.to_tuple(), "This text uses the color")
```


## `progress(value)`

A simple progress bar.
value in the [0, 1] range, where 1 means “completed”.

**Example:**

```python
progress(0.5)
```


## `spinner()`

A spinner widget used to indicate loading.

**Example:**

```python
spinner()
```


## `Tags`


A list of tags and their colors. Create it once, outside the update function, and draw it with
tags_edit().

A tag has one or more levels separated by "/": "projet/web/front". Only its last level is shown
("front"); the mouse over it shows the full path. Each level has its own color, kept by its
path: "projet" and "projet/web" can have different colors, and the full path is shown with the
color of each of its levels. A level without a color gets one chosen from its path.

**Args**

- `tags` (list[str]): the tags at the start
- `colors` (dict[str, color]): the colors of levels, by path, e.g. {"projet": "#3b82f6", "projet/web": (16, 185, 129)}

**Usage:**

```python
tags = Tags(["projet/web", "perso"], colors={"projet": "#3b82f6"})

def update_func(ctx):
    if tags_edit(tags):
        print(tags.tags)
```


- `Tags.add(tag)` (method): Adds a tag. False when it is empty or already there.


  **Args**

  - `tag` (str): the full path, e.g. "projet/web"

- `Tags.color(path)` (method): The color of a level, "#rrggbb": the one given, or the one chosen from its path.


  **Args**

  - `path` (str): the path of the level, e.g. "projet" or "projet/web"

- `Tags.colors` (attribute): The colors given to levels, by path, as "#rrggbb": what to save to restore them. The

  levels without a color are not in it. Setting it replaces them.

- `Tags.remove(tag)` (method): Removes a tag. False when it was not there.


  **Args**

  - `tag` (str): the full path

- `Tags.set_color(path, color)` (method): Gives a color to a level; None goes back to the one chosen from its path.


  **Args**

  - `path` (str): the path of the level, e.g. "projet" or "projet/web"
  - `color` (tuple | str | None): (r, g, b), (r, g, b, a) or "#rrggbb[aa]"

- `Tags.tags` (attribute): The tags, full paths, in the order they were added. Setting it replaces them.

## `tags_edit(tags, **kwargs)`

Shows tags as chips, each one with its last level, its full path in the colors of its levels
on hover. With editable (the default) a cross removes a tag, a right click changes the color of
each of its levels, and a field adds one: "projet/web" then Enter. Backspace in the empty field
removes the last tag.

**Args**

- `tags` (Tags): the state, changed by the widget

**Keyword args**

- `editable` (bool): the cross, the menu and the field, True by default
- `hint_text` (str): the text of the empty field, "Add a tag" by default
- `suggestions` (list[str]): tags offered under the field while typing, e.g. all the known ones; those already there are left out
- `field_width` (float): the width of the field, 120 by default
- `compact` (bool): only a dot per tag in its color, the full path on hover, e.g. in a row of a list; read only
- `dot_size` (float): the diameter of the dots of compact, 10 by default
- `muted` (list[str]): tags drawn with their colors desaturated, e.g. those an item inherits
- `small` (bool): smaller chips, with the small text, e.g. in a row of a list or of a tree
- `max_visible` (int): with more tags, only the first ones are shown and then "+5": the mouse over it lists the others, a click shows them all (and "−" shows the first ones again)
- `id` (str): to tell apart two widgets of the same Tags

**Returns**

- `bool`: True when a tag was added or removed, or a color changed

**Example:**

```python
tags = Tags(["projet/web", "perso"])
known = ["projet/web", "projet/app", "perso", "perso/santé"]

def update_func(ctx):
    if tags_edit(tags, suggestions=known):
        save(tags.tags, tags.colors)
    tags_edit(tags, compact=True, id="row")  # the dots only
```


## `TagTreeView`


The state of a tree of tags: the levels folded shut and the one selected. Create it once,
outside the update function, and draw it with tag_tree_view().

The tree has a node for every level of the tags: "projet/web" and "projet/app" make a node
"projet" with two children. Each node shows the last level in the color of its path.

**Args**

- `selected` (str): the path of the level selected at the start
- `folded` (list[str]): the paths of the levels folded shut at the start; the others are open

**Usage:**

```python
tree = TagTreeView()

def update_func(ctx):
    clicked = tag_tree_view(tree, tags)
    if clicked:
        show_items_tagged(clicked)
```


- `TagTreeView.collapse_all(tags)` (method): Folds every level that has children, among the levels of tags


  **Args**

  - `tags` (Tags): the tags of the tree

- `TagTreeView.expand_all()` (method): Opens every level
- `TagTreeView.folded` (attribute): The paths of the levels folded shut, sorted: what to save to restore them. Setting it

  replaces them.

- `TagTreeView.selected` (attribute): The path of the level selected, or None

## `tag_tree_view(view, tags, **kwargs)`

Shows the tags as a tree: a node for every level, its last level in the color of its path,
the full path on hover. A click selects a level, the arrow (or a double click) folds or opens
it. A right click changes the color of the level or removes it from the tags, with the tags
under it.

**Args**

- `view` (TagTreeView): the state: the folded levels and the selection
- `tags` (Tags): the tags shown, changed when one is removed or a color changes

**Keyword args**

- `counts` (dict[str, int]): a number shown at the right of a level, e.g. the items tagged
- `editable` (bool): the menu of the right click, True by default
- `removable` (bool): the Remove entry of that menu, True by default; False keeps only the colors, e.g. when removing a tag would take it off many items

**Returns**

- str | None: the path of the level clicked during the frame (it is now view.selected)

**Example:**

```python
library = Tags(["projet/web", "projet/app", "perso/santé"])
tree = TagTreeView()

def update_func(ctx):
    def sidebar():
        clicked = tag_tree_view(tree, library, counts={"projet": 12, "projet/web": 5})
        if clicked:
            print("show", clicked)
    left_panel("tags", sidebar)
```
