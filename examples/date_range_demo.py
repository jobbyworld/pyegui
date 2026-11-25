"""Démo de `date_range_picker` : une plage de dates, éventuellement ouverte, qui filtre un tableau.

Trois façons de choisir la plage :
  - le calendrier : « Entre » (début puis fin), « À partir de » (>= une date), « Jusqu'à » (<= une date) ;
  - le champ en haut du calendrier, en français, validé par Entrée : « cette semaine », « depuis lundi »,
    « jusqu'au 20/03 », « 12/03 au 20/03 », « ce mois-ci », « demain », « tout » ;
  - le champ ci-dessous, hors du calendrier, avec les mêmes phrases (`parse_date_range`).

Lancer avec : python examples/date_range_demo.py
"""

import datetime as dt

from pyegui import *

today = dt.date.today()
day = dt.timedelta


def event(offset, title, kind):
    return (today + day(days=offset), title, kind)


events = [
    event(-60, "Bilan trimestriel", "Réunion"),
    event(-45, "Livraison v0.6", "Livraison"),
    event(-30, "Atelier egui", "Formation"),
    event(-14, "Revue de code", "Réunion"),
    event(-7, "Point équipe", "Réunion"),
    event(-1, "Mise en production", "Livraison"),
    event(0, "Démo pyegui", "Réunion"),
    event(3, "Rétrospective", "Réunion"),
    event(10, "Formation Python", "Formation"),
    event(21, "Livraison v0.8", "Livraison"),
    event(35, "Séminaire", "Formation"),
    event(50, "Bilan annuel", "Réunion"),
]

period = DateRange(today - day(days=14), None)  # >= il y a deux semaines, sans fin
typed = Str("")  # le champ hors du calendrier
typed_error = Str("")
view = TableView(sort_by="Date")  # tri et filtres du tableau, gardés entre les frames
ACCENT = (59, 130, 246)
SENTENCES = ["cette semaine", "semaine prochaine", "ce mois-ci", "depuis lundi dernier", "jusqu'à demain", "tout"]


def set_period(start, end):
    period.start, period.end = start, end


def describe(holder):
    """la plage en mots, avec « ∞ » pour une extrémité ouverte"""
    if holder.start is None and holder.end is None:
        return "toutes les dates"
    if holder.end is None:
        return f"à partir de {human_day(holder.start)} ({holder.start:%d/%m/%Y})"
    if holder.start is None:
        return f"jusqu'à {human_day(holder.end)} ({holder.end:%d/%m/%Y})"
    days = (holder.end - holder.start).days + 1
    return f"du {holder.start:%d/%m/%Y} au {holder.end:%d/%m/%Y} ({days} jour{'s' if days > 1 else ''})"


def picker_row():
    label("Période")
    date_range_picker(period, id="period")
    colored_label(ACCENT, describe(period))


def typed_row():
    label("Saisie")
    text_edit_singleline(typed, hint_text="cette semaine, depuis lundi, 12/03 au 20/03…")
    if key_pressed("Enter", when_typing=True) and typed.value.strip():
        found = parse_date_range(typed.value)
        if found is None:
            typed_error.value = f"« {typed.value} » n'est pas compris"
        else:
            typed_error.value = ""
            set_period(*found)
            typed.value = ""
    if typed_error.value:
        colored_label((239, 68, 68), typed_error.value)


def sentences_row():
    label("Essayer")
    for sentence in SENTENCES:
        if button_clicked(sentence):
            set_period(*parse_date_range(sentence))
            typed_error.value = ""


def results():
    shown = [e for e in events if period.contains(e[0])]
    strong(f"{len(shown)} évènement{'s' if len(shown) > 1 else ''} sur {len(events)}")
    if not shown:
        weak("Aucun évènement dans cette période : élargissez-la ou choisissez « tout ».")
        return
    table(
        ["Date", "Évènement", "Type"],
        [[d.isoformat(), title, kind] for d, title, kind in shown],
        view=view,
        types=["date", "text", "choice"],  # le tableau trie par date et filtre par type
        id="events",
    )


def update(ctx):
    heading("Plage de dates")
    weak("Une plage à deux extrémités, chacune pouvant rester ouverte (∞). Le champ en haut du calendrier comprend le français.")
    add_space(6)
    horizontal(picker_row)
    horizontal(typed_row)
    horizontal(sentences_row)
    separator()
    results()


run_native("Plage de dates", update)
