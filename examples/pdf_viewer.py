"""A PDF viewer: thumbnails and bookmarks in the sidebar, the pages in the middle, a toolbar and a search.

    python examples/pdf_viewer.py [file.pdf]

The pages are drawn by threads of pyegui (`craft-pdf`), the window never waits for them: a page
that is not ready shows a "..." and the view scrolls at once. Wheel to scroll, Ctrl + wheel to zoom
around the pointer, PageUp / PageDown / Home / End with the pointer over the pages. A drag selects
text (a double click a word, a triple click a line), Ctrl+C copies it; the links work. The search
marks every match in the pages. Drop a PDF on the window to open it.
Nothing to install: the metadata, the text and the bookmarks come from `PdfDocument`.
"""

import sys

from pyegui import *

doc = PdfDocument(sys.argv[1], wait=False) if len(sys.argv) > 1 else None  # read by a thread
view = PdfView(doc)
continuous = Bool(True)
dark = Bool(False)
tool = Int(0)  # what a drag does: 0 selects text, 1 moves the pages
query = Str("")
searched = [""]
password = Str("")
started = Bool(False)
side = [0]  # 0: thumbnails, 1: bookmarks
bookmarks = {"doc": None, "items": []}


def open_document(path):
    global doc
    doc = PdfDocument(path, wait=False)
    view.doc = doc
    query.value = ""


def thumbnails():
    for number in range(1, doc.pages + 1):
        if pdf_thumbnail(doc, number, size=110, selected=number == view.page, dark=dark.value):
            view.page = number


def outline():
    if bookmarks["doc"] is not doc:  # read once per document
        bookmarks.update(doc=doc, items=doc.outline)
    if not bookmarks["items"]:
        weak("No bookmark")
    for item in bookmarks["items"]:
        title = "    " * item["level"] + (item["title"] or "…")
        if selectable_label_clicked(False, title) and item["page"]:
            view.go_to(item["page"], item["y"] or 0)


def sidebar():
    if doc is None or not doc.pages:
        weak("No page")
        return

    def tabs():
        if selectable_label_clicked(side[0] == 0, "Pages"):
            side[0] = 0
        if selectable_label_clicked(side[0] == 1, "Signets"):
            side[0] = 1

    horizontal(tabs)
    weak(f"{doc.pages} pages")
    scroll_area_vertical(thumbnails if side[0] == 0 else outline)


def navigation():
    if button_clicked("◀"):
        view.previous_page()
    label(f"{view.page} / {view.pages}")
    if button_clicked("▶"):
        view.next_page()
    separator()
    if button_clicked("−"):
        view.zoom = view.zoom / 1.25
    label(f"{round(view.zoom * 100)} %")
    if button_clicked("+"):
        view.zoom = view.zoom * 1.25
    if button_clicked("Width"):
        view.fit = "width"
    if button_clicked("Page"):
        view.fit = "page"
    if button_clicked("100 %"):
        view.zoom = 1.0
    separator()
    if button_clicked("⟳"):
        view.rotate(90)
    checkbox(continuous, "Continuous")
    view.continuous = continuous.value
    checkbox(dark, "Dark")
    view.dark = dark.value
    segmented_control(tool, [(None, "Text"), (None, "Pan")])
    view.tool = "hand" if tool.value else "select"
    if view.busy:
        label("…")


def finding():
    label("🔍")
    text_edit_singleline(query, hint_text="Search the text")
    if query.value != searched[0]:  # a new text: a new search
        searched[0] = query.value
        view.search(query.value)
    if view.matches:
        label(f"{view.match_index} / {view.matches}" + ("+" if view.searching else ""))
    elif query.value:
        label("searching…" if view.searching else "no match")
    if button_clicked("▲"):
        view.previous_match()
    if button_clicked("▼"):
        view.next_match()
    if view.selected_text and button_clicked("Copy"):
        view.copy_selection()
    title = doc.info.get("title")
    if title:
        weak(title)


def toolbar():
    if doc is None:
        label("Start it with a PDF: python examples/pdf_viewer.py file.pdf (or drop one here)")
        return
    if doc.loading:
        label("Opening…")
        return
    if doc.error:
        label("⚠ " + doc.error)
        return
    if doc.needs_password:

        def ask():
            label("🔒 Password")
            text_edit_singleline(password)
            if button_clicked("Open") and not doc.unlock(password.value):
                password.value = ""
            view.doc = doc

        horizontal(ask)
        return
    horizontal(navigation)
    horizontal(finding)


def update_func(ctx):
    if not started.value:
        ctx.set_system_theme()
        started.value = True
    for path in dropped_files():
        open_document(str(path))
    sidebar_panel(sidebar, default_size=160, min_size=110)
    topbar_panel(toolbar)
    pdf_view(view, background="#7a7a7a" if ctx.is_dark_theme else "#c8c8c8")


if __name__ == "__main__":
    run_native(
        "PDF viewer",
        update_func,
        inner_width=1100,
        inner_height=800,
        fullsize_content=True,
        layout=dict(gap=8, rounding=12),
    )
