# Themes, colors, fonts and glass

Everything that changes the look is a method of the `Context`, given to the update function of the top level. Call them from the first frame; `reset_style` goes back to the defaults before a new style is applied.

## `Context.is_dark_theme`

True when theme is dark


## `Context.is_light_theme`

True when theme is light


## `Context.set_dark_theme()`

Sets dark theme. Default is system's


## `Context.set_light_theme()`

Sets light theme. Default is system's


## `Context.set_system_theme()`

Sets system's theme if it has been changed.


## `Context.set_style(**kwargs)`

Changes the style of the app. Only the given options are changed.

The style is kept between frames, so this can be called once, for example on the first frame.

Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]". Strokes are (width, color).
Shadows are False, (blur, color) or (offset_x, offset_y, blur, spread, color).

**Keyword args**

- `theme` (str): "light" or "dark" to change only the style of this theme. Default changes both
- `rounding` (float): corner radius of widgets, windows and menus
- widget_rounding (float), window_rounding (float): corner radius of widgets or windows and menus only
- `panel_fill` (color): background of the panels and the central panel
- window_fill (color), window_stroke (stroke), window_shadow (shadow): look of windows, menus and popups
- `popup_shadow` (shadow): shadow of menus, tooltips and combo boxes
- button_fill (color), button_hover_fill (color), button_active_fill (color): background of buttons and other interactive widgets
- button_stroke (stroke), button_hover_stroke (stroke): border of interactive widgets
- text_color (color), hyperlink_color (color), selection_color (color)
- `extreme_bg_color` (color): background of text edits, plots and scroll bars
- `faint_bg_color` (color): background of striped rows
- `code_bg_color` (color): background of code
- item_spacing (tuple[float, float]), button_padding (tuple[float, float]), window_margin (float)
- `menu_margin` (tuple[float, float]): horizontal and vertical space inside the menus
- body_size (float), heading_size (float), monospace_size (float): font sizes
- `animation_time` (float): duration of animations in seconds

**Example:**

```python
def update_func(ctx):
    ctx.set_style(rounding=12, button_fill="#3b82f6", text_color=(240, 240, 240), body_size=15)
```


## `Context.reset_style()`

Restores the default style of both themes.


## `Context.set_layout(**kwargs)`

Sets the layout rules of the window: a few values given once, that the regions of the window
(the sidebar, the top bar, the bottom bar, the central area and the dock) take and can override.
Replaces the rules given before. The same dict can be given to run_native as layout.

**Keyword args**

- `gap` (float): space around the window and between two boxes. Default is 8
- `rounding` (float): corner radius of the boxes. Default is 12
- `outline` (tuple[float, color]): outline of the boxes and of the panes of the dock. Default is a
- thin translucent grey
- `opacity` (float): from 0 to 1, how opaque the background of the boxes is. With a backdrop
- or a transparent window the rest shows through. Default is 1
- `inner_margin` (float): space between the outline of a box and its content. Default is 12 for
- the sidebar and 8 for the others
- sidebar, topbar, bottombar, central, dock (dict): the same values for one region only

A value is looked up in the call of the region (sidebar_panel(..., rounding=6)), then in the
dict of the region, then in the global values, then in the defaults.

**Example:**

```python
ctx.set_layout(gap=8, rounding=12, opacity=0.9, sidebar=dict(opacity=0.6), dock=dict(rounding=8))
```


## `Context.set_accent(color)`

Changes the accent color: selection, links, text cursor, active pill tabs and the plot
items without a color.

**Example:**

```python
ctx.set_accent("#22c55e")
```


## `Context.set_glass_style(tint=None, accent=None, mode='partial')`

Applies a modern style looking like frosted glass, for both themes: translucent surfaces,
the Inter font with semibold titles, and an accent color for the selection, links, active
tabs and plots. With the system setting "Reduce transparency" (macOS), the surfaces are opaque.

Works best with a backdrop effect: run_native(..., backdrop=Backdrop.MICA).
It can be customized afterwards with set_style. reset_style removes it.

**Args**

- `tint` (color): color of the glass. Default is white on light theme and smoked on dark theme
- `accent` (color): color of the selection, links, active tabs and plots. Default is blue
- `mode` (str): "partial": the panels and menus are glass, the content is an almost opaque sheet, which is easier to read. "full": every surface is glass. Default is "partial"

**Example:**

```python
def update_func(ctx):
    ctx.set_glass_style(accent="#8b5cf6")
    heading("Glass")

run_native("Glass", update_func, backdrop=Backdrop.ACRYLIC)
```


## `Context.set_backdrop(backdrop)`

Changes the backdrop effect behind the window, for example to follow the theme.

The window must have been created with a backdrop: run_native(..., backdrop=Backdrop.MICA).

**Args**

- `backdrop` (str): same values as the backdrop argument of run_native

**Example:**

```python
ctx.set_dark_theme()
ctx.set_backdrop(Backdrop.MICA_DARK)
```


## `Context.set_font(source)`

Tell egui which fonts to use.

The default egui fonts only support latin and cyrillic alphabets, but you can call this to install additional fonts that support e.g. Japanese characters.

The new fonts will become active at the start of the next pass. This will overwrite the existing fonts.
 
**Example:**

```python
def update_func(ctx):
  ctx.set_font("NotoSansJP-VariableFont_wght.ttf")
  heading("天気の子")
```
