# Values that widgets edit

pyegui has no callbacks. A widget that edits a value takes a holder object (`Str`, `Int`, `Float`, `Bool`, `Date`, `RGB`), reads it each frame and writes `holder.value` when the user changes it. A holder is created once, outside the update function, and kept between frames. `Response` is what `label_response`-like functions return.

## `Str`


Str stores string value that can be referenced

**Usage:**

```python
data = Str("")

def update_func():
    heading(f"Value of the data is {data.value}")
    if button_clicked("Add :)"):
        data.value += ":) "
```


- `Str.value` (attribute)

## `Int`


Int stores integer value that can be referenced

**Usage:**

```python
data = Int(69)

def update_func():
    heading(f"Value of the data is {data.value}")
    if button_clicked("Increment"):
        data.value += 1
```


- `Int.value` (attribute)

## `Float`


Float stores float value that can be referenced

**Usage:**

```python
data = Float(69.0)

def update_func():
    heading(f"Value of the data is {data.value}")
    if button_clicked("Increment"):
        # what can go wrong?
        data.value += 0.1
```


- `Float.value` (attribute)

## `Bool`


Bool stores a boolean value that can be referenced

**Usage:**

```python
data = Bool(False)

def update_func():
    heading(f"Value of the data is {data.value}")
    # button will be shown only if the checkbox is checked
    if data.value and button_clicked("set to False"):
        # hiding the button
        data.value = False
    checkbox(data, "Check me")
```


- `Bool.value` (attribute)

## `Date`


Date picker

**Usage:**

```python
date = Date(datetime.datetime.now())
date_picker_button(date)
```


- `Date.value` (attribute)

## `RGB`


Rgb color picker

**Usage:**

```python
color_rgb = RGB(0.27, 0.27, 0.27)
color_edit_button_rgb(color_rgb)
```


- `RGB.b` (attribute)
- `RGB.g` (attribute)
- `RGB.r` (attribute)

## `RGBA`


A color with transparency, each channel between 0 and 1

**Usage:**

```python
color = RGBA(0.2, 0.5, 1.0, 0.5)
color_edit_button_rgba(color)
```


- `RGBA.a` (attribute)
- `RGBA.b` (attribute)
- `RGBA.g` (attribute)
- `RGBA.r` (attribute)
- `RGBA.to_tuple()` (method): The color as (r, g, b, a) with channels between 0 and 255, as taken by the style functions

## `Response`


What the user did with a widget during the frame, returned by label_response,
selectable_label_response and interactive


- `Response.clicked` (attribute): clicked with the primary button
- `Response.double_clicked` (attribute): double clicked with the primary button
- `Response.drag_started` (attribute): the drag started during this frame
- `Response.drag_stopped` (attribute): the drag ended during this frame
- `Response.dragged` (attribute): the widget is being dragged with the primary button
- `Response.hovered` (attribute): the mouse is over the widget
- `Response.rect` (attribute): where the widget is on the window, as (left, top, right, bottom) in points
- `Response.secondary_clicked` (attribute): clicked with the secondary button, usually the right one
