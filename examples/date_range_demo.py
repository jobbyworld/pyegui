"""Démo de `date_range_picker` : une plage de dates, éventuellement ouverte, qui filtre un tableau.

Trois façons de choisir : « Entre » (début puis fin), « À partir de » (>= une date, sans fin) et
« Jusqu'à » (<= une date, sans début). Des raccourcis donnent les plages usuelles.

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
view = TableView(sort_by="Date")  # tri et filtres du tableau, gardés entre les frames
selected = Int(-1)
ACCENT = (59, 130, 246)


def set_period(start, end):
    period.start, period.end = start, end


def describe(holder):
    """la plage en mots : ce que le sélecteur dit, avec l'infini pour une extrémité ouverte"""
    if holder.start is None and holder.end is None:
        return "toutes les dates"
    if holder.end is None:
        return f"à partir du {holder.start:%d/%m/%Y}"
    if holder.start is None:
        return f"jusqu'au {holder.end:%d/%m/%Y}"
    days = (holder.end - holder.start).days + 1
    return f"du {holder.start:%d/%m/%Y} au {holder.end:%d/%m/%Y} ({days} jour{'s' if days > 1 else ''})"


def shortcuts():
    month_start = today.replace(day=1)
    if button_clicked("7 derniers jours"):
        set_period(today - day(days=6), today)
    if button_clicked("30 derniers jours"):
        set_period(today - day(days=29), today)
    if button_clicked("Ce mois"):
        set_period(month_start, None)
    if button_clicked("Passé"):
        set_period(None, today)
    if button_clicked("Futur"):
        set_period(today, None)
    if button_clicked("Tout"):
        set_period(None, None)


def picker_row():
    label("Période")
    date_range_picker(period, id="period")
    colored_label(ACCENT, describe(period))


def results():
    shown = [e for e in events if period.contains(e[0])]
    strong(f"{len(shown)} évènement{'s' if len(shown) > 1 else ''} sur {len(events)}")
    if not shown:
        weak("Aucun évènement dans cette période : élargissez-la ou choisissez « Tout ».")
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
    weak("Un sélecteur à deux extrémités, chacune pouvant rester ouverte (∞).")
    add_space(6)
    horizontal(picker_row)
    horizontal(shortcuts)
    separator()
    results()


run_native("Plage de dates", update)
