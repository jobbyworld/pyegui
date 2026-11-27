"""Shows a style sheet on real controls: `python style_sheets_demo.py <sheet> [light|dark]`.

A sheet of `examples/style_sheets/<sheet>.json` is the same file the style editor (`style_doc.py`) saves and loads:
the tokens of the light and of the dark theme. It is given to the app with `Context.set_style_tokens`.
"""
import json
import os
import sys

from pyegui import *

HERE = os.path.dirname(os.path.abspath(__file__))
NAME = sys.argv[1] if len(sys.argv) > 1 else "papier"
MODE = sys.argv[2] if len(sys.argv) > 2 else "light"

with open(os.path.join(HERE, "style_sheets", NAME + ".json")) as f:
    SHEET = json.load(f)

ROOT = os.path.dirname(HERE)
tree = FileTreeView("sheet-tree", ROOT, show_root=True)
tree.set_open(os.path.join(ROOT, "core"), True)
tabs = TabView(["Aperçu", "Données", "Réglages"])
name, volume, option, notes = Str("Joachim"), Float(0.6), Bool(True), Str("Une note sur\nplusieurs lignes")
state = {"first": True, "clicks": 0}


def overview():
    def row():
        if button_clicked("Bouton"):
            state["clicks"] += 1
        checkbox(option, "Option")
        label(f"{state['clicks']} clics")
        hyperlink_to("un lien", "https://github.com")

    horizontal(row)
    horizontal(lambda: (label("Nom"), text_edit_singleline(name), slider_float(volume, 0.0, 1.0, "Volume")))
    text_edit_multiline(notes)
    weak("Arbre de fichiers")
    file_tree_view(tree, grid=True, columns=["modified", "size"], max_height=260)


def update_func(ctx):
    if state["first"]:
        state["first"] = False
        ctx.reset_style()
        for theme in ("light", "dark"):
            ctx.set_style_tokens(theme, {p: tuple(v) if isinstance(v, list) else v for p, v in SHEET[theme].items()})
        ctx.set_dark_theme() if MODE == "dark" else ctx.set_light_theme()
    heading(f"{SHEET['name']} ({MODE})")
    weak(SHEET["description"])
    separator()
    tab_view(tabs, lambda i: overview() if i == 0 else label(tabs.titles[i]))


run_native("Feuilles de style", update_func, inner_width=760, inner_height=640)
