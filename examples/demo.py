import math

from pyegui import *

# navigation

PAGES = ["Panels", "Windows & modals", "Menus & tooltips", "Grid", "Tables", "Tree view", "Dock", "Plots"]
page = Int(0)
show_inspector = Bool(True)
status = Str("Ready")

# windows & modals

window_open = Bool(False)
fixed_window_open = Bool(False)
confirm_open = Bool(False)
deleted = Int(0)

# grid

name = Str("Ferris")
age = Int(8)
email = Str("")
subscribed = Bool(True)

# tables

people = [
  ["Alice", "32", "Paris"],
  ["Bob", "27", "Lyon"],
  ["Chloé", "41", "Nantes"],
  ["David", "35", "Lille"],
] * 25
selected_person = Int(-1)
tasks = ["Write docs", "Fix bugs", "Release 0.6"]

# tree view

tree = TreeView("project", multi_select=True, drag_and_drop=True)
tree_nodes = [
  TreeNode(0, "project", [
    TreeNode(1, "src", [TreeNode(2, "main.py"), TreeNode(3, "utils.py")]),
    TreeNode(4, "docs", [TreeNode(5, "index.rst")]),
    TreeNode(6, "README.md"),
  ]),
]
tree.set_open(0, True)

# dock

def note_pane(text):
  return lambda: label(text)

new_pane_count = Int(0)

def new_tab_button(tab_bar):
  # drawn after the last tab of every tab bar, the new pane goes to this tab bar
  if button_clicked("+"):
    new_pane_count.value += 1
    title = f"New {new_pane_count.value}"
    tab_bar.add_pane(DockPane(title, note_pane(f"{title} was added with the + of a tab bar"), closable=True))

dock = Dock(DockContainer("horizontal", [
  DockPane("Left", note_pane("Drag the tabs to rearrange the panes")),
  DockContainer("vertical", [
    DockPane("Top", note_pane("Drag the separators to resize")),
    DockContainer("tabs", [
      DockPane("Tab A", note_pane("Panes can be grouped in tabs")),
      DockPane("Tab B", note_pane("This one can be closed"), closable=True),
    ]),
  ]),
]), tab_bar_trailing=new_tab_button)

# plots

frequency = Float(1.0)
amplitude = Float(1.0)
bars = PlotBars([3, 5, 2, 8, 6, 4], name="sales", color=(90, 160, 220))


# tree helpers: the tree view only reports moves, the app updates its data

def all_nodes(nodes):
  for node in nodes:
    yield node
    if node.children is not None:
      yield from all_nodes(node.children)

def find_parent(nodes, node_id, parent=None):
  for node in nodes:
    if node.id == node_id:
      return parent
    if node.children is not None:
      found = find_parent(node.children, node_id, node)
      if found is not None:
        return found
  return None

def is_ancestor(ancestor_id, node_id):
  parent = find_parent(tree_nodes, node_id)
  while parent is not None:
    if parent.id == ancestor_id:
      return True
    parent = find_parent(tree_nodes, parent.id)
  return False

def move_nodes(sources, target_id, position, position_id):
  by_id = {node.id: node for node in all_nodes(tree_nodes)}
  target = by_id[target_id]

  for source_id in sources:
    if source_id == target_id or is_ancestor(source_id, target_id):
      continue
    parent = find_parent(tree_nodes, source_id)
    if parent is None:
      continue

    # children returns a copy, so the lists are modified and assigned back
    parent.children = [n for n in parent.children if n.id != source_id]

    children = target.children
    if position == "first":
      index = 0
    elif position == "last":
      index = len(children)
    else:
      index = next((i for i, n in enumerate(children) if n.id == position_id), len(children))
      if position == "after":
        index += 1
    children.insert(index, by_id[source_id])
    target.children = children


# pages

def panels_page():
  heading("Panels")
  label("This app uses a top panel (menu), a left panel (navigation), a bottom panel (status) and a right panel (inspector).")
  label("Panels must be declared at the beginning of update_func, before any other widget.")
  separator()
  checkbox(show_inspector, "Show the inspector (right panel, animated)")
  label("Panels can be resized by dragging their border.")

def windows_page():
  heading("Windows & modals")
  toggle_value(window_open, "Movable window")
  toggle_value(fixed_window_open, "Fixed window")
  separator()
  if button_clicked("Delete everything"):
    confirm_open.value = True
  label(f"Deleted {deleted.value} times")

  window("A window", window_ui, open=window_open, default_width=250, default_pos=(200, 200))
  window("Fixed window", lambda: label("I cannot be moved"), open=fixed_window_open,
         fixed_pos=(400, 300), resizable=False, collapsible=False)

  if confirm_open.value and modal("confirm", confirm_ui):
    confirm_open.value = False

def window_ui():
  label("Windows can be moved, resized and collapsed.")
  slider_float(amplitude, 0, 5, "amplitude")
  if button_clicked("Close me"):
    window_open.value = False

def confirm_ui():
  heading("Are you sure?")
  label("Press escape or click outside to cancel.")
  with Layout(LayoutType.Horizontal):
    if button_clicked("Yes, delete"):
      deleted.value += 1
      confirm_open.value = False
    if button_clicked("Cancel"):
      confirm_open.value = False

def menus_page():
  heading("Menus & tooltips")
  label("The menu bar at the top is made with top_panel, menu_bar and menu_button.")
  separator()
  tooltip("A simple tooltip", lambda: button_clicked("Hover me"))
  tooltip_ui(lambda: button_clicked("Hover me too"), rich_tooltip)
  separator()
  with Group():
    context_menu(lambda: label("Right click me"), context_menu_ui)

def rich_tooltip():
  heading("Rich tooltip")
  weak("Tooltips can contain any widget")
  progress(0.7)

def context_menu_ui():
  if button_clicked("Copy"):
    status.value = "Copied"
  if button_clicked("Paste"):
    status.value = "Pasted"
  menu_button("More", lambda: button_clicked("Nothing here"))

def grid_page():
  heading("Grid")
  label("Widgets are added left to right, end_row() starts a new row.")
  separator()

  def rows():
    label("Name")
    text_edit_singleline(name)
    end_row()
    label("Age")
    drag_int(age, 0, 150, 1)
    end_row()
    label("Email")
    text_edit_singleline(email, hint_text="ferris@rust-lang.org")
    end_row()
    label("Newsletter")
    checkbox(subscribed, "subscribed")
    end_row()

  grid("form", rows, striped=True, num_columns=2, spacing=(20, 8))

def tables_page():
  heading("Tables")
  label("table() shows texts. Only visible rows are drawn. Click on a row to select it.")
  if selected_person.value >= 0:
    strong(f"Selected: {people[selected_person.value][0]} (row {selected_person.value})")
  table(["Name", "Age", "City"], people, selected_person, id="people", max_height=200)

  separator()
  label("table_ui() draws each cell with a function.")

  def cell(row, col):
    if col == 0:
      label(tasks[row])
    elif button_clicked("Done"):
      tasks.pop(row)
      status.value = "Task done"

  table_ui(["Task", ""], len(tasks), cell, id="tasks")
  if not tasks and button_clicked("Reset tasks"):
    tasks.extend(["Write docs", "Fix bugs", "Release 0.6"])

def tree_page():
  heading("Tree view")
  label("Select with click, ctrl and shift. Navigate with arrows. Drag and drop nodes to move them.")
  separator()
  tree_view(tree, tree_nodes)

  if tree.moved:
    move_nodes(*tree.moved)
    status.value = f"Moved {tree.moved[0]}"
  if tree.activated:
    status.value = f"Activated {tree.activated}"

def dock_page():
  heading("Dock")
  with Layout(LayoutType.Horizontal):
    if button_clicked("Add a pane"):
      new_pane_count.value += 1
      title = f"New {new_pane_count.value}"
      dock.add_pane(DockPane(title, note_pane(f"{title} was added with add_pane"), closable=True))
    label(f"Panes: {', '.join(dock.titles)}")
  dock_area(dock)

def plots_page():
  heading("Plots")
  slider_float(frequency, 0.1, 5, "frequency")
  slider_float(amplitude, 0, 5, "amplitude")

  xs = [x / 20 for x in range(200)]
  wave = PlotLine([(x, amplitude.value * math.sin(frequency.value * x)) for x in xs], name="sin", fill=0)
  limit = PlotRule(amplitude.value / 2, name="half amplitude", color=(220, 80, 80))
  samples = PlotPoints([(x, amplitude.value * math.sin(frequency.value * x)) for x in xs[::20]],
                       name="samples", radius=4)

  pos = plot("waves", [wave, limit, samples], height=250, x_label="x", y_label="y")
  if pos:
    status.value = f"x: {pos[0]:.2f}  y: {pos[1]:.2f}"

  plot("bars", [bars], height=150, allow_zoom=False, allow_drag=False, allow_scroll=False)

PAGE_FUNCS = [panels_page, windows_page, menus_page, grid_page, tables_page, tree_page, dock_page, plots_page]


# app

def update_func(ctx):
  def menus():
    def file_menu():
      if button_clicked("Open a window"):
        window_open.value = True
      if button_clicked("Quit"):
        confirm_open.value = True
        page.value = PAGES.index("Windows & modals")

    def view_menu():
      checkbox(show_inspector, "Inspector")
      def theme_menu():
        if button_clicked("Light"):
          ctx.set_light_theme()
        if button_clicked("Dark"):
          ctx.set_dark_theme()
        if button_clicked("System"):
          ctx.set_system_theme()
      menu_button("Theme", theme_menu)

    def pages_menu():
      for i, title in enumerate(PAGES):
        if button_clicked(title):
          page.value = i

    menu_button("File", file_menu)
    menu_button("View", view_menu)
    menu_button("Pages", pages_menu)

  def navigation():
    heading("Demo")
    separator()
    for i, title in enumerate(PAGES):
      selectable_value(page, i, title)

  def inspector():
    heading("Inspector")
    label(f"Page: {PAGES[page.value]}")
    label(f"Selected nodes: {tree.selected}")
    label(f"Selected person: {selected_person.value}")

  # panels first, then the contents of the central panel
  top_panel("menu", lambda: menu_bar(menus))
  bottom_panel("status", lambda: small(status.value))
  left_panel("navigation", navigation, default_size=160)
  right_panel("inspector", inspector, expanded=show_inspector, default_size=200)

  PAGE_FUNCS[page.value]()

if __name__ == "__main__":
  run_native("pyegui demo", update_func, inner_width=1100, inner_height=700)
