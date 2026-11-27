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
    "light": {"spacing.button_padding_radius": 0.8, "menu.button_padding_radius": 0.8, "colors.text": "#1c1c1eff", "colors.window": "#f9f9ffd8", "popups.menu_fill": "#f9f9ffd0",
              "popups.dropdown_fill": "#f9f9ffd0", "popups.tooltip_fill": "#f9f9ffe0"},
    # dark: the striped rows are lighter than the glass, the windows and the popups let the backdrop through
    "dark": {"spacing.button_padding_radius": 0.8, "menu.button_padding_radius": 0.8, "colors.text": "#f2f2f7ff", "colors.striped_bg": "#ffffff14", "colors.window": "#13151dc8",
             "popups.menu_fill": "#13151dc0", "popups.dropdown_fill": "#13151dc0", "popups.tooltip_fill": "#13151dd8"},
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
    for t, tokens in CONTRAST.items():
        sheet[t].update(tokens)
    with open(OUT, "w") as f:
        json.dump(sheet, f, indent=2, ensure_ascii=False)
    print({t: len(sheet[t]) for t in ("light", "dark")})
    os._exit(0)


run_native("sheet", update, inner_width=200, inner_height=100)
