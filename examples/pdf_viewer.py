"""A PDF viewer: the thumbnails of the pages in the sidebar, the pages in the middle, a toolbar.

    python examples/pdf_viewer.py [file.pdf]

The pages are drawn by threads of pyegui (`craft-pdf`), the window never waits for them: a page
that is not ready shows a "..." and the view scrolls at once. Wheel to scroll, Ctrl + wheel to zoom
around the pointer, PageUp / PageDown / Home / End with the pointer over the pages.
Nothing to install: the metadata come from `pdf_info` / `PdfDocument.info`.
"""

import sys
import time

from pyegui import *

doc = PdfDocument(sys.argv[1]) if len(sys.argv) > 1 else None
view = PdfView(doc)
continuous = Bool(True)
password = Str("")
started = Bool(False)


def open_document(path):
    global doc
    doc = PdfDocument(path)
    view.doc = doc


def thumbnails():
    for number in range(1, doc.pages + 1):
        if pdf_thumbnail(doc, number, size=110, selected=number == view.page):
            view.page = number


def sidebar():
    if doc is None or not doc.pages:
        weak("No page")
        return
    weak(f"{doc.pages} pages")
    scroll_area_vertical(thumbnails)


def toolbar():
    def row():
        if doc is None:
            label("Start it with a PDF: python examples/pdf_viewer.py file.pdf")
            return
        if doc.error:
            label("⚠ " + doc.error)
            return
        if doc.needs_password:
            label("🔒 Password")
            text_edit_singleline(password)
            if button_clicked("Open") and not doc.unlock(password.value):
                password.value = ""
            view.doc = doc
            return
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
        checkbox(continuous, "Continuous")
        view.continuous = continuous.value
        if view.busy:
            label("…")
        title = doc.info.get("title")
        if title:
            weak(title)

    horizontal(row)


def update_func(ctx):
    if not started.value:
        ctx.set_system_theme()
        started.value = True
    for path in dropped_files():
        open_document(str(path))
    sidebar_panel(sidebar, default_size=150, min_size=110)
    topbar_panel(toolbar)
    pdf_view(view, background="#7a7a7a" if ctx.is_dark_theme else "#c8c8c8")


if __name__ == "__main__":
    run_native(
        "PDF viewer",
        update_func,
        inner_width=1000,
        inner_height=760,
        fullsize_content=True,
        layout=dict(gap=8, rounding=12),
    )
