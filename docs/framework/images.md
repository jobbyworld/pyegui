# Images and icons

Images are given as uri: `file://` followed by a path, or an `http(s)` url. SVG icons stay sharp at any size and can be tinted.

## `image(source, **kwargs)`

Show an image available at the given uri.

**Keyword args**

- max_width (int), max_height (int): the image is scaled down to fit
- `size` (float): shows the image in a square of this side, scaled up or down. The space is kept while the image loads, so the layout does not jump when it appears
- `tint` (color): multiplies the colors of the image. With a white icon, the icon takes the color

**Example:**

```python
image("https://picsum.photos/480")
image("file://icons/folder.svg", size=48)
image("file://assets/ferris.png", max_height = 50, max_width = 50)
image("file://icons/gear.svg", max_width=16, max_height=16, tint=(120, 120, 128))
```


## `image_button_clicked(source, text=None, size=16.0, tint=None, selected=False, frame=True)`

A button showing an image, and a text if given. Returns True when it is clicked.

**Args**

- `source` (str): uri of the image, like for image(). SVG icons stay sharp at any size
- `text` (str): a text on the right of the image
- `size` (float): size of the image. Default is 16
- `tint` (color): multiplies the colors of the image. With a white icon, the icon takes the color
- `selected` (bool): show the button as selected, like a toggle. Default is False
- `frame` (bool): draw the background of the button. False shows it only when the button is hovered or selected, like the items of a sidebar. Default is True

**Example:**

```python
if image_button_clicked("file://icons/list.svg", "List", selected=view == LIST):
    view = LIST
```


## `image_and_text_clicked(source, text)`

Creates a button with an image to the left of the text

**Example:**

```python
if image_and_text_clicked("https://picsum.photos/480", "click me"):
  print("clicked")
```


## `file_icon(path, size=32.0)`

Returns the native icon of a file or folder, as the file manager of the platform shows it,
to draw with image(). Returns None when the platform has no icon to give: draw your own then.

For now only macOS gives icons, the ones of the Finder.

**Args**

- `path` (str): path of the file or folder
- `size` (float): size of the icon in points, it is rendered for the pixel density of the screen. Default is 32

**Example:**

```python
icon = file_icon("/Applications", 48)
if icon:
    image(icon, max_width=48, max_height=48)
else:
    label("📁", size=40)
```
