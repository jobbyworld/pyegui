# Layout, sizes and scrolling

Update functions can be nested: a function given to `horizontal`, `group`, `frame`... draws in a child ui with its own layout and style. Panels must be added before any other widget, see [panels.md](panels.md).

## `horizontal(update_fun)`

Start a ui with horizontal layout. After you have called this, the function registers the contents as any other widget.

Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. The initial height is style.spacing.interact_size.y. Centering is almost always what you want if you are planning to mix widgets or use different types of text.

If you don’t want the contents to be centered, use horizontal_top instead.

**Example:**

```python
def horizontal_update_func():
  heading("I'm horizontal")

horizontal(horizontal_update_func)
```


## `horizontal_centered(update_fun)`

Like horizontal, but allocates the full vertical height and then centers elements vertically.


## `horizontal_top(update_fun)`

Like horizontal, but aligns content with top.


## `horizontal_wrapped(update_fun)`

Start a ui with horizontal layout that wraps to a new row when it reaches the right edge of the max_size. After you have called this, the function registers the contents as any other widget.

Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. The initial height is style.spacing.interact_size.y. Centering is almost always what you want if you are planning to mix widgets or use different types of text.


## `vertical(update_fun)`

Start a ui with vertical layout. Widgets will be left-justified.


## `vertical_centered(update_fun)`

Start a ui with vertical layout. Widgets will be horizontally centered.


## `vertical_centered_justified(update_fun)`

Start a ui with vertical layout. Widgets will be horizontally centered and justified (fill full width).


## `centered_and_justified(update_fun)`

This will make the next added widget centered and justified in the available space.

Only one widget may be added inside update_func!


## `columns(count, update_fun)`

Splits the available width in columns of the same width.
update_func is called once for each column, with the index of the column.

**Example:**

```python
def column(i):
    heading(f"Column {i}")
    label("Some text")

columns(3, column)
```


## `grid(id, update_fun, **kwargs)`

A simple grid layout. Widgets are added left to right, call end_row to start a new row.

**Keyword args**

- `striped` (bool): every other row has a different background color
- `num_columns` (int): the number of columns, needed for striped grids to fill the width
- min_col_width (float), min_row_height (float)
- `spacing` (tuple[float, float]): space between columns and rows

**Example:**

```python
def grid_ui():
    label("Name")
    text_edit_singleline(name)
    end_row()
    label("Age")
    drag_int(age, 0, 150, 1)
    end_row()

grid("form", grid_ui, striped=True)
```


## `end_row()`

Moves to the next row of a grid.


## `group(update_fun, **kwargs)`

Visually groups the contents together.

**Example:**

```python
def update_func():
  heading("hi")
  heading("there")

group(update_func)
```

Accepts the same keyword args as the frame function: fill, stroke, rounding, inner_margin, outer_margin and shadow.


## `Group`


Visually groups the contents together.

Accepts the same keyword args as the frame function: fill, stroke, rounding, inner_margin, outer_margin and shadow.

**Usage:**

```python
with Group():
   heading("hi")
   heading("there")

with Group(fill=(255, 255, 255, 40), rounding=12):
   label("styled group")
```



## `indent(update_fun)`

Create a child ui which is indented to the right.
**Example:**

```python
def update_func():
  heading("I'm indented")
indent(update_func)
```


## `scope(update_fun)`

Create a scoped child ui.

You can use this to temporarily change the Style of a sub-region.

**Example:**

```python
def update_func():
  heading("0.5 opacity")
  set_opacity(0.5)

heading("normal opacity")
scope(update_func)
```


## `Scope`


Create a scope for the contents.
You can use this to temporarily change the Style of a sub-region

**Usage:**

```python
with Scope():
   set_opacity(0.5)
   heading("hi")
   heading("there")

heading("normal opacity")
```



## `Layout`


Layout class can be used to specify layout of widgets that go after with statement.

**Usage:**

```python
with Layout(LayoutType.VerticalCentered):
   heading("This widget will be horizontally centered")
```

**Layout types**

**Horizontal**

Start a ui with horizontal layout.

Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. Centering is almost always what you want if you are planning to mix widgets or use different types of text.

If you don’t want the contents to be centered, use HorizontalTop instead.

**HorizontalCentered**

Like Horizontal, but allocates the full vertical height and then centers elements vertically.

**HorizontalTop**

Like Horizontal, but aligns content with top.

**HorizontalWrapped**

Start a ui with horizontal layout that wraps to a new row when it reaches the right edge.

Elements will be centered on the Y axis, i.e. adjusted up and down to lie in the center of the horizontal layout. Centering is almost always what you want if you are planning to mix widgets or use different types of text.

**Vertical**

Start a ui with vertical layout. Widgets will be left-justified.

**VerticalCentered**

Start a ui with vertical layout. Widgets will be horizontally centered.

**VerticalCenteredJustified**

Start a ui with vertical layout. Widgets will be horizontally centered and justified (fill full width).

**CenteredAndJustified**

This will make the next added widget centered and justified in the available space.

Only one child widget is allowed!



## `LayoutType`


Values of this enum are used in Layout


- `LayoutType.CenteredAndJustified` (attribute): Values of this enum are used in Layout
- `LayoutType.Horizontal` (attribute): Values of this enum are used in Layout
- `LayoutType.HorizontalCentered` (attribute): Values of this enum are used in Layout
- `LayoutType.HorizontalTop` (attribute): Values of this enum are used in Layout
- `LayoutType.HorizontalWrapped` (attribute): Values of this enum are used in Layout
- `LayoutType.Vertical` (attribute): Values of this enum are used in Layout
- `LayoutType.VerticalCentered` (attribute): Values of this enum are used in Layout
- `LayoutType.VerticalCenteredJustified` (attribute): Values of this enum are used in Layout

## `frame(update_fun, **kwargs)`

A frame around the contents, with a custom background, border, rounding, margins and shadow.

**Keyword args**

- `fill` (color): background color. Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
- `stroke` (tuple[float, color]): border width and color
- `rounding` (float): corner radius
- inner_margin (float), outer_margin (float): space inside and outside the border
- `shadow` (False | tuple[int, color] | tuple[int, int, int, int, color]): (blur, color) or (offset_x, offset_y, blur, spread, color)

**Example:**

```python
def card():
    heading("Card")
    label("A frame with a translucent background")

frame(card, fill=(255, 255, 255, 60), stroke=(1, (255, 255, 255, 120)), rounding=16, inner_margin=12, shadow=(24, (0, 0, 0, 60)))
```


## `collapsing(heading, update_fun)`

A CollapsingHeader that starts out collapsed.

**Example:**

```python
def update_func():
  heading("hi")
collapsing("collapsed", update_func)
```


## `add_space(amount)`

Add extra space before the next widget.

The direction is dependent on the layout.
**Example:**

```python
add_space(5)
heading("I'm so spaced now")
```


## `separator()`

A visual separator. A horizontal or vertical line on layout.

**Example:**

```python
separator()
```


## `set_width(width)`

Sets the width of the current ui: the widgets after it wrap and justify to this width.

**Example:**

```python
def sidebar():
    set_width(200)
    label("200 points wide")
vertical(sidebar)
```


## `set_min_width(width)`

Sets the minimum width of the current ui


## `set_max_width(width)`

Sets the maximum width of the current ui


## `set_height(height)`

Sets the height of the current ui


## `set_min_height(height)`

Sets the minimum height of the current ui


## `set_max_height(height)`

Sets the maximum height of the current ui


## `available_width()`

Width left for the next widgets, in points.

**Example:**

```python
if available_width() < 400:
    label("narrow layout")
```


## `available_height()`

Height left for the next widgets, in points.


## `scroll_area_vertical(update_fun)`

Create a vertical scroll area.

**Example:**

```python
def update_func():
  heading("hi")
  heading("there")
  # a lot of elements

scroll_area_vertical(update_func)
```


## `scroll_area_horizontal(update_fun)`

Create a horizontal scroll area.

**Example:**

```python
def update_func():
  heading("hi")
  heading("there")
  # a lot of elements

scroll_area_horizontal(update_func)
```


## `scroll_area_both(update_fun)`

Create a scroll area that scrolls in both directions, with a bar for each.

**Example:**

```python
scroll_area_both(wide_and_tall_ui)
```


## `scroll_to_cursor(align=None)`

Scrolls the scroll area the current ui is in, so the next widget is visible.

**Args**

- `align` (str): where the next widget goes: "top", "center" or "bottom". Default is the least scrolling that shows it

**Example:**

```python
def log_ui():
    for line in lines:
        label(line)
    if new_line_added:
        scroll_to_cursor("bottom")

scroll_area_vertical(log_ui)
```


## `scroll_to_rect(rect)`

Scrolls the scroll area the current ui is in, just enough to show a rectangle, e.g. the
rect of the Response of the item that was selected with the keyboard.

**Args**

- `rect` (tuple[float, float, float, float]): (left, top, right, bottom) in points, like Response.rect

**Example:**

```python
response = interactive(tile)
if tile_was_selected_by_key:
    scroll_to_rect(response.rect)
```


## `disable()`

Calling disable() will cause the Ui to deny all future interaction and all the widgets will draw with a gray look.

Usually it is more convenient to use add_enabled.

Note that once disabled, there is no way to re-enable the Ui.

**Example:**

```python
disable()
if button_clicked("you can't click me"):
  pass
```


## `add_enabled(enabled, update_fun)`

Add a section that is possibly disabled, i.e. greyed out and non-interactive.

If you call add_enabled from within an already disabled Ui, the result will always be disabled, even if the enabled argument is true.

**Example:**

```python
add_enabled(False, lambda: button_clicked("you can't click me"))
button_clicked("but you can click me")
```


## `is_enabled()`

True if the widgets of the current ui can be interacted with, see disable() and add_enabled()


## `set_invisible()`

Calling set_invisible() will cause all further widgets to be invisible, yet still allocate space.

The widgets will not be interactive (set_invisible() implies disable()).

Once invisible, there is no way to make the Ui visible again.

**Example:**

```python
set_invisible()
heading("this will not be visible")
```


## `is_visible()`

True if the widgets of the current ui are shown, see set_invisible()


## `set_opacity(opacity)`

Make the widget in this Ui semi-transparent.

opacity must be between 0.0 and 1.0, where 0.0 means fully transparent (i.e., invisible) and 1.0 means fully opaque.
**Example:**

```python
set_opacity(0.5)
```
