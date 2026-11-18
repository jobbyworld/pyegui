"""Several windows in one app: `Viewport(title, update_func)` is a second native window of the same
process, so the system groups them under one icon of the Dock (Mission Control shows them all).
The windows share the values of the app: the text typed in one is shown in the other."""

import pyegui
from pyegui import Str, Viewport, button_clicked, heading, label, text_edit_singleline, horizontal

shared = Str("Hello")
counter = [0]


def inspector(ctx):
    heading("Inspector")
    label("Typed in any window:")
    text_edit_singleline(shared)
    if button_clicked("Close"):
        windows[0].close()


def another(ctx):
    heading("Another window")
    label(f"Shared text: {shared.value}")


windows = [Viewport("Inspector", inspector, inner_width=360, inner_height=200)]


def update(ctx):
    heading("Main window")
    text_edit_singleline(shared)

    def row():
        if button_clicked("Inspector (toggle)"):
            windows[0].toggle()
        if button_clicked("New window"):
            counter[0] += 1
            window = Viewport(f"Window {counter[0]}", another, inner_width=300, inner_height=160)
            window.open()
            windows.append(window)

    horizontal(row)
    label(f"{sum(w.is_open for w in windows)} secondary windows open")


pyegui.run_native("Windows", update, inner_width=520, inner_height=240)
