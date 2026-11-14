# Application, window and context

`run_native` opens the window and calls your update function once per frame. The update function of the top level receives the `Context`, which holds what is global to the app: the theme, the fonts, the clipboard, opening URLs, the state saved between two runs (`persist_id`, `save_state`, `load_state`). The styling methods of the context are in [styling.md](styling.md).

## `run_native(app_name, update_func, **kwargs)`

Creates a window and runs update_func.
This is an entrypoint for your GUI application.

**Args**

- `app_name` (str): name displayed at the header bar
- `update_func` (Callable[[Context], None]): your function that draws UI
- `inner_height` (float): the desired height of the window
- `inner_width` (float): the desired width of the window
- `min_inner_height` (float): min height of the window
- `min_inner_width` (float): min width of the window
- `max_inner_height` (float): max height of the window
- `max_inner_width` (float): max width of the window
- `fullscreen` (bool): whether to open app in fullscreen
- `maximized` (bool): whether to open app maximized
- `resizable` (bool): whether our app is resizable
- `transparent` (bool): whether our app is transparent
- `icon_path` (str): path to icon in rgba format
- `backdrop` (str): blurs what is behind the window, which makes it transparent. Use with ctx.set_glass_style().
- "mica", "tabbed" (Windows 11), "acrylic", "blur" (Windows 10 and 11). Add "_light" or "_dark" to mica, tabbed
- and acrylic to force their tint, otherwise it follows the theme of the system,
- "vibrancy" (macOS), "liquid_glass" and the more see-through "liquid_glass_clear" (macOS 26).
- The Backdrop class has a constant for each of them, like Backdrop.MICA.
- Ignored with a warning on other platforms
- `persist_id` (str): a name that is unique to the app, e.g. "mycompany.finder". The position and
- size of the window, what egui remembers (scroll positions, collapsed sections) and the state
- given to ctx.save_state() are saved on disk under this name and come back at the next run.
- Nothing is saved without it
- `fullsize_content` (bool): macOS: the content fills the window under a transparent title bar,
- keeping the window buttons at the top left. Leave about 28 points free at the top of the
- first panel. Ignored on other platforms
- `layout` (dict): the layout rules of the window: spacing (gap), corners (rounding), outline,
- background between the boxes, sheet inside the boxes, opacity and inner_margin, given once and
- overridden by region ("sidebar", "topbar", "bottombar", "central", "dock"). They are used by
- sidebar_panel, topbar_panel, bottombar_panel, the Dock and the central area. It is the same as
- calling set_layout on the context, see its documentation. Example:
- layout=dict(gap=8, rounding=12, dock=dict(opacity=0.8))

**Examples:**

```python
name = Str("")

def update_func(ctx):
    heading(f"Hello, {name.value}!")
    text_edit_singleline(name)

    if button_clicked("click me"):
        print("clicked")

run_native("My app", update_func)
```


## `Context`


Context object that controls global aspects of your app

**Usage:**

```python
def update_func(ctx):
    ctx.set_light_theme
    heading("Using light theme even if system's is dark")

run_native("My app", update_func)
```


- `Context.copy_text(text)` (method): Copy the given text to the system clipboard.
- `Context.load_state()` (method): Returns the state saved by the previous run with save_state(), or None the first time (or

  without persist_id in run_native). Call it on the first frame, before drawing.

  **Example:**

  ```python
  def update_func(ctx):
      if not started:
          saved = ctx.load_state() or {}
          theme.value = saved.get("theme", 0)
          started = True
  ```

- `Context.open_url(url)` (method): Opens an URL with whatever the OS has registered for it: the browser for a http(s)

  address, or an installed application for its own scheme, e.g. "claude://..." for Claude
  Desktop.

  **Args**

  - `url` (str): the address to open

  **Example:**

  ```python
  ctx.open_url("https://example.com")
  ```

- `Context.request_repaint(after=0.0)` (method): Asks for another call of update_func, for an animation: the app is otherwise drawn again

  only when something happens (mouse, keys, a timer of its own).

  **Args**

  - `after` (float): wait this many seconds first. Default is 0, as soon as possible

  **Example:**

  ```python
  if time.monotonic() - started < 1.0:  # a fade of one second
      ctx.request_repaint()
  ```

- `Context.save_state(state)` (method): Remembers the state of the app, any value json can write (dict, list, str, numbers, bool,

  None), to give it back to load_state() at the next run. It is written to disk every 30
  seconds and when the window closes, only if run_native has a persist_id. The last call
  wins: call it every frame with the whole state, it is cheap, or only when it changes.

  **Example:**

  ```python
  ctx.save_state({"theme": theme.value, "folder": str(current_folder)})
  ```


## `Backdrop`


Names of the backdrop effects, for run_native(backdrop=...) and ctx.set_backdrop().
They are plain strings, so "mica" works as well as Backdrop.MICA.

- MICA, TABBED: Windows 11
- ACRYLIC, BLUR: Windows 10 and 11
- the _LIGHT and _DARK variants of MICA, TABBED and ACRYLIC force the tint instead of following
  the theme of Windows
- VIBRANCY: macOS
- LIQUID_GLASS, LIQUID_GLASS_CLEAR (barely blurred): macOS 26

**Usage:**

```python
run_native("Glass", update_func, backdrop=Backdrop.LIQUID_GLASS)
ctx.set_backdrop(Backdrop.ACRYLIC_DARK)
```


- `Backdrop.ACRYLIC` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.ACRYLIC_DARK` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.ACRYLIC_LIGHT` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.BLUR` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.LIQUID_GLASS` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.LIQUID_GLASS_CLEAR` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.MICA` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.MICA_DARK` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.MICA_LIGHT` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.TABBED` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.TABBED_DARK` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.TABBED_LIGHT` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.

- `Backdrop.VIBRANCY` (attribute): str(object='') -> str

  str(bytes_or_buffer[, encoding[, errors]]) -> str

  Create a new string object from the given object. If encoding or
  errors is specified, then the object must expose a data buffer
  that will be decoded using the given encoding and error handler.
  Otherwise, returns the result of object.__str__() (if defined)
  or repr(object).
  encoding defaults to sys.getdefaultencoding().
  errors defaults to 'strict'.


## `pixels_per_point()`

Number of physical pixels for each point of the ui. 2.0 on most retina screens
