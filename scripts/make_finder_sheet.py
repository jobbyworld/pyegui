"""Writes examples/style_sheets/finder.json: the tokens of the style of examples/finder.py (the blue accent and the
full glass, with the opacity and the levels of the app) for both themes, as the difference with the default style.

    .venv/bin/python scripts/make_finder_sheet.py
"""
import json
import os
from pyegui import run_native

OUT = os.path.join(os.path.dirname(__file__), "..", "examples", "style_sheets", "finder.json")
ACCENT, OPACITY, LEVELS = "#3b82f6", 1.2, {"central": 0.3, "dock": 0.3}  # as in finder.py


# what the glass needs on top of the plain style: contrast of the text and of the striped rows, windows and popups half transparent
CONTRAST = {
    "light": {"spacing.button_padding_radius": 0.8, "menu.button_padding_radius": 0.8, "colors.text": "#1c1c1eff", "colors.window": "#f9f9ffeb", "popups.menu_fill": "#f9f9ffe8",
              "popups.dropdown_fill": "#f9f9ffe8", "popups.tooltip_fill": "#f9f9fff2"},
    # dark: the striped rows are lighter than the glass, the windows and the popups let the backdrop through
    "dark": {"spacing.button_padding_radius": 0.8, "menu.button_padding_radius": 0.8, "colors.text": "#f2f2f7ff", "colors.striped_bg": "#ffffff14", "colors.window": "#13151deb",
             "popups.menu_fill": "#13151de6", "popups.dropdown_fill": "#13151de6", "popups.tooltip_fill": "#13151df2"},
}


# the "app" section: the look of examples/finder.py that is not a token of the framework (APP_STYLE of finder.py)
APP = {
    "edge": [1, "#7f7f7f3c"], "line": [1, "#7f7f7f46"],
    "hover_fill": "#7f7f7f1e", "card_fill": "#7f7f7f1c", "card_selected_fill": "#7f7f7f3c", "card_hover_fill": "#7f7f7f23",
    "item_rounding": 6, "card_rounding": 8, "dialog_rounding": 12, "dock_rounding": 9, "pane_rounding": 6,
    "pane_margin": 8, "unified_gap": 8, "danger": "#dc3c3c",
    "glass_opacity": OPACITY, "glass_central": LEVELS["central"], "glass_dock": LEVELS["dock"], "blur": 32,
}


def flatten(snap):
    out = {}
    for group in ("colors", "tabs", "pill_tabs", "grid", "popups"):
        for name, value in snap[group].items():
            if name != "chevron":
                out[f"{group}.{name}"] = value
    for st, d in snap["widgets"].items():
        for name, value in d.items():
            out[f"widgets.{st}.{name}"] = value
    for name, value in snap["window"].items():
        if "shadow" not in name:
            out[f"window.{name}"] = value
    return out


def update(ctx):
    ctx.reset_style()
    plain = {t: flatten(ctx.style_snapshot(t)) for t in ("light", "dark")}
    ctx.set_glass_style(accent=ACCENT, mode="full", opacity=OPACITY, **LEVELS)
    sheet = {"name": "finder", "description": "Le style de finder.py : accent bleu, verre complet", "opacity": 1.0}
    for t in ("light", "dark"):
        now = flatten(ctx.style_snapshot(t))
        sheet[t] = {p: (list(v) if isinstance(v, tuple) else v) for p, v in now.items() if v != plain[t].get(p)}
    sheet["app"] = {"light": dict(APP), "dark": dict(APP)}
    for t, tokens in CONTRAST.items():
        sheet[t].update(tokens)
    with open(OUT, "w") as f:
        json.dump(sheet, f, indent=2, ensure_ascii=False)
    print({t: len(sheet[t]) for t in ("light", "dark")})
    os._exit(0)


run_native("sheet", update, inner_width=200, inner_height=100)
