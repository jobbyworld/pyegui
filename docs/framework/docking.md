# Docking tabs

A `Dock` holds panes shown as tabs that the user can resize, rearrange by dragging and close. Create it once, add the panes, then show it with `dock_area` every frame.

## `Dock`


Dock stores a layout of panes that the user can resize, rearrange by dragging tabs, and close.

**Args**

- `root` (DockPane | DockContainer): initial layout
- `id` (str): unique id of the dock. Default is "dock"

**Keyword Args**

- `glass` (bool): sizes matching ctx.set_glass_style(): taller tab bar, more space between the tabs, wider see-through gaps between the panes. The other options override it
- `tab_bar_height` (float): height of the tab bar. Default is 24
- `tab_bar_color` (color): background of the tab bar. Default is transparent
- `hide_single_tab_bar` (bool): hide the tab bar of tabs holding a single pane. Default is False
- `tab_style` (str): "classic" tabs joined to their pane, or "pill" tabs rounded on every corner, the active one filled with the selection color. Default is "classic"
- `active_tab_fill` (color): background of the active tab. Default is transparent, so the active tab has the same background as its pane
- `inactive_tab_fill` (color): background of the other tabs. Default is a dark tint setting them apart
- `hovered_tab_fill` (color): background of an inactive tab under the mouse. Default is a lighter tint
- `tab_spacing` (float): space between two tabs. Default is 2, 4 with glass

The top corners of the tabs are rounded like the widgets of the theme, see the rounding and
widget_rounding options of ctx.set_style().

    active_tab_text_color (color): title color of the active tab

    inactive_tab_text_color (color): title color of the other tabs. Default is dimmed

    active_tab_stroke (tuple[float, color]): outline of the active tab, open at the bottom. Default is a
        thin translucent line, dark on light theme and light on dark theme

    inactive_tab_stroke (tuple[float, color]): outline of the other tabs. Default is a fainter line

    tab_title_spacing (float): space on the left and right of the tab titles. Default is 8

    gap_width (float): gap between the panes of horizontal, vertical and grid containers. Default is 1

    resize_stroke (tuple[float, color]): line drawn in the gaps between panes

    drag_preview_color (color): fill of the area where a dragged tab will be dropped

    drag_preview_stroke (tuple[float, color]): outline of the area where a dragged tab will be dropped

    pane_stroke (tuple[float, color]): outline of the panes, rounded like the tabs and open under the
        active tab so the tab and its pane form one shape. Default is the outline of the active tab

    pane_margin (float): space between the outline of the panes and their contents. Default is 6

Colors are (r, g, b), (r, g, b, a) or "#rrggbb[aa]".

The tab bars can hold widgets, drawn by functions taking a DockTabBar. They are called for every
tab bar of the dock:

    tab_bar_left (Callable[[DockTabBar], None]): widgets on the left of the tabs, e.g. a menu

    tab_bar_trailing (Callable[[DockTabBar], None]): widgets right after the last tab, e.g. a "+" button

    tab_bar_right (Callable[[DockTabBar], None]): widgets on the right of the tab bar, added right to left

    tab_menu (Callable[[DockTabBar, str], None]): the content of the menu opened by a right click on a
    tab, given the tab bar and the title of the tab, e.g. `lambda bar, title: dock_menu(bar, panes)`

**Usage:**

```python
dock = Dock(DockContainer("horizontal", [
    DockPane("Files", files_ui),
    DockContainer("tabs", [DockPane("Editor", editor_ui), DockPane("Preview", preview_ui)]),
]))

def update_func(ctx):
    dock_area(dock)

# a "+" button opening a tab in the tab bar it belongs to
def new_tab_button(tab_bar):
    if button_clicked("+"):
        tab_bar.add_pane(DockPane("New", new_ui, closable=True))

dock = Dock(root, tab_bar_trailing=new_tab_button)

# glass sizes, with pink inactive tabs
dock = Dock(root, glass=True, inactive_tab_fill="#ec489933", tab_bar_height=30)
```


- `Dock.active_title` (attribute): Title of the active pane: the visible pane of the first tabs, or None
- `Dock.add_pane(pane)` (method): Adds a pane to the first tabs of the dock, or to the root container, and makes it the active tab
- `Dock.load(layout, panes=None)` (method): Restores a layout given by save(). Returns True if the layout was applied, and False,

  leaving the dock as it is, if it is empty, damaged or from an unrelated dock: an old
  saved state never raises.

  The panes are found by title among the ones of the dock. A pane saved in the layout but
  that does not exist any more is left out. A pane that was closed when the layout was saved
  is not in it, so it is closed again; to be able to open it, give it in panes: the layout
  can then also bring back a pane that is not in the dock yet.

  **Args**

  - `layout` (dict): what save() returned
  - `panes` (list[DockPane]): more panes the layout can use, for the ones that are not in the dock now

  **Usage:**

  ```python
  saved = ctx.load_state() or {}
  if "dock" in saved:
      dock.load(saved["dock"], panes=all_panes)
  ```

- `Dock.remove_pane(title)` (method): Closes the first pane with this title, whether or not it is `closable` (that only decides

  whether its tab has a close button). Returns False if there is no such pane. The pane object
  is yours: add_pane() brings it back.

  **Usage:**

  ```python
  if button_clicked("Close the console"):
      dock.remove_pane("Console")
  ```

- `Dock.reset_style()` (method): Goes back to the default look of the dock
- `Dock.save()` (method): Returns the layout of the dock as plain Python values (dicts, lists, strings, numbers): which

  panes are open, how they are grouped in tabs and splits, the active tab of each group and the
  size of each part. It is made to be given to ctx.save_state() and later to load(). Returns
  None if called inside a pane of the same dock.

  **Usage:**

  ```python
  ctx.save_state({"dock": dock.save()})
  ```

- `Dock.set_active(title)` (method): Makes the first pane with this title the active tab. Returns False if there is no such pane


  **Usage:**

  ```python
  if selectable_label_clicked(dock.active_title == "Tasks", "Tasks"):
      dock.set_active("Tasks")
  ```

- `Dock.set_style(**kwargs)` (method): Changes the look of the dock. Takes the same keyword arguments as Dock, the options

  that are not given are kept. Call reset_style() to go back to the defaults.

  **Usage:**

  ```python
  dock.set_style(tab_bar_height=30, active_tab_fill=(59, 130, 246, 120))
  ```

- `Dock.set_tab_bar(**kwargs)` (method): Changes the functions drawing in the tab bars, see Dock. None removes a function, the

  functions that are not given are kept.

  **Usage:**

  ```python
  dock.set_tab_bar(trailing=new_tab_button, right=None)
  ```

- `Dock.titles` (attribute): Titles of the panes currently in the dock, in display order

## `DockPane`


A pane of a Dock, shown as a tab.

**Args**

- `title` (str): title of the tab
- `update_func` (Callable[[], None]): draws the contents of the pane
- `closable` (bool): show a close button on the tab. Default is False
- `hide_tab` (bool): hide the tab bar while the pane is alone in its tabs, like hide_single_tab_bar but for this pane only. The tab is back as soon as another pane joins it. Default is False
- `sheet` (color): with layout rules, the background of the pane and of its tab, instead of the sheet of the dock. Its opacity is multiplied by the opacity of the dock. Default is the sheet of the dock
- `central` (bool): the pane is the main content of the window: with set_glass_style(dock=...), it is filled with the level of the central area, not the one of the dock. Default is False
- `label` (str | Callable[[], str]): the text shown in the tab, when it is not the title. A function is called at each frame, so the tab can follow what the pane shows (the folder of a tab of files). The title stays the identity of the pane: for `set_active`, `remove_pane`, `save` and `load`. Default is the title

**Usage:**

```python
DockPane("Editor", editor_ui, closable=True)
DockPane("tab 2", files_ui, label=lambda: current_folder().name)  # the tab shows the folder
DockPane("Notes", notes_ui, sheet=(59, 130, 246, 200))  # a blue sheet, with layout rules
```


- `DockPane.central` (attribute)
- `DockPane.closable` (attribute)
- `DockPane.hide_tab` (attribute)
- `DockPane.label` (attribute)
- `DockPane.title` (attribute)
- `DockPane.update_func` (attribute)

## `DockContainer`


A container of a Dock, holding DockPanes and other DockContainers.

**Args**

- `kind` (str): "tabs", "horizontal", "vertical" or "grid"
- `children` (list[DockPane | DockContainer]): contents of the container

**Usage:**

```python
DockContainer("horizontal", [DockPane("Files", files_ui), DockPane("Editor", editor_ui)])
```


- `DockContainer.children` (attribute)
- `DockContainer.kind` (attribute)

## `DockTabBar`


A tab bar of a Dock, given to the tab_bar_left, tab_bar_trailing and tab_bar_right callbacks.

**Usage:**

```python
def new_tab_button(tab_bar):
    if button_clicked("+"):
        tab_bar.add_pane(DockPane(f"Untitled {len(tab_bar.titles) + 1}", editor_ui, closable=True))

dock = Dock(root, tab_bar_trailing=new_tab_button)

# a menu listing the tabs
def tabs_menu(tab_bar):
    def items():
        for title in tab_bar.titles:
            if button_clicked(title):
                tab_bar.set_active(title)
        separator()
        if button_clicked("Close the others"):
            for title in tab_bar.titles:
                if title != tab_bar.active_title:
                    tab_bar.close(title)
    menu_button("☰", items)

dock = Dock(root, tab_bar_left=tabs_menu)
```

The changes are applied once the dock is drawn, so they show up on the next frame.


- `DockTabBar.active_title` (attribute): Title of the active tab, or None
- `DockTabBar.add_pane(pane)` (method): Adds a pane to these tabs and makes it the active tab
- `DockTabBar.close(title)` (method): Closes the first tab with this title. Only the panes created with closable=True are closed
- `DockTabBar.dock_titles` (attribute): Titles of the panes of the whole dock, not only of these tabs, in display order
- `DockTabBar.remove_pane(title)` (method): Closes the first pane of the dock with this title, in these tabs or in others, whether or not

  it is `closable`

- `DockTabBar.set_active(title)` (method): Makes the first tab with this title the active tab
- `DockTabBar.titles` (attribute): Titles of the tabs, in display order

## `dock_area(dock)`

Shows a Dock, filling the available space.

**Example:**

```python
dock = Dock(DockContainer("tabs", [DockPane("A", lambda: label("a")), DockPane("B", lambda: label("b"))]))

def update_func(ctx):
    dock_area(dock)
```


## `dock_menu(dock, panes, **kwargs)`

The content of a menu that manages the panes of a dock: a check box for each pane that tells
whether it is open, to show it or close it, and a button that gives back the layout the app
started with. Call it in a menu (`menu_button`, `menu_image_button`), a context menu or a popup,
not in a pane of the same dock. In the `tab_menu` of the dock (the menu of a right click on a
tab) give it the tab bar the function receives instead of the dock.

**Args**

- `dock` (Dock | DockTabBar): the dock to manage, or the tab bar given to a `tab_menu` function
- `panes` (list[DockPane]): the panes the menu manages: the ones that can be shown or closed, open or not. Another pane of the dock, the central one for example, is not in the menu

**Keyword args**

- `layout` (dict): a layout given by `dock.save()`, for the button that goes back to it. Without
- it there is no button. Not for a tab bar: the dock cannot be rebuilt while it is drawn, add
- your own button to the menu and call `dock.load()` for it
- `reset_text` (str): the text of that button. Default is "Reset layout"
- `show` (Callable[[str], None]): called with the title of a pane the user wants to show, instead
- of adding it to the first tabs of the dock (to the tabs of the tab bar for a tab bar): for an
- app that puts each pane in its own place. The dock is changed by the next frame at the latest,
- as for add_pane

Returns True during the frame a pane was shown or closed or the layout was restored.

**Example:**

```python
panes = [DockPane("Files", files_ui, closable=True), DockPane("Console", console_ui, closable=True)]
dock = Dock(DockContainer("horizontal", panes), tab_menu=lambda bar, title: dock_menu(bar, panes))
default_layout = dock.save()

def update_func(ctx):
    menu_button("Panes", lambda: dock_menu(dock, panes, layout=default_layout))
    dock_area(dock)
```


## `TabView`


The tabs of a `tab_view`: their titles and the active one. A state object: create it once, outside the update
function; `tab_view(tabs, content)` draws it.

**Args**

- `titles` (list[str]): the titles of the tabs. Default is none
- `active` (int): the index of the active tab. Default is 0

**Example:**

```python
tabs = TabView(["Files", "Editor"])

def update_func(ctx):
    tab_view(tabs, lambda i: label(f"content of {tabs.titles[i]}"), closable=True, add_button=True)
    if tabs.take_added():
        tabs.add(f"Tab {len(tabs.titles) + 1}")
```


- `TabView.active` (attribute): The index of the active tab (read and write)
- `TabView.active_title` (attribute): The title of the active tab, or None when there is no tab
- `TabView.add(title)` (method): Adds a tab at the end and makes it the active one
- `TabView.remove(title)` (method): Removes the first tab with this title. The active tab stays the same tab, or the one before it if it was removed
- `TabView.set_active_title(title)` (method): Makes the tab with this title the active one
- `TabView.take_added()` (method): True once after the "+" button was clicked. Add the tab with `add`
- `TabView.take_closed()` (method): The title of the tab closed with its cross (or the middle button) since the last call, or None. The tab is

  already removed

- `TabView.titles` (attribute): The titles of the tabs, in order

## `tab_view(tabs, update_fun, **kwargs)`

A bar of tabs with, under it, the pane with the content of the active tab.

The look is the one of the tabs of a `Dock`: both read the same style tokens (`tabs.*` for the classic tabs,
`pill_tabs.*` for the pill ones, see `Context.set_style_tokens`), and the options of the call win over them.

**Args**

- `tabs` (TabView): the titles and the active tab
- `update_fun` (callable): draws the content of the active tab; it is given the index of the tab

**Keyword args**

- `tab_style` (str): "classic" tabs joined to their pane, or "pill" tabs rounded on every corner. Default is classic
- `closable` (bool): a cross on every tab, and the middle button closes it (`TabView.take_closed`)
- `add_button` (bool): a "+" after the last tab (`TabView.take_added`)
- `margin` (float): space between the pane and its content. Default is 8
- tab_bar_height (float), tab_spacing (float), tab_title_spacing (float): sizes of the bar
- tab_bar_color, active_tab_fill, inactive_tab_fill, hovered_tab_fill (color): backgrounds
- active_tab_text_color, inactive_tab_text_color (color): colors of the titles
- active_tab_stroke, inactive_tab_stroke (tuple[float, color]): outlines

**Example:**

```python
tabs = TabView(["A", "B"])

def update_func(ctx):
    tab_view(tabs, lambda i: label("content " + tabs.titles[i]), tab_style="pill", closable=True)
```
