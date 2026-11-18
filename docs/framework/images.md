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

macOS gives the icons of the Finder and Windows the ones of the Explorer (the shell, so the
icons of the file types installed on the machine, thumbnails excluded). Linux gives none.

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


## `set_blue_folders(enabled=True)`

Draws the icons of the folders in blue, in the style of the Finder of macOS, instead of the
yellow of the Explorer of Windows. It changes the icons given by file_icon and drawn by the
file views. It has no effect on macOS (already blue) nor on Linux. Default is False

**Args**

- `enabled` (bool): True for blue folders, False for the native ones

**Example:**

```python
set_blue_folders(True)  # once, before the first frame
```


## `ImageView`


The state of an image viewer: the zoom (1.0 is the size of the image), the place of the image
and whether it is fitted to the room. Create it once, outside the update function; it goes back
to the fit when `image_view` is given another source.

Properties: `zoom` (float, 1.0 is 100%, writing it turns `fit` off), `fit` (bool, the image is
scaled down to the room, True by default). `reset()` fits again and centers the image.

**Example:**

```python
view = ImageView()

def update_func(ctx):
    if button_clicked("Fit"):
        view.reset()
    label(f"{view.zoom:.0%}")
    image_view(view, "file:///tmp/photo.png")
```


- `ImageView.fit` (attribute): True to scale the image down to the room (never up)
- `ImageView.reset()` (method): Fits the image to the room and centers it.
- `ImageView.zoom` (attribute): The zoom, 1.0 is the size of the image. The value used at the last frame when `fit` is on.

## `image_view(view, source, **kwargs)`

Shows an image in a viewer that takes all the space available (or the height given). The wheel
(or a pinch) zooms around the pointer, a drag moves the image, a double click fits it again,
Ctrl + and Ctrl - zoom and Ctrl 0 fits, with the pointer over the viewer.

**Args**

- `view` (ImageView): the state of the viewer
- `source` (str): the uri of the image: `file://` and a path, or an `http(s)` url

**Keyword args**

- `height` (float): the height, the available one by default
- `background` (tuple | str): the color behind the image, none by default

**Example:**

```python
view = ImageView()

def update_func(ctx):
    image_view(view, "file:///tmp/photo.png", background="#202020")
```
