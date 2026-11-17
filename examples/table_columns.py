"""Un tableau dont on trie, filtre, déplace, redimensionne et épingle les colonnes.

    python examples/table_columns.py

- un clic sur un titre trie, un second clic inverse l'ordre ; glisser un titre déplace la colonne
- glisser la bordure droite d'un titre redimensionne la colonne
- l'entonnoir à droite d'un titre filtre (un texte, des bornes, une période, des valeurs à garder)
- un clic droit sur un titre épingle la colonne à gauche ou à droite : les autres défilent
  horizontalement (molette, Maj + molette ou barre en bas)
"""

from pyegui import *

CITIES = ["Paris", "Lyon", "Marseille", "Lille", "Nantes"]
ROWS = [
    [f"Personne {i:03d}", str(20 + i * 7 % 45), CITIES[i % 5], f"{1000 + i * 137 % 4000} KB",
     f"2024-{1 + i % 12:02d}-{1 + i * 3 % 28:02d}", "oui" if i % 3 else "non", f"note {i}", f"extra {i}"]
    for i in range(500)
]
HEADERS = ["Nom", "Âge", "Ville", "Taille", "Date", "Actif", "Note", "Extra"]
TYPES = ["text", "number", "choice", "number", "date", "choice", "text", "text"]

view = TableView(sort_by="Nom")
view.pinned_left = ["Nom"]
view.pinned_right = ["Actif"]
selected = Int(-1)


def update_func(ctx):
    horizontal(lambda: (
        small(f"{len(view.visible_rows)} lignes, triées par {view.sort_by or '—'}"),
        button_clicked("Réinitialiser les colonnes") and view.reset_columns(),
        button_clicked("Effacer les filtres") and view.clear_filters(),
    ))
    separator()
    table(HEADERS, ROWS, selected, view=view, types=TYPES, column_widths=[140, 70, 110, 90, 110, 80, 160, 160])


run_native("Colonnes d'un tableau", update_func)
