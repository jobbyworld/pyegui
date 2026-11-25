"""Une petite application pour découvrir les concepts de base de pyegui.

Lancer avec : python examples/framework_basics.py
"""

from pyegui import *


# Les holders et les autres données sont créés une fois, hors de update().
# Ils gardent leur valeur entre les frames.
name = Str("Ada")
clicks = Int(0)
draft = Str("")
show_hint = Bool(True)
notes = []
state_loaded = False


def sidebar():
    heading("Navigation")
    label("Accueil")
    separator()
    checkbox(show_hint, "Afficher l'aide")


def topbar():
    horizontal(lambda: (strong("Pyegui"), label("Les bases en pratique")))


def status_bar():
    small(f"{clicks.value} clic(s)")


def counter():
    strong(f"Nombre de clics : {clicks.value}")

    def buttons():
        if button_clicked("Ajouter un clic"):
            print("Cliked")
            clicks.value += 1
        if button_clicked("Réinitialiser"):
            clicks.value = 0

    horizontal(buttons)


def notes_section():
    heading("Petite liste")
    text_edit_singleline(draft, hint_text="Écrire une note")

    if button_clicked("Ajouter la note") and draft.value.strip():
        notes.append(draft.value.strip())
        draft.value = ""

    for note in notes:
        label(f"• {note}")


def content():
    heading(f"Bonjour, {name.value} !")
    label("Modifie le champ : le texte affiché se met à jour dès la frame suivante.")
    text_edit_singleline(name, hint_text="Ton nom")

    separator()
    group(counter)

    if show_hint.value:
        weak("Les widgets sont appelés à chaque frame; ils ne sont pas créés une seule fois.")

    separator()
    notes_section()


def update(ctx):
    global state_loaded

    # Charger une fois au démarrage; run_native utilise persist_id pour garder cet état.
    if not state_loaded:
        saved = ctx.load_state() or {}
        if isinstance(saved, dict):
            if isinstance(saved.get("name"), str):
                name.value = saved["name"]
            if isinstance(saved.get("clicks"), int):
                clicks.value = saved["clicks"]
        state_loaded = True

    ctx.save_state({"name": name.value, "clicks": clicks.value})

    # Les panneaux doivent être dessinés avant les widgets du contenu central.
    sidebar_panel(sidebar, default_size=190)
    topbar_panel(topbar, exact_size=44, resizable=False)
    bottombar_panel(status_bar, exact_size=30, resizable=False)

    # Tout ce qui suit les panneaux est dessiné dans la zone centrale.
    content()


if __name__ == "__main__":
    run_native(
        "Pyegui — les bases",
        update,
        inner_width=720,
        inner_height=560,
        persist_id="pyegui.framework_basics",
    )
