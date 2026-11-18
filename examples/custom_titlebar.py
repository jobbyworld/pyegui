"""A window with its own title bar: the buttons at the left like macOS, the bar moves the window.

custom_titlebar=True removes the title bar of the system (on macOS the buttons of the system stay).
A transparent window with a backdrop and rounded boxes looks the best.
"""
import pyegui


def bar():
    pyegui.window_controls()
    pyegui.window_drag_area(title="Custom title bar")


def update(ctx):
    pyegui.topbar_panel(lambda: pyegui.horizontal(bar))
    pyegui.heading("Drag the bar to move the window")
    pyegui.label("Double click the bar to maximize, drag the edges to resize.")


pyegui.run_native(
    "Custom title bar",
    update,
    inner_width=720,
    inner_height=420,
    min_inner_width=360,
    min_inner_height=200,
    custom_titlebar=True,
    layout=dict(gap=8, rounding=10),
)
