import math

from pyegui import *

# state

show_tree = Bool(True)
about_open = Bool(False)
quit_confirm = Bool(False)
selected_row = Int(-1)
name = Str("Ferris")
age = Int(8)

tree = TreeView("files", multi_select=True)
nodes = [
  TreeNode(0, "src", [TreeNode(1, "main.py"), TreeNode(2, "utils.py")]),
  TreeNode(3, "docs", [TreeNode(4, "index.rst")]),
  TreeNode(5, "README.md"),
]
names = {1: "main.py", 2: "utils.py", 4: "index.rst", 5: "README.md"}

sin = PlotLine([(x / 10, math.sin(x / 10)) for x in range(100)], name="sin")
cos = PlotLine([(x / 10, math.cos(x / 10)) for x in range(100)], name="cos")
measures = PlotPoints([(1, 0.5), (3, -0.2), (6, 0.8)], name="measures", radius=4)
threshold = PlotRule(0.5, name="threshold", color=(200, 60, 60))

people = [["Alice", "32", "Paris"], ["Bob", "27", "Lyon"], ["Chloé", "41", "Nantes"]] * 50

# panes

def editor_ui(file_name):
  def ui():
    heading(file_name)
    code(f"# contents of {file_name}")
  return ui

def plot_ui():
  pos = plot("waves", [sin, cos, measures, threshold], x_label="x", y_label="y")
  if pos:
    small(f"x: {pos[0]:.2f} y: {pos[1]:.2f}")

def people_table_ui():
  table(["Name", "Age", "City"], people, selected_row)

def form_ui():
  def rows():
    label("Name")
    text_edit_singleline(name)
    end_row()
    label("Age")
    drag_int(age, 0, 150, 1)
    end_row()

  grid("form", rows, striped=True)
  tooltip("Shows a greeting", lambda: button_clicked("Hover me"))
  context_menu(lambda: label("Right click me"), lambda: button_clicked("Hello"))

dock = Dock(DockContainer("tabs", [
  DockPane("Plot", plot_ui),
  DockPane("Table", people_table_ui),
  DockPane("Form", form_ui),
]))

# app

def menus():
  def file_menu():
    if button_clicked("About"):
      about_open.value = True
    if button_clicked("Quit"):
      quit_confirm.value = True

  def view_menu():
    checkbox(show_tree, "Files")

  menu_button("File", file_menu)
  menu_button("View", view_menu)

def files_ui():
  heading("Files")
  tree_view(tree, nodes)

  for node_id in tree.activated:
    if node_id in names and names[node_id] not in dock.titles:
      dock.add_pane(DockPane(names[node_id], editor_ui(names[node_id]), closable=True))

def quit_ui():
  heading("Quit?")
  with Layout(LayoutType.Horizontal):
    if button_clicked("Yes"):
      exit()
    if button_clicked("No"):
      quit_confirm.value = False

def update_func(ctx):
  top_panel("menu", lambda: menu_bar(menus))
  bottom_panel("status", lambda: small(f"selected: {tree.selected}"))
  left_panel("files", files_ui, expanded=show_tree, default_size=180)

  dock_area(dock)

  window("About", lambda: label("pyegui workspace example"), open=about_open)

  if quit_confirm.value and modal("quit", quit_ui):
    quit_confirm.value = False

if __name__ == "__main__":
  run_native("workspace", update_func)
