"""Tags with several levels: Tags (the state), tags_edit (the widget) and the tree of the tags.

A tag is a path, "projet/web/front": its chip shows the last level, the mouse over it the full path
with the color of each level. Type a tag and Enter to add it (the known ones are suggested while
typing), the cross removes it, Backspace in the empty field removes the last one, a right click
changes the color of each of its levels. compact=True shows only a dot per tag.

The sidebar is a TagTreeView of every known tag: a node per level, a count at the right, a click
selects a level, the arrow folds it, a right click changes its color or removes it.

    python examples/tags.py
"""

from pyegui import (
    Bool,
    TagTreeView,
    Tags,
    add_space,
    button_clicked,
    code,
    heading,
    horizontal,
    label,
    left_panel,
    run_native,
    scroll_area_vertical,
    separator,
    small,
    strong,
    tag_tree_view,
    tags_edit,
    weak,
)

# the state, created once: the tags of the document and the colors of some levels
tags = Tags(
    ["projet/pyegui/widgets", "perso", "urgent"],
    colors={"projet": "#3b82f6", "projet/pyegui": "#8b5cf6", "urgent": "#ef4444"},
)
# a long list, for max_visible
many = Tags(
    ["projet/pyegui", "projet/pyfinder", "perso", "urgent", "à lire", "client/acme", "client/globex",
     "perso/santé", "perso/maison/travaux", "archive"],
    colors={"urgent": "#ef4444"},
)
# every tag known to the app, offered while typing
KNOWN = [
    "projet/pyegui/widgets",
    "projet/pyegui/docs",
    "projet/pyfinder",
    "perso",
    "perso/santé",
    "perso/maison/travaux",
    "urgent",
    "à lire",
]
# every tag of the app, in the tree of the sidebar, with the number of items of each level
library = Tags(KNOWN + many.tags, colors={"projet": "#3b82f6", "projet/pyegui": "#8b5cf6", "urgent": "#ef4444"})
COUNTS = {"projet": 14, "projet/pyegui": 9, "projet/pyegui/widgets": 4, "projet/pyegui/docs": 5, "projet/pyfinder": 5,
          "perso": 7, "perso/santé": 2, "perso/maison": 3, "perso/maison/travaux": 3, "urgent": 3, "à lire": 11,
          "client": 6, "client/acme": 4, "client/globex": 2, "archive": 40}
tree = TagTreeView(folded=["client"])
started = Bool(False)
log = []


def update_func(ctx):
    if not started.value:
        ctx.set_system_theme()
        started.value = True

    def sidebar():
        small("TagTreeView")
        scroll_area_vertical(lambda: tag_tree_view(tree, library, counts=COUNTS))

    left_panel("tags", sidebar, default_size=230)  # the panels first

    heading("Tags")
    weak("Enter adds, the cross removes, right click for the colors of the levels")
    add_space(8)
    if tags_edit(tags, suggestions=KNOWN, hint_text="Ajouter un tag"):
        log.append(", ".join(tags.tags))
    add_space(12)
    separator()

    small("Read only")
    tags_edit(tags, editable=False, id="read only")
    add_space(12)
    small("Compact: a dot per tag, the full path on hover")

    def compact_row():
        label("Document.md")
        tags_edit(tags, compact=True, id="compact")

    horizontal(compact_row)
    add_space(12)
    small("Many tags: max_visible=4, then +N (its tooltip lists the others, a click shows them all)")
    tags_edit(many, max_visible=4, id="many")

    def compact_many():
        label("Rapport.pdf")
        tags_edit(many, compact=True, max_visible=4, id="many compact")

    horizontal(compact_many)
    add_space(12)
    if tree.selected:

        def selection():
            weak("Selected in the tree:")
            strong(tree.selected)
            if button_clicked("Add to the document"):
                tags.add(tree.selected)

        horizontal(selection)
    small("tags.tags")
    code(repr(tags.tags))
    small("tags.colors (the levels given a color)")
    code(repr(tags.colors))
    if log:
        small("Changes")
        for line in log[-5:]:
            label(line)


if __name__ == "__main__":
    run_native("Tags", update_func, inner_width=900, inner_height=600)
