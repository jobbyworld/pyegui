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


## `segmented_control(selected, options, size=15.0, tint=None, tooltips=None)`

A row of buttons in one rounded track, where exactly one is selected: the view switch of a
file manager (list, icons, tree). The selected one is filled with the accent color.

**Args**

- `selected` (Int): index of the selected option, written when the user picks another
- `options` (list[tuple[str | None, str | None]]): one (icon, text) per option. The icon is the uri of an image, like for image(), the text may be None for an icon only option
- `size` (float): size of the icons. Default is 15
- `tint` (color): color of the icons of the options that are not selected. The selected one is white. Default is the color of the icons as they are
- `tooltips` (list[str | None]): a text shown when an option is hovered, one per option (None for none). Default is no tooltip

Returns True during the frame the selection changed.

**Example:**

```python
view = Int(0)

def update_func(ctx):
    segmented_control(view, [("file://icons/list.svg", "List"), ("file://icons/grid.svg", "Icons")])
    if view.value == 0:
        list_view()
```


## `segmented_buttons(options, id, size=16.0)`

A row of icon buttons in one rounded frame, like the controls of the top and bottom bands of the
Finder: the cells that are on are filled with the accent color, a cell can be disabled, have a
tooltip and a menu opened by a right click. Unlike segmented_control it holds no selection: it
returns the index of the cell clicked, and the app decides what is on.

**Args**

- `options` (list[dict]): one dict per cell: "icon" (the uri of an image, like for image()), and optionally "tooltip" (str), "on" (bool, filled with the accent color), "enabled" (bool, True by default) and "menu" (Callable[[], None], draws the menu of a right click)
- `id` (str): tells apart two controls of one ui
- `size` (float): size of the icons. Default is 16

Returns the index of the cell clicked during the frame, or None.

**Example:**

```python
picked = segmented_buttons(
    [
        {"icon": "file://icons/chevron-left.svg", "enabled": can_back, "tooltip": "Back"},
        {"icon": "file://icons/chevron-right.svg", "enabled": can_forward, "tooltip": "Forward"},
    ],
    id="nav",
)
if picked == 0:
    back()
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

Returns a Response: `lost_focus` is true during the frame where Enter, Tab or a click elsewhere ends the
typing (what to do with the text, a cell of a table for instance), `has_focus` while it is typed and `changed`
when a key changed the text.

**Keyword args**

- `hint_text` (str): grey text shown when the field is empty
- `request_focus` (bool): give the field the keyboard focus this frame, for example to let the user type into it right as it appears, like a search field in a popup. Default is False
- `select_all` (bool): select the whole text this frame, so that typing or pasting replaces it, e.g. together with request_focus when a field opens on a value. Default is False
- `frame` (bool): draw the frame of the field. False gives a bare field, for a cell of a table. Default is True
- `desired_width` (float): width of the field. float("inf") fills the available width

**Example:**

```python
text = Str("editable")
# inside update func
text_edit_singleline(text, hint_text="hint me bro")
if text_edit_singleline(text, frame=False).lost_focus:
    save(text.value)
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


## `DateRange`


A range of dates for `date_range_picker`, both ends included. An end that is None is open: a range
with a start and no end is "from this date on" (>=), a range with an end and no start is "up to this
date" (<=), a range with neither is every date.

**Usage:**

```python
period = DateRange(datetime.date(2026, 1, 1), None)   # >= 2026-01-01
date_range_picker(period)
if period.start is not None:
    ...
```


- `DateRange.contains(day)` (method): True when `day` is in the range (an open end has no limit)
- `DateRange.end` (attribute)
- `DateRange.start` (attribute)

## `date_range_picker(selection, **kwargs)`

Shows a range of dates, and opens a calendar when clicked. The calendar has three modes: "Entre" (a
click on the start, then on the end), "À partir de" (>= a date, no end) and "Jusqu'à" (<= a date, no
start). "Effacer" gives every date.

**Keyword args**

- `id` (str): tells apart several pickers of the same ui. Default is "range"

**Returns**

- `bool`: True during the frame the range was changed

**Example:**

```python
period = DateRange(None, None)
# inside update_func
if date_range_picker(period):
    print(period.start, period.end)   # either can be None: an open end
```


## `parse_date(text)`

The date a text says, in French: "12/03/2026" (day first), "demain", "lundi prochain", "dans 3 jours",
"il y a 2 semaines", "12 mars", "fin mars". None when the text is not understood.

**Example:**

```python
parse_date("lundi prochain")   # datetime.date(...)
```


## `parse_date_range(text)`

A range of dates typed by a person, as (start, end) with None for an open end, or None when it is not
understood: "cette semaine", "ce mois-ci", "depuis lundi", "jusqu'au 20/03", "12/03 au 20/03", "demain",
"tout".

**Example:**

```python
start, end = parse_date_range("depuis lundi")   # end is None
```


## `human_day(day)`

A day said the way people say it: "Aujourd'hui", "Hier", "lundi dernier", "12 mars", "12 mars 2025".

**Example:**

```python
human_day(datetime.date.today())   # "Aujourd'hui"
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
- `on_hover` (bool): the chips are read only until the mouse is over the widget, e.g. the cell of a row: then the crosses show and an empty tag, a field in a pill like the chips with a chevron, to type or choose a tag. The pill stays while it has the focus. What is typed is kept by the widget (give each row its own id). The tags in muted have no cross. False by default
- `dropdown` (bool): the field shows the list of all the suggestions as soon as it has the focus, filtered by what is typed, instead of only while typing. The keys Down and Up move a highlighted row (the list scrolls with it), Enter adds it, Escape clears what is typed. False by default
- `pick` (bool): the dropdown lists all the suggestions with a check box (checked for the tags that are on) and a click puts a tag on or takes it off while the list stays open, to choose several. False by default
- `list_height` (float): the preferred height of the list of suggestions, which scrolls beyond it. Default is 220
- `compact` (bool): only a dot per tag in its color, the full path on hover, e.g. in a row of a list; read only
- `dot_size` (float): the diameter of the dots of compact, 10 by default
- `icon` (bool): the tag icon in the color of the tag at the left of a chip, instead of a dot. False by default
- `muted` (list[str]): tags drawn with their colors desaturated, e.g. those an item inherits
- `small` (bool): smaller chips, with the small text, e.g. in a row of a list or of a tree
- `fit` (bool): shows as many tags as the room of the ui allows and the others in the "+5" badge, e.g. in a cell of a table; the room of the crosses and of the field is kept even when they do not show. Replaces max_visible. False by default
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
- `TagTreeView.take_deleted()` (method): The levels deleted with the trash since the last call (see `deletable` of tag_tree_view): the app

  removes them from its own data, they are not removed from the Tags.

- `TagTreeView.take_moved()` (method): The move asked for with the mouse since the last call (see `movable` of tag_tree_view), as

  (path, parent, before): the level goes under `parent` (None: at the top), before the level `before`
  of that parent (None: last). The app moves it in its own data.


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
- `deletable` (bool): a trash at the right of the row under the pointer: a first click turns it red, a second one deletes the level (it is given by `view.take_deleted()`, the app removes it from its data). False by default
- `movable` (bool): the rows can be dragged: dropped on the middle of another level it goes under it, on its upper or lower edge it goes before or after it. The move is given by `view.take_moved()` as (path, parent, before), the app moves it in its data. False by default
- `keep_order` (bool): the levels stay in the order of the tags instead of being sorted by name, so that a move before or after another level shows. False by default
- `icon` (bool): the tag icon in the color of the level instead of a dot. False by default
- `checks` (dict[str, str]): a check box at the left of every row, in the state given by the path of the level: "on", "off" or "mixed" (a level that is not in the dict is off). The tree then picks tags, e.g. for the selected items: a click on a row is returned like a selection, the app puts the tag on or takes it off.

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


## `PropertySet`


The state of a property set: by group or by name, the groups folded, the filter typed and the row
selected. Create it once, outside the update function, and draw it with property_set().

**Args**

- `grouped` (bool): rows in groups (categories) that fold (True, default) or sorted by name

Attributes (read and write):
    grouped (bool): the order of the rows

    filter (str): the text of the filter field

    selected (str | None): the id of the row selected

    split (float): the width of the name column, from 0.2 to 0.8 of the width

    collapsed (list[str]): the groups folded

**Usage:**

```python
props = PropertySet()

def update_func(ctx):
    for change in property_set(props, [...]):
        print(change)
```


- `PropertySet.collapsed` (attribute)
- `PropertySet.filter` (attribute)
- `PropertySet.grouped` (attribute)
- `PropertySet.selected` (attribute)
- `PropertySet.split` (attribute)

## `property_set(state, properties, **kwargs)`

Draws a property set and returns what the person changed.

A property set is the property grid of an IDE (the "Properties" window of Visual Studio): a table of two
columns, the name and an editor of the value. The rows are kept in groups (categories) that fold when their
header is clicked, or sorted by name; a filter field looks in the names, groups, descriptions and values; the
splitter between the columns can be dragged; the description of the row selected is shown at the bottom.

The widget keeps no value. Give it the current properties at each frame, apply the changes it returns,
and give it the new values at the next frame. A change the caller does not apply (a text that is not a
number) is undone by itself when the field loses the focus.

**Args**

- `state` (PropertySet): the state (create it once, outside the update function)
- `properties` (list[dict]): the rows, in order. A row is a dict with - ``id`` (str): what a change names (default: ``name``) - ``name`` (str): the text of the name column - ``type`` (str): ``"text"`` (default), ``"longtext"``, ``"number"``, ``"bool"``, ``"date"``, ``"choice"`` or ``"readonly"`` - ``value``: a str for a text, number or date (what is typed), a bool for a bool, the id (or a list of ids) of the options chosen for a choice; None for no value - ``group`` (str): its group (category) - ``description`` (str): shown at the bottom when the row is selected, and when the mouse is over its name - ``options`` (list): for a choice, ``str`` or ``{"id", "label", "color"}`` - ``multiple`` (bool), ``open`` (bool): a choice that takes several options, and one the person can add options to - ``note`` (str): what the row has without a value of its own (``"inherited from X"``), shown weak where the value would be - ``hint`` (str): shown in an empty field - ``unit`` (str): after a number - ``readonly`` (bool): shown, not editable

**Keyword args**

- `key` (str): what the properties are about (a path, a name): when it changes, the texts being typed and the selection are dropped (default "")
- `id` (str): tells two property sets with one state apart
- `toolbar` (bool): the order buttons and the filter field (default True)
- `description` (bool): the description of the row selected at the bottom (default True)
- `row_height` (float): default 26
- `default_group` (str): the group of the rows that have none (default "Misc")
- grouped_label, alphabetical_label, filter_hint, clear_tip, add_hint, empty_text (str): the texts of the widget, to translate it

**Returns**

- list[tuple]: the changes of this frame, each ``("set", id, text)`` (a text, or "1" / "0" for a bool),
- ``("clear", id)``, ``("choose", id, [option ids])`` or ``("add_option", id, label)`` (a new option of an
- open list, to add and choose)

**Example:**

```python
values = {"name": "Report", "done": False}
props = PropertySet()

def update_func(ctx):
    rows = [
        dict(id="name", name="Name", group="General", value=values["name"], description="The title"),
        dict(id="done", name="Done", type="bool", group="General", value=values["done"]),
        dict(id="size", name="Size", type="number", group="Layout", unit="cm", value="21"),
    ]
    for change in property_set(props, rows):
        if change[0] == "set" and change[1] == "name":
            values["name"] = change[2]
        elif change[0] == "set" and change[1] == "done":
            values["done"] = change[2] == "1"
```
