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

import datetime
import json
import os

from pyegui import *

SHEET = os.path.join(os.path.dirname(os.path.abspath(__file__)), "style_sheet.json")
SHEETS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "style_sheets")
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
STRONG, WEAK = "colors.strong_text", "colors.weak_text"
ROUND = "widgets.inactive.rounding"  # the corners of most controls
LINE = "widgets.noninteractive.stroke"  # the thin line of separators, groups, header rules
SEL, SEL_TEXT = "colors.selection_bg", "colors.selection_stroke"
# factors the widgets apply to a token (alpha multiplied): named once, since several widgets want the same look
SEL_SOFT = SEL + "*0.35"  # a range, a selected row of a list, a dragged row
UNFOCUSED = TEXT + "*0.15"  # a selection in a list that has lost the focus
HOVER_ROW = TEXT + "*0.06"  # a hovered row of a table
HOVER_SOFT = "widgets.hovered.weak_fill*0.5"  # a hovered row of a property grid or of a file view
TRACK = TEXT + "*0.08"  # the track of a horizontal scroll bar, a moved column
BOX_LINE = TEXT + "*0.7"  # the outline of an empty check box


def tokens_of(rows):
    """the tokens (without the factor) that rows read"""
    return {split(r[k])[0] for r in rows for k in ("fill", "stroke", "text", "rounding") if isinstance(r[k], str)}


def popup(label, rounding="window.rounding"):
    """the look of a window, a menu, a calendar: the window color, its stroke and corners"""
    return row(label, fill="colors.window", stroke="window.stroke", text=TEXT, rounding=rounding)


def chip(label, inherited=False):
    """a tag or a choice as a chip: the soft background of the theme; an inherited one is fainter"""
    return row(label, fill="derived.soft_fill" + "*0.6" * inherited, text=WEAK if inherited else STRONG, rounding=10)


def field(label, stroke, rounding=ROUND, dim=1.0):
    """a text field: the background of the edits, a stroke that follows the state"""
    return row(label, fill="colors.text_edit_bg", stroke=stroke, text=TEXT, rounding=rounding, dim=dim)


def selection_rows(feminine=True, rounding=None):
    """the selected item of a list: the selection of the theme with the focus, a discreet background without"""
    word = "Sélectionnée" if feminine else "Sélectionné"
    return [row(f"{word} (focus)", fill=SEL, text=SEL_TEXT, rounding=rounding), row(f"{word} (sans focus)", fill=UNFOCUSED, text=STRONG, rounding=rounding)]


def tab_rows(prefix, rounding=None):
    """the tabs of a Dock / TabView: bar, active, inactive and hovered tab, from the `<prefix>.*` tokens"""
    t = lambda name: f"{prefix}.{name}"
    return [
        ("Barre", [row("Fond de la barre", fill=t("bar_fill"), text=TEXT)]),
        ("Onglet", [
            row("Actif", fill=t("active_fill"), stroke=t("active_stroke"), text=t("active_text"), rounding=rounding),
            row("Inactif", fill=t("inactive_fill"), stroke=t("inactive_stroke"), text=t("inactive_text"), rounding=rounding),
            row("Survol", fill=t("hovered_fill"), stroke=t("inactive_stroke"), text=t("inactive_text"), rounding=rounding)]),
    ]


# rows that several widgets draw the same way
HEADER_RULE = row("Trait sous l'en-tête", stroke=LINE)
ROW_PLAIN, ROW_STRIPED = row("Normale", text=TEXT), row("Rayée", fill="colors.striped_bg", text=TEXT)
ROW_HOVERED = row("Survol", fill=HOVER_ROW, text=TEXT)
LIST_HOVERED = row("Ligne survolée", fill="widgets.hovered.weak_fill", text=TEXT, rounding=5)
BOX_ON, BOX_OFF = row("Case cochée", fill="colors.hyperlink", rounding=3), row("Case vide", stroke=BOX_LINE, rounding=3)

WIDGETS = [
    dict(
        name="Label", category="Texte", sample="Texte courant",
        functions="label, strong, weak, small, heading, monospace, colored_label",
        note="Le texte n'a pas de fond ni de bordure: il prend la couleur du texte du thème, le fort et le faible en dérivent.",
        parts=[
            ("Texte", [row("Courant", text="colors.text", font="Body"), row("Fort (strong)", text=STRONG, font="Body"), row("Faible (weak)", text=WEAK, font="Body"), row("Titre (heading)", text=STRONG, font="Heading"), row("Petit (small)", text="colors.text", font="Small")]),
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
        parts=[("Choix", [row("Non sélectionné", text="widgets.inactive.fg_stroke", rounding=ROUND)] + by_state("weak_fill", states=[("Survol", "hovered"), ("Pressé", "active")], disabled=False) + [row("Sélectionné", fill=SEL, stroke=SEL_TEXT, text=SEL_TEXT, rounding=ROUND)])],
    ),
    dict(
        name="Checkbox / Radio", category="Boutons", sample="✓", functions="checkbox, radio_value",
        note="La case a le fond `fill` (pas `weak_fill`); la coche prend la couleur du trait `fg_stroke`.",
        parts=[("Case", by_state("fill", states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Actif (pressé)", "active")])), ("Libellé", [row("Texte", text="widgets.noninteractive.fg_stroke")])],
    ),
    dict(
        name="Switch", category="Boutons", sample="●", functions="switch",
        note="Allumé: le fond est la couleur de la sélection.",
        parts=[("Interrupteur", by_state("fill", states=[("Éteint", "inactive"), ("Survol", "hovered")], disabled=False) + [row("Allumé", fill=SEL, stroke=SEL_TEXT, text="widgets.active.fg_stroke", rounding=ROUND)])],
    ),
    dict(
        name="Slider / DragValue", category="Boutons", sample="42", functions="slider_int, slider_float, drag_int, drag_float",
        note="Le rail est `inactive.fill`, la partie remplie la couleur de la sélection, la poignée suit l'état. Le champ de valeur est un bouton.",
        parts=[("Rail", [row("Rail", fill="widgets.inactive.fill", rounding=ROUND), row("Partie remplie", fill=SEL, rounding=ROUND)]), ("Poignée", by_state("fill", disabled=False)), ("Champ de valeur", by_state("weak_fill", disabled=False))],
    ),
    dict(
        name="ComboBox", category="Boutons", sample="Choisir ▾", functions="combo_box, choice_combo",
        note="Fermée: comme un bouton. Ouverte: l'état `open`; la liste est un menu.",
        parts=[("Liste fermée", by_state("weak_fill", states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Ouverte", "open")]))],
    ),
    dict(
        name="ProgressBar", category="Boutons", sample="60 %", functions="progress",
        parts=[("Barre", [row("Fond", fill="colors.text_edit_bg", stroke=LINE, text=TEXT, rounding=ROUND), row("Partie remplie", fill=SEL, text=TEXT, rounding=ROUND)])],
    ),
    dict(
        name="TextEdit", category="Saisie", sample="texte saisi", functions="text_edit_singleline, text_edit_multiline, code_editor",
        note="Fond `text_edit_bg` (le même que les tracés et les barres de défilement). Le focus prend le trait de la sélection.",
        parts=[
            ("Champ", [field("Inactif", "widgets.inactive.stroke"), field("Survol", "widgets.hovered.stroke", "widgets.hovered.rounding"), field("Focus", SEL_TEXT), field("Désactivé (fondu)", "widgets.inactive.stroke", dim=0.45)]),
            ("Texte sélectionné", [row("Sélection", fill=SEL, text=TEXT), row("Curseur", text="colors.cursor")]),
        ],
    ),
    dict(
        name="Window / Modal", category="Conteneurs", sample="Fenêtre", functions="window, modal, settings_window",
        parts=[("Fenêtre", [popup("Fenêtre")])],
    ),
    dict(
        name="Menu / Popup / Tooltip", category="Conteneurs", sample="Menu", functions="menu_button, context_menu, tooltip, popup",
        note="Même fond que les fenêtres, coins `menu_rounding`, ombre `popup_shadow`. Un élément de menu est un bouton (survol/actif).",
        parts=[("Menu", [popup("Menu", "window.menu_rounding")]), ("Élément de menu", by_state("weak_fill", disabled=False, states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Actif (pressé)", "active")]))],
    ),
    dict(
        name="Panel", category="Conteneurs", sample="Panneau", functions="left_panel, right_panel, top_panel, bottom_panel, sidebar_panel...",
        note="Le fond entre les boîtes est `panel`; les règles de mise en page (`set_layout`) peuvent le remplacer.",
        parts=[("Panneau", [row("Fond", fill="colors.panel", text="colors.text")])],
    ),
    dict(
        name="Group / Frame", category="Conteneurs", sample="Groupe", functions="group, frame",
        note="`group` prend le trait des widgets non interactifs et leurs coins; `frame` n'a ni fond ni trait par défaut.",
        parts=[("Groupe", [row("Groupe", stroke=LINE, text=TEXT, rounding="widgets.noninteractive.rounding")])],
    ),
    dict(
        name="Table / Grid", category="Données", sample="ligne", functions="table, grid(striped), tree_view",
        note="Une ligne sur deux a le fond `striped_bg`; la ligne choisie, la couleur de la sélection.",
        parts=[("Lignes", [row("Ligne", text=TEXT), row("Ligne rayée", fill="colors.striped_bg", text=TEXT), row("Sélectionnée", fill=SEL, text=SEL_TEXT)])],
    ),
    dict(
        name="Separator", category="Données", sample="", functions="separator",
        parts=[("Ligne", [row("Trait", stroke=LINE)])],
    ),
    dict(
        name="PropertySet", category="Widgets du framework", sample="Valeur", functions="PropertySet, property_set",
        note="Une page de propriétés comme la fenêtre Propriétés de Visual Studio. Tout vient des jetons des widgets non interactifs et de la sélection, atténués par un facteur (×0.7 = alpha multiplié).",
        parts=[
            ("Cadre", [row("Cadre", stroke=LINE, text=TEXT, rounding=8)]),
            ("En-tête de groupe", [row("Repos", fill="widgets.noninteractive.weak_fill*0.7", text="colors.strong_text"), row("Survol", fill="widgets.noninteractive.weak_fill", text="colors.strong_text"), row("Compteur", text="colors.weak_text")]),
            ("Ligne", [row("Repos", text="colors.text"), row("Survol", fill=HOVER_SOFT, text=TEXT), row("Sélectionnée", fill=SEL_SOFT, text=TEXT), row("Lecture seule", text="colors.weak_text"), row("Séparateur", stroke=LINE + "*0.6"), row("Champ qui a le focus", stroke=SEL_TEXT, text=TEXT, rounding=3)]),
            ("Description", [row("Zone de description", fill="widgets.noninteractive.weak_fill*0.8", text="colors.text", rounding=8)]),
        ],
    ),
    dict(
        name="Table", category="Widgets du framework", sample="cellule", functions="table, table(view=...), TableView",
        note="Le titre est fort quand la colonne est triée ou survolée, faible sinon. La barre de défilement horizontale se dessine avec la couleur du texte à faible alpha.",
        parts=[
            ("En-tête", [HEADER_RULE, row("Titre (trié ou survolé)", text=STRONG), row("Titre (autre colonne)", text=WEAK), row("Filtre actif", text="colors.hyperlink"), row("Filtre inactif", text=WEAK), row("Colonne déplacée", fill=TRACK, text=TEXT)]),
            ("Lignes", [ROW_PLAIN, ROW_STRIPED, ROW_HOVERED, row("Sélectionnée", fill=SEL, text=TEXT)]),
            ("Barre de défilement horizontale", [row("Piste", fill=TRACK, rounding=3), row("Poignée", fill=TEXT + "*0.3", rounding=3), row("Poignée (survol, glissée)", fill=TEXT + "*0.5", rounding=3)]),
            ("Réglage des colonnes", [row("Bouton au repos", fill="widgets.inactive.weak_fill", text=TEXT, rounding=4), row("Bouton au survol", fill="widgets.hovered.fill", text=TEXT, rounding=4), LIST_HOVERED, row("Ligne glissée", fill=SEL_SOFT, text=TEXT, rounding=5)]),
        ],
    ),
    dict(
        name="Dock / TabView (onglets)", category="Widgets du framework", sample="Fichiers", functions="Dock, DockPane, tab_view, TabView",
        note="Le Dock et le TabView lisent les mêmes jetons `tabs.*` (onglets classiques, collés à leur panneau). Une option donnée à l'appel (`active_tab_fill=...`) l'emporte sur le jeton, le jeton sur le défaut. Un onglet classique actif sans fond prend celui de son panneau.",
        parts=tab_rows("tabs"),
    ),
    dict(
        name="Dock / TabView (pilules)", category="Widgets du framework", sample="Fichiers", functions="Dock(tab_style='pill'), tab_view(tab_style='pill')",
        note="Les onglets en pilule flottent dans la barre: l'actif a la couleur de la sélection, les autres sont nus, sans trait. Jetons `pill_tabs.*`.",
        parts=tab_rows("pill_tabs", rounding=12),
    ),
    dict(
        name="Date", category="Widgets du framework", sample="27/11/2026", functions="date_picker_button, date_range_picker, DateRange",
        note="Le bouton est un bouton ordinaire. Dans le calendrier, les jours de la plage sont la couleur de la sélection atténuée; les bornes, la sélection pleine.",
        parts=[("Bouton de date", by_state("weak_fill", disabled=False, states=[("Inactif", "inactive"), ("Survol", "hovered"), ("Ouvert", "open")])), ("Calendrier", [
            popup("Fond du calendrier"),
            row("Jour", text=TEXT),
            row("Jour d'un autre mois", text=WEAK),
            row("Jour au survol", fill="widgets.hovered.weak_fill", text=TEXT, rounding="widgets.hovered.rounding"),
            row("Jour dans la plage", fill=SEL_SOFT, text=TEXT),
            row("Borne de la plage", fill=SEL, text=SEL_TEXT),
            row("Aujourd'hui", stroke="widgets.noninteractive.fg_stroke", text=TEXT)])],
    ),
    dict(
        name="Tags (puces)", category="Widgets du framework", sample="dev/web", functions="Tags, tags_edit, TagTreeView, tags_pick",
        note="La puce est le fond doux du thème (`derived.soft_fill`, la fenêtre tirée vers le texte) avec la couleur du tag en point et en contour; le texte est la couleur du tag tirée vers le texte fort. La couleur d'un tag est une donnée, pas un jeton.",
        parts=[
            ("Puce", [chip("Puce"), chip("Puce héritée (atténuée)", inherited=True)]),
            ("Bouton « ajouter »", [row("Repos", fill="widgets.inactive.weak_fill", stroke=WEAK + "*0.4", text=WEAK, rounding=10), row("Survol", fill="widgets.hovered.weak_fill", stroke=WEAK + "*0.4", text=WEAK, rounding=10)]),
            ("Liste des tags", [row("Ligne", text=TEXT), LIST_HOVERED, BOX_ON, BOX_OFF, row("Case cochée (héritée)", fill="colors.hyperlink*0.5", rounding=3)]),
        ],
    ),
    dict(
        name="Liste de choix (simple / multiple)", category="Widgets du framework", sample="Statut", functions="choice_combo, choice_edit, ChoiceSpec (property_set, colonnes)",
        note="Choix multiple: des cases. Choix unique: des ronds, un seul est plein. La valeur choisie s'affiche comme une puce.",
        parts=[
            ("Choix multiple (cases)", [BOX_ON, BOX_OFF, LIST_HOVERED]),
            ("Choix unique (ronds)", [row("Rond choisi", fill="colors.hyperlink", rounding=10), row("Rond vide", stroke=TEXT, rounding=10)]),
            ("Valeur affichée", [chip("Puce de la valeur"), chip("Valeur héritée ou résumée", inherited=True)]),
        ],
    ),
    dict(
        name="DataGrid", category="Widgets du framework", sample="Pomme", functions="DataGrid, data_grid (liste ou arbre, cellules éditables)",
        note="Une grille de données qui n'est pas liée aux fichiers: la liste et l'arbre dessinent comme les vues de fichiers. La sélection avec le focus est la sélection du thème; sans focus, le texte à 15 %.",
        parts=[("Lignes", [ROW_PLAIN, ROW_STRIPED, ROW_HOVERED] + selection_rows()), ("En-tête", [HEADER_RULE, row("Titre", text=WEAK), row("Titre trié", text=STRONG)]), ("Cellule en édition", [field("Champ", SEL_TEXT)]),
               ("Traits (jetons grid.*)", [row("Entre les lignes", stroke="grid.row_line"), row("Entre les colonnes", stroke="grid.column_line"), row("Guides de l'arbre", stroke="grid.guide_line")]),
               ("Symbole d'ouverture (jetons grid.chevron*)", [row("Symbole", text="grid.chevron_color"), row("Symbole survolé", text="grid.chevron_hover_color")])],
    ),
    dict(
        name="TreeView", category="Widgets du framework", sample="dossier", functions="tree_view, TreeView, TagTreeView",
        note="Dessiné par egui_ltreeview. La sélection d'un arbre sans focus est un fond discret, pas la couleur de la sélection.",
        parts=[("Lignes", [ROW_PLAIN, row("Sélectionnée (arbre actif)", fill=SEL, text=SEL_TEXT, rounding="widgets.active.rounding"), row("Sélectionnée (sans focus)", fill="widgets.inactive.weak_fill*0.3", text=TEXT, rounding="widgets.active.rounding")]), ("Repères", [row("Guides d'indentation", stroke=LINE), row("Marqueur de dépôt", fill=SEL + "*0.6", rounding="widgets.active.rounding")])],
    ),
    dict(
        name="FileListView / FileTreeView", category="Widgets du framework", sample="photo.png", functions="FileListView, FileIconView, FileTreeView",
        note="Une sélection avec le focus utilise la sélection du thème; sans focus, la couleur du texte à 15 %. Un dossier visé par un dépôt prend un cadre de la couleur de la sélection.",
        parts=[("Éléments", [row("Normal", text=TEXT), row("Rayé (liste)", fill="colors.striped_bg", text=TEXT), row("Survol", fill=HOVER_SOFT, text=TEXT, rounding=4)] + selection_rows(False, 4) + [row("Cible d'un dépôt", fill=SEL + "*0.25", stroke=SEL, text=TEXT, rounding=4)]), ("Séparateurs", [row("Colonnes et en-tête", stroke=LINE), row("Entre les lignes", stroke=LINE + "*0.6")])],
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
        for _token in tokens_of(_rows):
            if _w["name"] not in USED_BY.setdefault(_token, []):
                USED_BY[_token].append(_w["name"])

# the popups are not drawn by a widget of the pages above
USED_BY.update({"popups.dropdown_fill": ["ComboBox (the list)"], "popups.menu_fill": ["Menu / Popup (menus, context menus)"],
                "popups.tooltip_fill": ["Menu / Popup / Tooltip (tooltips)"],
                "popups.window_fill": ["Window / Modal (not the fill under the whole window: colors.window)"]})

# ---------------------------------------------------------------------------------------------------------
# the style sheet: tokens of both themes, edits on top of the defaults, a global transparency
# ---------------------------------------------------------------------------------------------------------

DERIVED = {"colors.strong_text", "colors.weak_text", "derived.soft_fill"}  # follow other tokens: not set
FILLS = {"colors.panel", "colors.window", "colors.text_edit_bg", "colors.striped_bg", "colors.code_bg", "colors.selection_bg"}

base = {}        # theme -> {path: value}, the defaults
edits = {"light": {}, "dark": {}}  # theme -> {path: value}, what the user changed
extras = {"light": {}, "dark": {}}  # tokens of a chosen sheet that the editors do not show (the symbol of the grids, the tokens of the contexts `menu.*`, `sidebar.*`...), given as they are
view = {}        # theme -> {path: value}, base + edits + transparency: what is drawn and applied
holders = {}     # (theme, path) -> RGBA / Float holders of the editors
opacity = Float(1.0)
state = dict(selected="Button", tab=0, dirty=True, last_opacity=1.0, filter=Str(""), note="")
filter_text = Str("")
sheet_choice = Int(0)
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
    for group in ("tabs", "pill_tabs"):
        for name, value in snap[group].items():
            out[f"{group}.{name}"] = value
    for name, value in snap["grid"].items():
        if name != "chevron":  # a name of symbol, not a color: kept as it is by `extras`
            out[f"grid.{name}"] = value
    for name, value in snap["popups"].items():  # the fill of the dropdowns, menus and tooltips, in RGBA
        out[f"popups.{name}"] = value
    for name, value in snap["window"].items():
        if "shadow" not in name:  # a shadow has five values: not edited here
            out[f"window.{name}"] = value
    return out


def is_number(path):
    return path.endswith(("rounding", "expansion", "spacing", "bar_height", "radius", "size"))


def is_fill(path):
    return path in FILLS or path.endswith("fill")


def derived_tokens(flat, dark):
    """tokens the widgets compute from others: the soft background of the chips (the window pulled toward the text)"""
    window = hex_to_rgba(flat["colors.window"])
    text = hex_to_rgba(flat["colors.text"])
    amount = 0.14 if dark else 0.03
    mixed = [round(window[i] + (text[i] - window[i]) * amount) for i in range(3)] + [255]
    return {"derived.soft_fill": rgba_hex(mixed)}


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
        merged.update(derived_tokens(merged, theme == "dark"))
        for path, value in list(merged.items()):
            if is_fill(path) and isinstance(value, str):
                merged[path] = fade(value, opacity.value)
        view[theme] = merged


def apply(ctx):
    """only what differs from the defaults is given to the app: a token that is only the default value would pin it
    (a tab with no fill takes the one of its pane, a token set to transparent would not)"""
    ctx.reset_style()
    for theme, _ in THEMES:
        changed = set(edits[theme])
        if opacity.value < 1.0:
            changed |= {p for p in view[theme] if is_fill(p)}
        ctx.set_style_tokens(theme, {p: view[theme][p] for p in changed if p not in DERIVED and p in view[theme]})
        if extras[theme]:
            ctx.set_style_tokens(theme, extras[theme])


def holder_for(theme, path):
    key = (theme, path)
    if key not in holders:
        value = edits[theme].get(path, base[theme][path])
        if is_number(path):
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
    if is_number(path):
        before = h.value
        slider_float(h, 0.0, 40.0, "")
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


def sheet_names():
    """the sheets of `examples/style_sheets/`, by name"""
    try:
        return sorted(f[:-5] for f in os.listdir(SHEETS_DIR) if f.endswith(".json"))
    except OSError:
        return []


def choose_sheet(name):
    """the tokens of a sheet become the edits (the ones the editors do not know go to `extras`); None is the defaults"""
    for theme, _ in THEMES:
        edits[theme].clear()
        extras[theme].clear()
    opacity.value = 1.0
    if name is not None:
        with open(os.path.join(SHEETS_DIR, name + ".json")) as f:
            data = json.load(f)
        opacity.value = data.get("opacity", 1.0)
        for theme, _ in THEMES:
            for path, value in data.get(theme, {}).items():
                value = tuple(value) if isinstance(value, list) else value
                (edits if path in base[theme] else extras)[theme][path] = value
    reset_holders()
    state["dirty"] = True
    state["note"] = f"Feuille: {name or 'défaut'}"


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



# ---------------------------------------------------------------------------------------------------------
# the live preview of each widget: the real control, drawn with the style that is applied now
# ---------------------------------------------------------------------------------------------------------

pv = dict(
    flag=Bool(True), off=Bool(False), index=Int(0), number=Int(40), real=Float(0.4), text=Str("texte saisi"),
    long=Str("plusieurs\nlignes"), row=Int(-1), window=Bool(False), date=Date(datetime.datetime.now()),
    range=DateRange(None, None), tags=Tags(["projet/web", "perso"], colors={"projet": "#3b82f6"}),
    tabs=TabView(["Fichiers", "Éditeur", "Journal"]), pills=TabView(["Un", "Deux", "Trois"]),
    tree=TreeView("sd-tree", multi_select=True), props=PropertySet(), files=None,
    multi=["a", "c"], single=["b"], values={"name": "Rapport", "done": False, "size": "21", "alpha": "0.5"},
)
CHOICES = [dict(id="a", label="À faire", color="#f59e0b"), dict(id="b", label="En cours", color="#3b82f6"), dict(id="c", label="Terminé", color="#10b981")]
NODES = [TreeNode(0, "src", [TreeNode(1, "main.py"), TreeNode(2, "style.py")]), TreeNode(3, "docs", [TreeNode(4, "guide.md")]), TreeNode(5, "README.md")]
TABLE = [["Alice", "32", "Paris"], ["Bob", "27", "Lyon"], ["Camille", "41", "Nice"], ["Dan", "19", "Lille"]]


def preview_window():
    if button_clicked("Ouvrir une fenêtre"):
        pv["window"].value = True
    window("Exemple", lambda: (label("Le fond, le trait, les coins et l'ombre de la fenêtre."), button_clicked("Fermer") and None), open=pv["window"], default_width=280)


def preview_props():
    p = pv["values"]
    rows = [
        dict(id="name", name="Nom", group="Général", value=p["name"], description="Le titre du rapport"),
        dict(id="done", name="Terminé", type="bool", group="Général", value=p["done"]),
        dict(id="size", name="Taille", type="number", group="Mise en page", unit="cm", value=p["size"]),
        dict(id="id", name="Identifiant", type="readonly", group="Mise en page", value="R-0042"),
    ]
    set_height(260)
    for change in property_set(pv["props"], rows, key="sd", id="sd-props", row_height=26):
        if change[0] == "set" and change[1] in p:
            p[change[1]] = (change[2] == "1") if change[1] == "done" else change[2]


def preview_files():
    if pv["files"] is None:
        pv["files"] = FileListView("sd-files", "~")
    set_height(220)
    file_list_view(pv["files"])


def preview_choice(multiple):
    key = "multi" if multiple else "single"
    change = choice_edit(CHOICES, pv[key], multiple=multiple, id="sd-" + key, width=220)
    if change and change[0] == "choose":
        pv[key] = change[1]
    elif change and change[0] == "clear":
        pv[key] = []


def preview_tabs(pill):
    t = pv["pills"] if pill else pv["tabs"]
    tab_view(t, lambda i: label("Contenu de " + t.titles[i]), tab_style="pill" if pill else "classic", closable=True, add_button=True)
    if t.take_added():
        t.add(f"Onglet {len(t.titles) + 1}")


GRID_DATA = {
    "1": ["Fruits", "", "Paris"], "2": ["Pomme", "12", "Lyon"], "3": ["Poire", "4", "Nice"],
    "4": ["Légumes", "", "Lille"], "5": ["Carotte", "30", "Paris"], "6": ["Navet", "7", "Lille"],
}
GRID_PARENTS = {"2": "1", "3": "1", "5": "4", "6": "4"}
GRID = DataGrid([dict(key="name", title="Nom", editable=True, width=200), dict(key="qty", title="Quantité", type="number", width=100, editable=True), dict(key="city", title="Ville", type="choice", width=120)], id="sd-grid")
GRID.expanded = ["1", "4"]


def preview_grid():
    """a tree of cells that can be edited with a double click, sorted and filtered from its header"""
    data_grid(GRID, [dict(id=i, cells=c, parent=GRID_PARENTS.get(i)) for i, c in GRID_DATA.items()], tree=True, settings=True, max_height=190)
    edit = GRID.take_edited()
    if edit:
        row, column, text = edit
        GRID_DATA[row][{"name": 0, "qty": 1, "city": 2}[column]] = text


PREVIEWS = {
    "Label": lambda: (label("Texte courant"), strong("Texte fort"), weak("Texte faible"), small("Petit texte"), heading("Titre"), colored_label("#ef4444", "Texte coloré")),
    "Hyperlink": lambda: (hyperlink_to("pyegui sur GitHub", "https://github.com/GachiLord/pyegui"), link_clicked("un faux lien")),
    "Code": lambda: (code("let x = 1;"), set_max_height(90), code_editor(pv["long"])),
    "Button": lambda: horizontal(lambda: (button_clicked("Valider"), add_enabled(False, lambda: button_clicked("Désactivé")), small_button_clicked("Petit"))),
    "Selectable": lambda: horizontal(lambda: [selectable_value(pv["index"], i, t) for i, t in enumerate(["Un", "Deux", "Trois"])]),
    "Checkbox / Radio": lambda: (checkbox(pv["flag"], "Une case"), horizontal(lambda: [radio_value(pv["index"], i, t) for i, t in enumerate(["Un", "Deux", "Trois"])])),
    "Switch": lambda: (switch(pv["flag"], "Interrupteur"), switch(pv["off"], "Éteint")),
    "Slider / DragValue": lambda: (slider_int(pv["number"], 0, 100, "Curseur"), drag_int(pv["number"], 0, 100, 1)),
    "ComboBox": lambda: combo_box(pv["index"], [0, 1, 2], ["Premier", "Deuxième", "Troisième"], "Choisir"),
    "ProgressBar": lambda: (progress(0.6), progress(pv["real"].value)),
    "TextEdit": lambda: (text_edit_singleline(pv["text"], hint_text="Saisir"), text_edit_multiline(pv["long"], hint_text="Plusieurs lignes")),
    "Window / Modal": preview_window,
    "Menu / Popup / Tooltip": lambda: menu_button("Menu ▾", lambda: (button_clicked("Ouvrir"), button_clicked("Enregistrer"), separator(), button_clicked("Quitter"))),
    "Panel": lambda: group(lambda: (strong("Un panneau"), label("Le fond entre les boîtes est celui du panneau."))),
    "Group / Frame": lambda: (group(lambda: label("Un groupe")), frame(lambda: label("Un cadre"), fill="#8080801a", rounding=8.0, inner_margin=8.0)),
    "Table / Grid": lambda: table(["Nom", "Âge", "Ville"], TABLE, pv["row"], id="sd-grid"),
    "Separator": lambda: (label("Au-dessus"), separator(), label("En dessous")),
    "PropertySet": preview_props,
    "Table": lambda: table(["Nom", "Âge", "Ville"], TABLE, pv["row"], id="sd-table", striped=True),
    "Dock / TabView (onglets)": lambda: preview_tabs(False),
    "Dock / TabView (pilules)": lambda: preview_tabs(True),
    "Date": lambda: (date_picker_button(pv["date"]), date_range_picker(pv["range"])),
    "Tags (puces)": lambda: tags_edit(pv["tags"]),
    "Liste de choix (simple / multiple)": lambda: (label("Choix multiple"), preview_choice(True), label("Choix unique"), preview_choice(False)),
    "TreeView": lambda: tree_view(pv["tree"], NODES),
    "DataGrid": preview_grid,
    "FileListView / FileTreeView": preview_files,
}


def preview(w):
    """the real control, in a frame, with the look given by the style that is applied (the mode of the bar above)"""
    fn = PREVIEWS.get(w["name"])
    if fn is None:
        return
    strong("Aperçu")
    weak("Le vrai contrôle, avec le style appliqué. Les boutons Clair / Sombre de la barre changent le thème.")
    frame(lambda: (set_min_width(max(200.0, available_width() - 8)), fn()), rounding=8.0, inner_margin=12.0, stroke=(1.0, "#80808040"))
    add_space(10)

def widget_page(w):
    heading(w["name"])
    weak(w["functions"])
    if w.get("note"):
        label(w["note"])
    separator()
    preview(w)
    for title, part_rows in w["parts"]:
        strong(title)

        def pair(i, part_rows=part_rows):
            theme, name = THEMES[i]
            theme_column(theme, name, w, part_rows)

        columns(2, pair)
        add_space(8)
    paths = sorted(set().union(*(tokens_of(rs) for _, rs in w["parts"])))
    collapsing("Jetons utilisés", lambda: [code(p) for p in paths])


# ---- the view of the tokens -----------------------------------------------------------------------------


def duplicates(theme):
    """tokens that have the same value under different names: candidates to be one"""
    groups = {}
    for path, value in view[theme].items():
        if path in DERIVED or is_number(path):
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
        label("Feuille")
        names = ["Défaut"] + sheet_names()
        before = sheet_choice.value
        combo_box(sheet_choice, list(range(len(names))), names, "")
        if sheet_choice.value != before:
            choose_sheet(None if sheet_choice.value == 0 else names[sheet_choice.value])
        separator()
        label("Transparence")
        slider_float(opacity, 0.2, 1.0, "")
        if button_clicked("Enregistrer"):
            save_sheet()
        if button_clicked("Charger"):
            load_sheet()
        if button_clicked("Réinitialiser"):
            sheet_choice.value = 0
            choose_sheet(None)
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
        if os.environ.get("STYLE_DOC_THEME") == "dark":  # for the checks
            ctx.set_dark_theme()
        elif os.environ.get("STYLE_DOC_THEME") == "light":
            ctx.set_light_theme()
        for theme, _ in THEMES:
            snap = ctx.style_snapshot(theme)
            base[theme] = flatten(snap)
            base[theme].update(derived_tokens(base[theme], theme == "dark"))
            FONTS[theme] = {k: tuple(v) for k, v in snap["fonts"].items()}
        if os.environ.get("STYLE_DOC_SHEET") in sheet_names():  # for the checks
            sheet_choice.value = 1 + sheet_names().index(os.environ["STYLE_DOC_SHEET"])
            choose_sheet(os.environ["STYLE_DOC_SHEET"])
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
    # the glass sheets (finder) are translucent: the backdrop of the system shows through, as in finder.py
    run_native("Styles des widgets", update_func, backdrop=Backdrop.LIQUID_GLASS, inner_width=1250, inner_height=860)
