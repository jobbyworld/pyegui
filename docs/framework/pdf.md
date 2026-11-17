# PDF

What a PDF says about itself, read by the Rust code of pyegui (the `craft-pdf` reader): `pdf_info` gives the pages, the version, the metadata, the dates and what the file forbids, with the GIL released so it can be called from a thread. Nothing to install: it replaces pypdf. `PdfDocument` opens a file, `PdfView` and `pdf_view` show its pages (scroll, zoom, fit to the width or the page, one page or all of them) and `pdf_thumbnail` shows one small page that can be clicked. The pages are drawn by threads and kept as textures, the window never waits for them. Only in a build with the `pdf` Cargo feature, which is on by default.

## `pdf_info(path)`

Reads what a PDF says about itself: its page count, version, metadata, dates and what it
forbids, without any other library (it replaces pypdf).

Returns a dict that always has the same keys, None where something is unknown or missing:
`pages`, `encrypted` (0 or 1), `needs_password` (0 or 1), the rights `can_print`, `can_copy`,
`can_modify`, `can_annotate`, `can_fill_forms`, `can_assemble` (1 allowed, 0 forbidden), the
texts `title`, `author`, `subject`, `keywords`, `creator`, `producer`, the dates `created` and
`modified` (seconds since the epoch), `pdf_version` (``"1.7"``) and `error`.

Nothing is raised: a file that cannot be read gives the reason in `error`. The file is read and
parsed with the GIL released, so it can be called from a thread, and it is closed at once (it
can be moved or deleted afterwards on Windows).

The rights are those written in the PDF; only the viewers respect them. A PDF that asks a
password to open (`needs_password`) gives neither its pages nor its metadata, but its rights are
still read, they are not encrypted. A PDF with an owner password only opens with an empty one,
as in every viewer.

**Args**

- `path` (str): path of the PDF file

**Example:**

```python
info = pdf_info("contract.pdf")
if info["error"] is None:
    print(info["pages"], info["title"], info["can_print"])
```


## `PdfDocument`


An open PDF file.

The file is read at once (it is not kept open, so it can be moved or deleted afterwards on Windows)
and its pages are drawn on demand by `pdf_view` and `pdf_thumbnail`. A document is a state object:
create it once, outside the update function, and share it between the views and the thumbnails.

A file that cannot be opened does not raise: `error` tells why and `pages` is 0. A PDF that asks a
password has `needs_password` set: give it with `unlock` (or as `password`).

Properties (read only): `path`, `pages`, `needs_password`, `error`, and `info`, the dict of
`pdf_info` (title, author, dates, rights...) read from the open document.

**Args**

- `path` (str): path of the PDF file
- `password` (str): the password to open it with, none by default (a PDF that has only an owner password opens without one)

**Example:**

```python
doc = PdfDocument("contract.pdf")
if doc.needs_password and not doc.unlock("secret"):
    print("wrong password")
print(doc.pages, doc.info["title"], doc.page_size(1))
```


- `PdfDocument.error` (attribute): Why the file could not be read, None when it is open (or waits for a password)
- `PdfDocument.info` (attribute): What the document says about itself, the dict of `pdf_info`
- `PdfDocument.needs_password` (attribute): True while the PDF asks a password to be opened
- `PdfDocument.page_size(page)` (method): Size of a page in points (1/72 inch), its rotation applied.


  **Args**

  - `page` (int): the number of the page, from 1

- `PdfDocument.pages` (attribute): Number of pages, 0 when the document is not open
- `PdfDocument.path` (attribute): Path of the file
- `PdfDocument.unlock(password)` (method): Opens a PDF that asks a password. Returns True when it is open, False when the password is

  wrong (or when the document did not need one: then it is open already).


## `PdfView`


The state of a PDF viewer: the document it shows, the page, the zoom and the scrolling.

Create it once, outside the update function, and draw it with `pdf_view`. In the continuous mode
(the default) the pages follow one another in a scroll area and `page` is the one at the top of
the window; in the single page mode only the current page is shown.

Properties: `doc` (a `PdfDocument` or None, write only), `page` (from 1, read it to show a page
number, write it to go to a page), `pages`, `zoom` (1.0 is 100%, 96 dpi; writing it turns `fit`
off), `fit` (`"width"`, `"page"` or `"none"`), `continuous`, `busy` (pages are being drawn).

**Args**

- `doc` (PdfDocument): the document to show, none by default
- `fit` (str): `"width"` (default) fits the widest page to the window, `"page"` fits a whole page, `"none"` uses `zoom`
- `zoom` (float): the zoom when `fit` is `"none"`, 1.0 by default
- `continuous` (bool): scroll through all the pages (True, default) or show one page
- `page` (int): the first page shown, 1 by default

**Example:**

```python
doc = PdfDocument("contract.pdf")
view = PdfView(doc)

def update_func(ctx):
    if button_clicked("Fit the page"):
        view.fit = "page"
    label(f"page {view.page} / {view.pages}")
    pdf_view(view)
```


- `PdfView.busy` (attribute): True while pages of the view are being drawn (their place shows a "…")
- `PdfView.continuous` (attribute): True to scroll through all the pages, False to show the current page only
- `PdfView.doc` (attribute): The document shown (write only): a `PdfDocument`, or None to show nothing. Giving the document

  that is shown already changes nothing.

- `PdfView.fit` (attribute): `"width"`, `"page"` or `"none"`
- `PdfView.next_page()` (method): Goes to the next page (one page further, the last one stays)
- `PdfView.page` (attribute): The page at the top of the window, from 1. Writing it goes to that page.
- `PdfView.pages` (attribute): Number of pages of the document shown, 0 without one
- `PdfView.previous_page()` (method): Goes to the previous page
- `PdfView.zoom` (attribute): The zoom, 1.0 is 100% (96 dpi). The value that is used at the last frame when `fit` is on.

  Writing it sets `fit` to `"none"`.


## `pdf_view(view, **kwargs)`

Shows the pages of a PDF in a viewer that takes all the space available (or the height given).
The pages are drawn by threads: the window never waits for them, a page that is not ready shows
a "…" (or a blurry picture of itself while a sharper one is drawn).

The wheel and the scroll bars scroll, Ctrl + wheel (or a pinch) zooms around the pointer, and
with the pointer over the viewer PageUp / PageDown go a page (a window in the continuous mode)
further, Home and End to the first and the last page.

**Args**

- `view` (PdfView): the state of the viewer

**Keyword args**

- `height` (float): the height, the available one by default
- `margin` (float): the space around the pages, 12 by default
- `gap` (float): the space between two pages, 12 by default
- `background` (tuple | str): the color behind the pages, the one of the panel by default

**Example:**

```python
view = PdfView(PdfDocument("contract.pdf"))

def update_func(ctx):
    pdf_view(view, background="#808080")
```


## `pdf_thumbnail(doc, page, **kwargs)`

Shows one page of a document as a small picture, and returns True when it is clicked. Draw
the thumbnails of the pages in a vertical scroll area next to a `pdf_view` and go to the page that
is clicked. The pictures are drawn by threads like the ones of the viewer, only for the thumbnails
that are on screen.

**Args**

- `doc` (PdfDocument): the document
- `page` (int): the number of the page, from 1

**Keyword args**

- `size` (float): the longest side of the picture, 96 by default
- `selected` (bool): frames the thumbnail as the page shown, False by default
- `label` (bool): writes the number of the page under the picture, True by default

**Example:**

```python
def sidebar():
    def thumbnails():
        for p in range(1, doc.pages + 1):
            if pdf_thumbnail(doc, p, selected=p == view.page):
                view.page = p
    scroll_area_vertical(thumbnails)
```
