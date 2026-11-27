# Themes, colors, fonts and glass

Everything that changes the look is a method of the `Context`, given to the update function of the top level. Call them from the first frame; `reset_style` goes back to the defaults before a new style is applied. `set_layout` (or `run_native(layout=dict(...))`) gives the spacing, corners, outline, background, sheet and opacity of the whole window once, and a region (`sidebar`, `topbar`, `bottombar`, `central`, `dock`) or a call overrides them.

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


## `Context.style_snapshot(theme=None)`

The style a theme is drawn with now, as plain data: what the widgets take their colors, strokes,
corners and fonts from. Read it to document or to compare styles; to change it use `set_style`.

**Args**

- `theme` (str): "light" or "dark". Default is the theme shown now

Colors are "#rrggbbaa" strings, a stroke is (width, color), a shadow is (offset_x, offset_y, blur, spread, color).
The dict has the keys `theme`, `colors` (panel, window, text edit and code backgrounds, links, selection, ...),
`widgets` (for `noninteractive`, `inactive`, `hovered`, `active` and `open`: `fill`, `weak_fill`, `stroke`,
`fg_stroke` (the color of the text and icons), `rounding`, `expansion`), `window` (stroke, rounding, shadows),
`fonts` (the name of a text style to (size, family)) and `spacing`.

**Example:**

```python
s = ctx.style_snapshot("dark")
print(s["widgets"]["hovered"]["weak_fill"], s["colors"]["text_edit_bg"])
```


## `Context.set_style_tokens(theme, tokens)`

Changes any value of the style of a theme by its path, the same paths as the dict of `style_snapshot`
(`colors.panel`, `widgets.hovered.weak_fill`, `widgets.active.stroke`, `window.rounding`, `fonts.Body`,
`spacing.item_spacing`, `tabs.active_fill`, `pill_tabs.spacing`, `grid.guide_line`, `grid.chevron`, `popups.dropdown_fill`...). The widgets of a context (`menu`, `dropdown`, `sidebar`, `topbar`, `bottombar`, `window`) have their own tokens `<context>.<state>.<field>`, e.g. `menu.hovered.weak_fill`, looked up before `widgets.<state>.<field>`; `<context>.button_padding` (x, y) and `<context>.button_padding_radius` give the padding of their buttons, `spacing.button_padding_radius` does it for the whole app: a factor of the rounding of the buttons that the padding at the left and at the right is at least (a round button needs room for its corners). The fill of the popups is an RGBA color whose alpha is the transparency: `popups.dropdown_fill` (the list of a combo box), `popups.menu_fill` (menus and context menus) `popups.tooltip_fill` (tooltips) and `popups.window_fill` (windows and modals, while `colors.window` is the fill under the whole window of the app), e.g. `{"popups.dropdown_fill": "#1e1e2e80"}` for a half transparent dropdown. The sheet of a region of the layout is a token too: `layout.sidebar.sheet` (also `topbar`, `bottombar`, `central`, `dock`) and `layout.sheet` for all of them, RGBA colors like `"#1e1e2ecc"`; the sheet of a call or of the rules of the region (`set_layout`) comes first. The tabs of a `Dock` and of a `tab_view` read the
`tabs.*` (classic tabs) and `pill_tabs.*` tokens. Only the given paths are changed. Colors are "#rrggbb[aa]", (r, g, b[, a]);
a stroke is (width, color); a rounding or a size is a float; a font is (size, family) or a size.

**Args**

- `theme` (str): "light" or "dark"
- `tokens` (dict): path to value

Returns the list of the paths that are not known (nothing is raised for them).

**Example:**

```python
ctx.set_style_tokens("dark", {"widgets.hovered.weak_fill": "#3a3a4aff", "widgets.inactive.rounding": 8})
```


## `Context.reset_style()`

Restores the default style of both themes, and forgets the tokens of the tabs (`tabs.*`, `pill_tabs.*`) of the grids (`grid.*`) of the popups (`popups.*`) and of the layout (`layout.*`).


## `Context.set_layout(**kwargs)`

Sets the layout rules of the window: a few values given once, that the regions of the window
(the sidebar, the top bar, the bottom bar, the central area and the dock) take and can override.
Replaces the rules given before. The same dict can be given to run_native as layout.

**Keyword args**

- `gap` (float): space around the window and between two boxes. Default is 8
- `rounding` (float): corner radius of the boxes. Default is 12
- `outline` (tuple[float, color]): outline of the boxes and of the panes of the dock. Default is a
- thin translucent grey
- `sheet` (color): the background inside the boxes (the rounded rectangles). Default is the
- panel color of the theme
- `opacity` (float): from 0 to 1, multiplies the opacity of the sheet. With a backdrop or a
- transparent window what is behind shows through. Default is 1
- `background` (color): global only. What is seen between the boxes, in the gaps and around them.
- Default is the panel color of the theme; (0, 0, 0, 0) shows the backdrop of the window
- `inner_margin` (float): space between the outline of a box and its content. Default is 12 for
- the sidebar and 8 for the others
- sidebar, topbar, bottombar, central, dock (dict): the same values (but background) for one
- region only

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


## `Context.set_glass_style(tint=None, accent=None, mode='partial', opacity=1.0, panels=None, windows=None, content=None, sidebar=None, bars=None, central=None, dock=None, background=None)`

Applies a modern style looking like frosted glass, for both themes: translucent surfaces,
the Inter font with semibold titles, and an accent color for the selection, links, active
tabs and plots. With the system setting "Reduce transparency" (macOS), the surfaces are opaque.

Works best with a backdrop effect: run_native(..., backdrop=Backdrop.MICA).
It can be customized afterwards with set_style. reset_style removes it.

**Args**

- `tint` (color): color of the glass. Default is white on light theme and smoked on dark theme
- `accent` (color): color of the selection, links, active tabs and plots. Default is blue
- `mode` (str): "partial": the panels and menus are glass, the content is an almost opaque sheet, which is easier to read. "full": every surface is glass. Default is "partial"
- `opacity` (float): multiplies the opacity of every surface: below 1 more transparent, above 1 more opaque. Default is 1, the levels of the style
- `panels` (float): the opacity of the panels, the sidebars and the bars, from 0 (clear) to 1 (opaque), instead of the level of the style
- `windows` (float): the opacity of the windows, menus and popups, from 0 to 1
- `content` (float): the opacity of the sheet under the content in the partial mode, from 0 to 1
- `sidebar` (float): the opacity of the left and right panels, instead of panels
- `bars` (float): the opacity of the top and bottom panels, instead of panels
- `central` (float): the opacity of the central area (what is drawn after the panels), in both modes, instead of content. A DockPane(..., central=True) takes it too
- `dock` (float): the opacity of the panes of the docks. Given, each pane is filled on its own and the central area under the dock is left clear, so the dock and the central pane can differ. Default: the panes are not filled, the central area is seen through them
- `background` (float): the opacity of the glass under the whole window, seen in the gaps between the panels and the panes. Default: nothing, the backdrop is seen there

The surfaces stay opaque with the system setting "Reduce transparency". Call it again with
other levels to change them, e.g. from a slider.

**Example:**

```python
def update_func(ctx):
    ctx.set_glass_style(accent="#8b5cf6")
    heading("Glass")

# more transparent everywhere, and clear panels
ctx.set_glass_style(mode="full", opacity=0.6, panels=0.15)

# one level per part of the window
ctx.set_glass_style(mode="full", sidebar=0.2, bars=0.5, central=0.85, dock=0.6, background=0.1)

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


## `Context.set_backdrop_blur(radius)`

Blurs what is behind the window, by a radius in points: 0 is no blur. macOS only (it does
nothing elsewhere), with a transparent window: a backdrop or transparent=True in run_native.

It adds to the blur of the backdrop: Backdrop.LIQUID_GLASS_CLEAR is barely blurred, so the
radius sets the blur alone. The blur is rounded like the corners of the window. It uses a
private layer of macOS (the one of its glass views), looked up when called: on a system
without it, nothing happens.

**Args**

- `radius` (float): from 0 (sharp) to about 64 (very blurred)

**Example:**

```python
ctx.set_backdrop_blur(20)
run_native("Glass", update_func, backdrop=Backdrop.LIQUID_GLASS_CLEAR)
```


## `Context.set_backdrop_image(source=None, luminance=None, saturation=None, blur=0.0, contrast=None, scrim=None, opacity=1.0, fit='cover')`

Draws a picture under the whole window, as its backdrop, with a luminance, a saturation, a
blur and an opacity. It is seen where nothing is drawn over it: with a transparent window and
layout rules or a glass style that leave the background and the sheets clear or translucent,
for example `set_layout(sheet=(0, 0, 0, 120))` or `set_glass_style(mode="full")`.

The file is read by a thread and scaled down to 1920 pixels on its longest side, the window
does not wait for it. Call it again with other settings to change the filters: the picture
is read once, the filters are applied again to the copy (call it at every frame from a
slider if you like). `None` removes the picture.

**Args**

- `source` (str | None): a path or a `file://` uri (PNG, JPEG, GIF, WebP, BMP...), None to remove it

**Keyword args**

- `luminance` (float | str | tuple): the brightness, 1.0 as it is, 0 black, up to 4. A pair
- `(light, dark)` gives one value for the light theme and one for the dark theme, which is
- followed when the theme changes. `"auto"` (alone or in the pair) measures the picture and
- brings it to a brightness that suits the theme: lifted under the light theme, toned down
- under the dark one. Default is 1.0
- `saturation` (float | tuple): the strength of the colors, 1.0 as they are, 0 gray, up to 4.
- A number or a pair `(light, dark)` like the luminance. Default is 1.0
- `blur` (float): the radius of a gaussian blur in pixels, 0 is sharp. Default is 0
- `contrast` (float | tuple): 1.0 as it is; less flattens the picture toward its mean
- brightness, which keeps the text readable over a picture with bright and dark parts. A
- number or a pair `(light, dark)`, from 0 to 2. Default is 1.0
- `scrim` (float | tuple): a veil over the picture, from 0 (none) to 0.95: black under the dark
- theme, white under the light one, which keeps the text readable without spoiling the
- picture. A number, or a pair `(light, dark)`. Default is 0
- `opacity` (float): from 0 (not seen, the backdrop effect shows) to 1. Default is 1.0
- `fit` (str): "cover" fills the window and crops, "contain" shows all of it, "stretch" deforms it,
- "center" keeps its size. Default is "cover"

**Example:**

```python
ctx.set_backdrop_image("wallpaper.jpg", luminance=(1.0, 0.5), saturation=0.8, blur=12)
ctx.set_backdrop_image("wallpaper.jpg", luminance="auto", scrim=(0.1, 0.3))
ctx.set_backdrop_image(None)  # removed
```


## `Context.backdrop_image_error()`

Why the picture of `set_backdrop_image` could not be read, or None.


## `Context.set_central_margin(margin)`

Sets the inner margin of the central panel: the space between the panels and what is drawn
after them. Default is the 8 points of egui; 0 lets the content (a Dock for example) reach
the panels, and its own margins decide the gaps. Not used with layout rules, which lay out
the central area themselves.

**Args**

- `margin` (float | tuple[float, float, float, float]): the four sides, or (left, top, right, bottom)

**Example:**

```python
ctx.set_central_margin(0)
frame(lambda: dock_area(dock), outer_margin=6)  # the same gap on every side
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
