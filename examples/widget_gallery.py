import datetime
import math
from pyegui import *

RED = 0                       
GREEN = 1
BLUE = 2

text_singline = Str("")
text_multiline = Str("")
checkbox_value = Bool(False)
color = Int(RED)
color_rgb = RGB(69, 69, 69)
date = Date(datetime.datetime.now())
visible = Bool(True)
is_interactive = Bool(True)
opacity_value = Float(1)
color_rgba = RGBA(0.23, 0.51, 0.96, 0.8)
bold = Bool(False)
angle = Float(0.0)
fruits = ["apple", "banana", "cherry"]
basket = []

def update_func(ctx):
  scroll_area_vertical(gallery)


def gallery():
  label("Welcome to the widget gallery!")

  hyperlink_to("pyegui on GitHub", "https://github.com/GachiLord/pyegui")

  text_edit_singleline(text_singline, hint_text="hint text")
  text_edit_multiline(text_multiline, hint_text="hint text")
  code("print(69)")
  code_editor(text_multiline)

  if button_clicked("clear text"):
    text_singline.value = ""

  if link_clicked("I'm a fake link"):
    text_singline.value = "new text"

  checkbox(checkbox_value, "check me")

  with Layout(LayoutType.Horizontal):
    radio_value(color, RED, "red")
    radio_value(color, GREEN, "green")
    radio_value(color, BLUE, "blue")

  with Layout(LayoutType.Horizontal):
    selectable_value(color, RED, "red")
    selectable_value(color, GREEN, "green")
    selectable_value(color, BLUE, "blue")

  combo_box(color, [RED, GREEN, BLUE], ["red", "green", "blue"], "Choose your fate") 

  slider_int(color, 0, 2, "slide me")

  drag_int(color, 0, 2, 1)

  progress(1 / (color.value + 1))

  color_edit_button_rgb(color_rgb)

  image("https://github.githubassets.com/favicons/favicon.svg", max_width=240, max_height=320)

  if image_and_text_clicked("https://github.githubassets.com/favicons/favicon.svg", "button with image"):
    print("clicked on image with text")

  date_picker_button(date) 

  collapsing("collapsed", lambda: heading("I'm so collapsed now"))

  separator()

  heading("More widgets")

  with Layout(LayoutType.Horizontal):
    colored_label((220, 50, 50), "red text")
    colored_label("#22c55e", "green text")
    colored_label(color_rgba.to_tuple(), "text in the color below")

  with Layout(LayoutType.Horizontal):
    color_edit_button_rgba(color_rgba)
    label(repr(color_rgba))

  with Layout(LayoutType.Horizontal):
    if selectable_label_clicked(bold.value, "selectable label"):
      bold.value = not bold.value
    label("selected" if bold.value else "not selected")

  with Layout(LayoutType.Horizontal):
    drag_angle(angle)
    label(f"{math.degrees(angle.value):.0f} degrees")

  def column(i):
    strong(f"Column {i + 1}")
    label("Same width")

  columns(3, column)

  label("Drag the fruits into the basket:")
  with Layout(LayoutType.Horizontal):
    for fruit in fruits:
      dnd_drag_source(f"fruit {fruit}", fruit, lambda fruit=fruit: label(fruit))
  dropped = dnd_drop_zone(lambda: label(f"Basket: {', '.join(basket) or 'empty'}"))
  if dropped is not None:
    basket.append(dropped)

  separator()

  heading("Configure block in the frame")

  with Group():
    set_opacity(opacity_value.value)
    if not visible.value:
      set_invisible()
    if not is_interactive.value:
      disable()

    heading("This block may be hidden")
    button_clicked("Try to click me")

  with Layout(LayoutType.Horizontal):
    checkbox(visible, "Visible")
    checkbox(is_interactive, "Interactive")
    slider_float(opacity_value, 0.0, 1.0, "Opacity")


if __name__ == "__main__":
  run_native("Widget Gallery", update_func, inner_height=900, inner_width=500)

