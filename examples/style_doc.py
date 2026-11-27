"""Style documentation of the widgets, and an editor of the style sheet.

For each widget, the elements that are drawn (background, border, text, rounding, font) in each state
(inactive, hovered, active, open, disabled) and in both themes (light and dark), side by side, with the
token of the style that gives each value (`widgets.hovered.weak_fill`, `colors.panel`...).

The "Tokens" view lists every token with its value in both themes, the widgets that use it, and the values
that are the same under different names (what to unify). A color can be edited there; the whole app, and
the specimens, follow at once. The transparency slider multiplies the alpha of every background.
The look can be saved to / loaded from a JSON file: the first draft of a style sheet whose values are
inherited through the layouts.

    .venv/bin/python examples/style_doc.py
"""

import json
import os

from pyegui import *

SHEET = os.path.join(os.path.dirname(os.path.abspath(__file__)), "style_sheet.json")
THEMES = [("light", "Clair"), ("dark", "Sombre")]

# ---------------------------------------------------------------------------------------------------------
# what each widget draws: rows of (label, fill, stroke, text, rounding) as paths in the style snapshot
# ---------------------------------------------------------------------------------------------------------


def row(label, fill=None, stroke=None, text=None, rounding=None, dim=1.0, font=None):
    return dict(label=label, fill=fill, stroke=stroke, text=text, rounding=rounding, dim=dim, font=font)


def by_state(fill, stroke="stroke", text="fg_stroke", rounding="rounding", states=None, disabled=True):
    """one row per state of an interactive widget, from the `widgets.<state>.*` tokens"""
    names = states or [("Inactif", "inactive"), ("Survol", "hovered"), ("Actif (pressé)", "active"), ("Ouvert", "open")]
    rows = [
        row(
            label,
            fill and f"widgets.{key}.{fill}",
            stroke and f"widgets.{key}.{stroke}",
            text and f"widgets.{key}.{text}",
            rounding and f"widgets.{key}.{rounding}",
        )
        for label, key in names
    ]
    if disabled:  # egui fades a disabled widget toward the background
        rows.append(row("Désactivé (fondu)", fill and f"widgets.inactive.{fill}", stroke and f"widgets.inactive.{stroke}", text and "widgets.inactive.fg_stroke", rounding and "widgets.inactive.rounding", dim=0.45))
    return rows


TEXT = "colors.text"

WIDGETS = [
    dict(
        name="Label", category="Texte", sample="Texte courant",
        functions="label, strong, weak, small, heading, monospace, colored_label",
        note="Le texte n'a pas de fond ni de bordure: il prend la couleur du texte du thème, le fort et le faible en dérivent.",
        parts=[
            ("Texte", [row("Courant", text="colors.text", font="Body"), row("Fort (strong)", text="colors.strong_text", font="Body"), row("Faible (weak)", text="colors.weak_text", font="Body"), row("Titre (heading)", text="colors.strong_text", font="Heading"), row("Petit (small)", text="colors.text", font="Small")]),
            ("Alertes", [row("Avertissement", text="colors.warn"), row("Erreur", text="colors.error")]),
        ],
    ),
    dict(
        name="Hyperlink", category="Texte", sample="un lien", functions="hyperlink, hyperlink_to, link_clicked",
        note="La couleur ne change pas au survol: le lien est souligné.",
        parts=[("Lien", [row("Normal", text="colors.hyperlink")])],
    ),
    dict(
        name="Code", category="Texte", sample="let x = 1;", functions="code, code_view, code_editor",
        note="Fond propre au code, police monospace.",
        parts=[("Bloc de code", [row("Code", fill="colors.code_bg", text="colors.text", font="Monospace")])],
    ),
    dict(
        name="Button", category="Boutons", sample="Valider", functions="button_clicked, small_button_clicked, image_button...",
        note="Le fond d'un bouton est `weak_fill`, pas `fill`. Le survol agrandit le bouton (`expansion`).",
        parts=[("Bouton", by_state("weak_fill"))],
    ),
    dict(
        name="Selectable", category="Boutons", sample="Choix", functions="selectable_label_clicked, selectable_value",
        note="Sélectionné: fond de la sélection. Sinon: sans fond, comme un bouton plat; le fond n'apparaît qu'au survol.",
        parts=[("Choix", [row("Non sélectionné", text="widgets.inactive.fg_stroke", rounding="widgets.inactive.rounding")] + [row(l, fill=f"widgets.{k}.weak_fill", stroke=f"widgets.{k}.stroke", text=f"widgets.{k}.fg_stroke", rounding=f"widgets.{k}.rounding") for l, k in [("Survol", "hovered"), ("Pressé", "active")]] + [row("Sélectionné", fill="colors.selection_bg", stroke="colors.selection_stroke", text="colors.selection_stroke", rounding="widgets.inactive.rounding")])],
    ),
    dict(
        name="Checkbox / Radio", category="Boutons", sample="✓", functions="checkbox, radio_value",
        note="La case a le fond `fill` (pas `weak_fill`); la coche prend la couleur du trait `fg_stroke`.",
        parts=[("Case", by_state("fill", states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Actif (pressé)", "active")])), ("Libellé", [row("Texte", text="widgets.noninteractive.fg_stroke")])],
    ),
    dict(
        name="Switch", category="Boutons", sample="●", functions="switch",
        note="Allumé: le fond est la couleur de la sélection.",
        parts=[("Interrupteur", [row("Éteint", fill="widgets.inactive.fill", stroke="widgets.inactive.stroke", text="widgets.inactive.fg_stroke", rounding="widgets.inactive.rounding"), row("Survol", fill="widgets.hovered.fill", stroke="widgets.hovered.stroke", text="widgets.hovered.fg_stroke", rounding="widgets.hovered.rounding"), row("Allumé", fill="colors.selection_bg", stroke="colors.selection_stroke", text="widgets.active.fg_stroke", rounding="widgets.inactive.rounding")])],
    ),
    dict(
        name="Slider / DragValue", category="Boutons", sample="42", functions="slider_int, slider_float, drag_int, drag_float",
        note="Le rail est `inactive.fill`, la partie remplie la couleur de la sélection, la poignée suit l'état. Le champ de valeur est un bouton.",
        parts=[("Rail", [row("Rail", fill="widgets.inactive.fill", rounding="widgets.inactive.rounding"), row("Partie remplie", fill="colors.selection_bg", rounding="widgets.inactive.rounding")]), ("Poignée", by_state("fill", disabled=False)), ("Champ de valeur", by_state("weak_fill", disabled=False))],
    ),
    dict(
        name="ComboBox", category="Boutons", sample="Choisir ▾", functions="combo_box, choice_combo",
        note="Fermée: comme un bouton. Ouverte: l'état `open`; la liste est un menu.",
        parts=[("Liste fermée", by_state("weak_fill", states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Ouverte", "open")]))],
    ),
    dict(
        name="ProgressBar", category="Boutons", sample="60 %", functions="progress",
        parts=[("Barre", [row("Fond", fill="colors.text_edit_bg", stroke="widgets.noninteractive.stroke", text="colors.text", rounding="widgets.inactive.rounding"), row("Partie remplie", fill="colors.selection_bg", text="colors.text", rounding="widgets.inactive.rounding")])],
    ),
    dict(
        name="TextEdit", category="Saisie", sample="texte saisi", functions="text_edit_singleline, text_edit_multiline, code_editor",
        note="Fond `text_edit_bg` (le même que les tracés et les barres de défilement). Le focus prend le trait de la sélection.",
        parts=[
            ("Champ", [row("Inactif", fill="colors.text_edit_bg", stroke="widgets.inactive.stroke", text="colors.text", rounding="widgets.inactive.rounding"), row("Survol", fill="colors.text_edit_bg", stroke="widgets.hovered.stroke", text="colors.text", rounding="widgets.hovered.rounding"), row("Focus", fill="colors.text_edit_bg", stroke="colors.selection_stroke", text="colors.text", rounding="widgets.inactive.rounding"), row("Désactivé (fondu)", fill="colors.text_edit_bg", stroke="widgets.inactive.stroke", text="colors.text", rounding="widgets.inactive.rounding", dim=0.45)]),
            ("Texte sélectionné", [row("Sélection", fill="colors.selection_bg", text="colors.text"), row("Curseur", text="colors.cursor")]),
        ],
    ),
    dict(
        name="Window / Modal", category="Conteneurs", sample="Fenêtre", functions="window, modal, settings_window",
        parts=[("Fenêtre", [row("Fenêtre", fill="colors.window", stroke="window.stroke", text="colors.text", rounding="window.rounding")])],
    ),
    dict(
        name="Menu / Popup / Tooltip", category="Conteneurs", sample="Menu", functions="menu_button, context_menu, tooltip, popup",
        note="Même fond que les fenêtres, coins `menu_rounding`, ombre `popup_shadow`. Un élément de menu est un bouton (survol/actif).",
        parts=[("Menu", [row("Menu", fill="colors.window", stroke="window.stroke", text="colors.text", rounding="window.menu_rounding")]), ("Élément de menu", by_state("weak_fill", disabled=False, states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Actif (pressé)", "active")]))],
    ),
    dict(
        name="Panel", category="Conteneurs", sample="Panneau", functions="left_panel, right_panel, top_panel, bottom_panel, sidebar_panel...",
        note="Le fond entre les boîtes est `panel`; les règles de mise en page (`set_layout`) peuvent le remplacer.",
        parts=[("Panneau", [row("Fond", fill="colors.panel", text="colors.text")])],
    ),
    dict(
        name="Group / Frame", category="Conteneurs", sample="Groupe", functions="group, frame",
        note="`group` prend le trait des widgets non interactifs et leurs coins; `frame` n'a ni fond ni trait par défaut.",
        parts=[("Groupe", [row("Groupe", stroke="widgets.noninteractive.stroke", text="colors.text", rounding="widgets.noninteractive.rounding")])],
    ),
    dict(
        name="Table / Grid", category="Données", sample="ligne", functions="table, grid(striped), tree_view",
        note="Une ligne sur deux a le fond `striped_bg`; la ligne choisie, la couleur de la sélection.",
        parts=[("Lignes", [row("Ligne", text="colors.text"), row("Ligne rayée", fill="colors.striped_bg", text="colors.text"), row("Sélectionnée", fill="colors.selection_bg", text="colors.selection_stroke")])],
    ),
    dict(
        name="Separator", category="Données", sample="", functions="separator",
        parts=[("Ligne", [row("Trait", stroke="widgets.noninteractive.stroke")])],
    ),
    dict(
        name="PropertySet", category="Widgets du framework", sample="Valeur", functions="PropertySet, property_set",
        note="Une page de propriétés comme la fenêtre Propriétés de Visual Studio. Tout vient des jetons des widgets non interactifs et de la sélection, atténués par un facteur (×0.7 = alpha multiplié).",
        parts=[
            ("Cadre", [row("Cadre", stroke="widgets.noninteractive.stroke", text="colors.text", rounding=8)]),
            ("En-tête de groupe", [row("Repos", fill="widgets.noninteractive.weak_fill*0.7", text="colors.strong_text"), row("Survol", fill="widgets.noninteractive.weak_fill", text="colors.strong_text"), row("Compteur", text="colors.weak_text")]),
            ("Ligne", [row("Repos", text="colors.text"), row("Survol", fill="widgets.hovered.weak_fill*0.5", text="colors.text"), row("Sélectionnée", fill="colors.selection_bg*0.35", text="colors.text"), row("Lecture seule", text="colors.weak_text"), row("Séparateur", stroke="widgets.noninteractive.stroke*0.6"), row("Champ qui a le focus", stroke="colors.selection_stroke", text="colors.text", rounding=3)]),
            ("Description", [row("Zone de description", fill="widgets.noninteractive.weak_fill*0.8", text="colors.text", rounding=8)]),
        ],
    ),
    dict(
        name="Table", category="Widgets du framework", sample="cellule", functions="table, table(view=...), TableView",
        note="Le titre est fort quand la colonne est triée ou survolée, faible sinon. La barre de défilement horizontale se dessine avec la couleur du texte à faible alpha.",
        parts=[
            ("En-tête", [row("Trait sous l'en-tête", stroke="widgets.noninteractive.stroke"), row("Titre (trié ou survolé)", text="colors.strong_text"), row("Titre (autre colonne)", text="colors.weak_text"), row("Filtre actif", text="colors.hyperlink"), row("Filtre inactif", text="colors.weak_text"), row("Colonne déplacée", fill="colors.text*0.08", text="colors.text")]),
            ("Lignes", [row("Normale", text="colors.text"), row("Rayée", fill="colors.striped_bg", text="colors.text"), row("Survol", fill="colors.text*0.06", text="colors.text"), row("Sélectionnée", fill="colors.selection_bg", text="colors.text")]),
            ("Barre de défilement horizontale", [row("Piste", fill="colors.text*0.08", rounding=3), row("Poignée", fill="colors.text*0.3", rounding=3), row("Poignée (survol, glissée)", fill="colors.text*0.5", rounding=3)]),
            ("Réglage des colonnes", [row("Bouton au repos", fill="widgets.inactive.weak_fill", text="colors.text", rounding=4), row("Bouton au survol", fill="widgets.hovered.fill", text="colors.text", rounding=4), row("Ligne survolée", fill="widgets.hovered.weak_fill", text="colors.text", rounding=5), row("Ligne glissée", fill="colors.selection_bg*0.35", text="colors.text", rounding=5)]),
        ],
    ),
    dict(
        name="TreeView", category="Widgets du framework", sample="dossier", functions="tree_view, TreeView, TagTreeView",
        note="Dessiné par egui_ltreeview. La sélection d'un arbre sans focus est un fond discret, pas la couleur de la sélection.",
        parts=[("Lignes", [row("Normale", text="colors.text"), row("Sélectionnée (arbre actif)", fill="colors.selection_bg", text="colors.selection_stroke", rounding="widgets.active.rounding"), row("Sélectionnée (sans focus)", fill="widgets.inactive.weak_fill*0.3", text="colors.text", rounding="widgets.active.rounding")]), ("Repères", [row("Guides d'indentation", stroke="widgets.noninteractive.stroke"), row("Marqueur de dépôt", fill="colors.selection_bg*0.6", rounding="widgets.active.rounding")])],
    ),
    dict(
        name="FileListView / FileTreeView", category="Widgets du framework", sample="photo.png", functions="FileListView, FileIconView, FileTreeView",
        note="Une sélection avec le focus utilise la sélection du thème; sans focus, la couleur du texte à 15 %. Un dossier visé par un dépôt prend un cadre de la couleur de la sélection.",
        parts=[("Éléments", [row("Normal", text="colors.text"), row("Rayé (liste)", fill="colors.striped_bg", text="colors.text"), row("Survol", fill="widgets.hovered.weak_fill*0.5", text="colors.text", rounding=4), row("Sélectionné (focus)", fill="colors.selection_bg", text="colors.selection_stroke", rounding=4), row("Sélectionné (sans focus)", fill="colors.text*0.15", text="colors.strong_text", rounding=4), row("Cible d'un dépôt", fill="colors.selection_bg*0.25", stroke="colors.selection_bg", text="colors.text", rounding=4)]), ("Séparateurs", [row("Colonnes et en-tête", stroke="widgets.noninteractive.stroke"), row("Entre les lignes", stroke="widgets.noninteractive.stroke*0.6")])],
    ),
]

CATEGORIES = []
for w in WIDGETS:
    if w["category"] not in CATEGORIES:
        CATEGORIES.append(w["category"])

def split(path):
    """a path with a factor, `colors.selection_bg*0.35`: the token and the factor the widget applies to its alpha"""
    if isinstance(path, str) and "*" in path:
        name, factor = path.split("*")
        return name, float(factor)
    return path, 1.0


# which widgets use which token (read from the rows above)
USED_BY = {}
for _w in WIDGETS:
    for _title, _rows in _w["parts"]:
        for _r in _rows:
            for _key in ("fill", "stroke", "text", "rounding"):
                if isinstance(_r[_key], str):
                    _token = split(_r[_key])[0]
                    USED_BY.setdefault(_token, [])
                    if _w["name"] not in USED_BY[_token]:
                        USED_BY[_token].append(_w["name"])

# ---------------------------------------------------------------------------------------------------------
# the style sheet: tokens of both themes, edits on top of the defaults, a global transparency
# ---------------------------------------------------------------------------------------------------------

DERIVED = {"colors.strong_text", "colors.weak_text"}  # follow other tokens: not set
FILLS = {"colors.panel", "colors.window", "colors.text_edit_bg", "colors.striped_bg", "colors.code_bg", "colors.selection_bg"}

base = {}        # theme -> {path: value}, the defaults
edits = {"light": {}, "dark": {}}  # theme -> {path: value}, what the user changed
view = {}        # theme -> {path: value}, base + edits + transparency: what is drawn and applied
holders = {}     # (theme, path) -> RGBA / Float holders of the editors
opacity = Float(1.0)
state = dict(selected="Button", tab=0, dirty=True, last_opacity=1.0, filter=Str(""), note="")
filter_text = Str("")
if os.environ.get("STYLE_DOC"):  # for the checks: STYLE_DOC=tokens or the name of a widget
    state["tab"], state["selected"] = (1, "Button") if os.environ["STYLE_DOC"] == "tokens" else (0, os.environ["STYLE_DOC"])


def flatten(snap):
    """the snapshot as {path: value} for the tokens that are colors, strokes or roundings"""
    out = {}
    for name, value in snap["colors"].items():
        out[f"colors.{name}"] = value
    for st, d in snap["widgets"].items():
        for name, value in d.items():
            out[f"widgets.{st}.{name}"] = value
    for name, value in snap["window"].items():
        if "shadow" not in name:  # a shadow has five values: not edited here
            out[f"window.{name}"] = value
    return out


def is_fill(path):
    return path in FILLS or path.endswith(".fill") or path.endswith(".weak_fill")


def hex_to_rgba(hex_color):
    h = hex_color.lstrip("#")
    return [int(h[i : i + 2], 16) for i in (0, 2, 4, 6)]


def rgba_hex(c):
    return "#%02x%02x%02x%02x" % tuple(c)


def fade(hex_color, factor):
    r, g, b, a = hex_to_rgba(hex_color)
    return rgba_hex((r, g, b, max(0, min(255, round(a * factor)))))


def color_of(value):
    """the color of a token: a color, or the color of a stroke (width, color)"""
    return value[1] if isinstance(value, (tuple, list)) else value


def recompute():
    """view = defaults, then the edits, then the transparency on every background"""
    for theme, _ in THEMES:
        merged = dict(base[theme])
        merged.update(edits[theme])
        for path, value in list(merged.items()):
            if is_fill(path) and isinstance(value, str):
                merged[path] = fade(value, opacity.value)
        view[theme] = merged


def apply(ctx):
    for theme, _ in THEMES:
        ctx.set_style_tokens(theme, {p: v for p, v in view[theme].items() if p not in DERIVED})


def holder_for(theme, path):
    key = (theme, path)
    if key not in holders:
        value = edits[theme].get(path, base[theme][path])
        if path.endswith("rounding") or path.endswith("expansion"):
            holders[key] = Float(float(value))
        else:
            r, g, b, a = hex_to_rgba(color_of(value))
            holders[key] = RGBA(r / 255, g / 255, b / 255, a / 255)
    return holders[key]


def reset_holders():
    holders.clear()


def edit_token(theme, path):
    """an editor of a color (with the width of the stroke) or of a rounding; returns True if it changed"""
    value = edits[theme].get(path, base[theme][path])
    h = holder_for(theme, path)
    if path.endswith("rounding") or path.endswith("expansion"):
        before = h.value
        slider_float(h, 0.0, 32.0, "")
        if h.value != before:
            edits[theme][path] = round(h.value)
            return True
        return False
    before = h.to_tuple()
    color_edit_button_rgba(h)
    now = h.to_tuple()
    if now != before:
        hexa = rgba_hex([int(round(c)) for c in now])
        edits[theme][path] = (value[0], hexa) if isinstance(value, (tuple, list)) else hexa
        return True
    return False


def save_sheet():
    data = {"opacity": opacity.value, "light": edits["light"], "dark": edits["dark"], "tokens": {t: base[t] for t, _ in THEMES}}
    with open(SHEET, "w") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    state["note"] = f"Enregistré: {SHEET}"


def load_sheet():
    try:
        with open(SHEET) as f:
            data = json.load(f)
    except OSError:
        state["note"] = "Pas encore de feuille enregistrée"
        return
    opacity.value = data.get("opacity", 1.0)
    for theme, _ in THEMES:
        edits[theme] = {p: (tuple(v) if isinstance(v, list) else v) for p, v in data.get(theme, {}).items()}
    reset_holders()
    state["dirty"] = True
    state["note"] = "Feuille chargée"


# ---------------------------------------------------------------------------------------------------------
# drawing
# ---------------------------------------------------------------------------------------------------------


def resolve(theme, path, dim=1.0):
    """the value of a token in a theme, as drawn (transparency and fade of disabled widgets included)"""
    if path is None:
        return None
    if isinstance(path, (int, float)):  # a fixed value of the widget, not a token
        return path
    path, factor = split(path)
    dim = dim * factor
    value = view[theme].get(path)
    if value is None:
        return None
    if isinstance(value, (tuple, list)):
        return (value[0], fade(value[1], dim)) if dim != 1.0 else (value[0], value[1])
    if isinstance(value, str):
        return fade(value, dim) if dim != 1.0 else value
    return value


def describe(theme, r):
    """the values of a row, as text: background, border, text, rounding, font"""
    parts = []
    for name, key in (("fond", "fill"), ("bord", "stroke"), ("texte", "text")):
        path = r[key]
        if path:
            token, factor = split(path)
            value = edits[theme].get(token) or base[theme].get(token)
            parts.append(f"{name} {format_value(value)}" + (f" ×{factor:g}" if factor != 1.0 else ""))
    if isinstance(r["rounding"], (int, float)):
        parts.append(f"rayon {r['rounding']:g} (fixe)")
    elif r["rounding"]:
        parts.append(f"rayon {view[theme].get(r['rounding'])}")
    if r["font"]:
        size, family = font_of(theme, r["font"])
        parts.append(f"police {family} {size:g}pt")
    return "  ·  ".join(parts)


def format_value(value):
    if isinstance(value, (tuple, list)):
        return f"{value[0]:g}px {value[1]}"
    return str(value)


FONTS = {}


def font_of(theme, name):
    return FONTS[theme].get(name, (14.0, "?"))


def specimen(theme, w, r):
    """what the row looks like: a box with the background, the border and the rounding, and the text in its color"""
    fill = resolve(theme, r["fill"], r["dim"])
    stroke = resolve(theme, r["stroke"], r["dim"])
    if isinstance(stroke, str):  # a color token used as a border (the focus): one point wide
        stroke = (1.0, stroke)
    text = resolve(theme, r["text"], r["dim"])
    rounding = resolve(theme, r["rounding"]) or 0
    options = dict(rounding=float(rounding), inner_margin=(10, 4, 10, 4))
    if fill:
        options["fill"] = fill
    if stroke and stroke[0] > 0:
        options["stroke"] = (stroke[0], stroke[1])
    text_color = color_of(text) if text else resolve(theme, "colors.text")
    sample = w["sample"] if (r["fill"] or r["text"]) else ""

    def inside():
        set_min_width(120)
        colored_label(text_color, sample or " ")

    frame(inside, **options)


def theme_column(theme, title, w, part_rows):
    text = resolve(theme, "colors.text")
    weak_text = resolve(theme, "colors.weak_text")

    def card():
        # the card is in the colors of its own theme, not of the theme shown: the text is colored by hand
        colored_label(resolve(theme, "colors.strong_text"), title)
        for r in part_rows:
            horizontal(lambda r=r: (specimen(theme, w, r), colored_label(text, r["label"])))
            colored_label(weak_text if theme == "light" else text, describe(theme, r))

    frame(card, fill=view[theme]["colors.panel"], rounding=10.0, inner_margin=12.0, stroke=(1.0, "#80808040"))


def widget_page(w):
    heading(w["name"])
    weak(w["functions"])
    if w.get("note"):
        label(w["note"])
    separator()
    for title, part_rows in w["parts"]:
        strong(title)

        def pair(i, part_rows=part_rows):
            theme, name = THEMES[i]
            theme_column(theme, name, w, part_rows)

        columns(2, pair)
        add_space(8)
    paths = sorted({split(p)[0] for _, rs in w["parts"] for r in rs for p in (r["fill"], r["stroke"], r["text"], r["rounding"]) if isinstance(p, str)})
    collapsing("Jetons utilisés", lambda: [code(p) for p in paths])


# ---- the view of the tokens -----------------------------------------------------------------------------


def duplicates(theme):
    """tokens that have the same value under different names: candidates to be one"""
    groups = {}
    for path, value in view[theme].items():
        if path in DERIVED or path.endswith("rounding") or path.endswith("expansion"):
            continue
        key = format_value(value)
        groups.setdefault(key, []).append(path)
    return {k: v for k, v in groups.items() if len(v) > 1}


def tokens_page(ctx):
    heading("Jetons du style")
    weak("Chaque valeur de la feuille de style, dans les deux thèmes. Modifiable: toute l'app suit.")
    horizontal(lambda: (label("Filtrer"), text_edit_singleline(filter_text, hint_text="colors.panel, hovered, rounding...")))
    needle = filter_text.value.strip().lower()
    changed = False

    def table_rows():
        nonlocal changed
        for path in sorted(base["light"]):
            if needle and needle not in path.lower() and not any(needle in u.lower() for u in USED_BY.get(path, [])):
                continue
            label(path)
            for theme, _ in THEMES:
                if path in DERIVED:
                    colored_label(color_of(base[theme][path]), color_of(base[theme][path]))
                elif edit_token(theme, path):
                    changed = True
            weak(", ".join(USED_BY.get(path, [])) or "—")
            end_row()

    grid("tokens", lambda: (
        strong("Jeton"), strong("Clair"), strong("Sombre"), strong("Utilisé par"), end_row(), table_rows()), striped=True, num_columns=4, spacing=(14.0, 6.0))
    if changed:
        state["dirty"] = True
    separator()
    collapsing("Valeurs identiques sous des noms différents (à unifier)", lambda: [
        (strong(title), [horizontal(lambda groups=groups, key=key: (code(key), weak(", ".join(groups[key])))) for key in sorted(groups)])
        for title, groups in ((name, duplicates(theme)) for theme, name in THEMES)
    ])


# ---------------------------------------------------------------------------------------------------------
# the window
# ---------------------------------------------------------------------------------------------------------


def toolbar(ctx):
    def left():
        label("Mode")
        if button_clicked("Clair"):
            ctx.set_light_theme()
        if button_clicked("Sombre"):
            ctx.set_dark_theme()
        if button_clicked("Système"):
            ctx.set_system_theme()
        separator()
        label("Transparence")
        slider_float(opacity, 0.2, 1.0, "")
        if button_clicked("Enregistrer"):
            save_sheet()
        if button_clicked("Charger"):
            load_sheet()
        if button_clicked("Réinitialiser"):
            edits["light"].clear()
            edits["dark"].clear()
            opacity.value = 1.0
            reset_holders()
            state["dirty"] = True
        if state["note"]:
            weak(state["note"])

    horizontal(left)


def sidebar():
    set_min_width(190)
    if selectable_label_clicked(state["tab"] == 1, "Jetons (feuille de style)"):
        state["tab"] = 1
    separator()
    for category in CATEGORIES:
        strong(category)
        for w in WIDGETS:
            if w["category"] == category and selectable_label_clicked(state["tab"] == 0 and state["selected"] == w["name"], w["name"]):
                state["tab"] = 0
                state["selected"] = w["name"]
        add_space(6)


def update_func(ctx):
    if not base:
        for theme, _ in THEMES:
            snap = ctx.style_snapshot(theme)
            base[theme] = flatten(snap)
            FONTS[theme] = {k: tuple(v) for k, v in snap["fonts"].items()}
    if opacity.value != state["last_opacity"]:
        state["last_opacity"] = opacity.value
        state["dirty"] = True
    if state["dirty"] or not view:
        recompute()
        apply(ctx)
        state["dirty"] = False
    left_panel("styles", lambda: scroll_area_vertical(sidebar))
    top_panel("tools", lambda: toolbar(ctx))
    if state["tab"] == 1:
        scroll_area_both(lambda: tokens_page(ctx))
    else:
        w = next(w for w in WIDGETS if w["name"] == state["selected"])
        scroll_area_vertical(lambda: widget_page(w))


if __name__ == "__main__":
    run_native("Styles des widgets", update_func, inner_width=1250, inner_height=860)
