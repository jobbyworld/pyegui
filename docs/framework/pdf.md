# PDF

What a PDF says about itself, read by the Rust code of pyegui (the `craft-pdf` reader): `pdf_info` gives the pages, the version, the metadata, the dates and what the file forbids, with the GIL released so it can be called from a thread. Nothing to install: it replaces pypdf. `PdfDocument` opens a file (in a thread with `wait=False`), reads the text of a page (`text`), its bookmarks (`outline`) and draws a page as a PNG (`render_png`). `PdfView` and `pdf_view` show its pages: scroll, zoom, fit to the width or the page, one page or all of them, rotation, dark colors, a search that marks its matches, a selection of text with the mouse that Ctrl+C copies, links that work and a hand to drag the pages. `pdf_thumbnail` shows one small page that can be clicked. The pages are drawn by threads and kept as textures, the window never waits for them. Only in a build with the `pdf` Cargo feature, which is on by default.

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

A big file takes a while to read: with `wait=False` the constructor returns at once, the file is
read by a thread and `loading` is True until it is open. The views and the thumbnails wait for it
by themselves, so the window does not freeze.

A file that cannot be opened does not raise: `error` tells why and `pages` is 0. A PDF that asks a
password has `needs_password` set: give it with `unlock` (or as `password`).

Properties (read only): `path`, `pages`, `loading`, `needs_password`, `error`, `info`, the dict of
`pdf_info` (title, author, dates, rights...) read from the open document, and `outline`, the
bookmarks of the document. `text(page)` reads the text of a page and `render_png` / `save_png`
draw one as a picture.

**Args**

- `path` (str): path of the PDF file
- `password` (str): the password to open it with, none by default (a PDF that has only an owner password opens without one)
- `wait` (bool): False reads the file in a thread, True by default

**Example:**

```python
doc = PdfDocument("contract.pdf")
if doc.needs_password and not doc.unlock("secret"):
    print("wrong password")
print(doc.pages, doc.info["title"], doc.page_size(1))

big = PdfDocument("archive.pdf", wait=False)   # the window does not wait
view = PdfView(big)
```


- `PdfDocument.error` (attribute): Why the file could not be read, None when it is open (or waits for a password)
- `PdfDocument.info` (attribute): What the document says about itself, the dict of `pdf_info`
- `PdfDocument.loading` (attribute): True while a thread reads the file (`wait=False`)
- `PdfDocument.needs_password` (attribute): True while the PDF asks a password to be opened
- `PdfDocument.outline` (attribute): The bookmarks of the document: a list of dicts with `title`, `level` (0 for the top ones),

  `page` (from 1, None when the bookmark goes nowhere), `y` (points from the top of the page, None
  when the destination gives none) and `uri` (a web address, for a bookmark that opens one).
  Give `page` and `y` to `PdfView.go_to`.

- `PdfDocument.page_size(page)` (method): Size of a page in points (1/72 inch), its rotation applied.


  **Args**

  - `page` (int): the number of the page, from 1

- `PdfDocument.pages` (attribute): Number of pages, 0 when the document is not open
- `PdfDocument.path` (attribute): Path of the file
- `PdfDocument.render_png(page, width=None, scale=1.0, dark=False)` (method): Draws a page and returns it as the bytes of a PNG picture.


  **Args**

  - `page` (int): the number of the page, from 1

  **Keyword args**

  - `width` (int): the width of the picture in pixels (its height follows)
  - `scale` (float): pixels per point when `width` is not given, 1.0 by default (72 dpi), 2.0 for 144 dpi
  - `dark` (bool): the dark colors, False by default

- `PdfDocument.save_png(page, path, width=None, scale=1.0, dark=False)` (method): Draws a page and writes it as a PNG file.


  **Args**

  - `page` (int): the number of the page, from 1
  - `path` (str): the file to write

  Keyword args: the same as `render_png`.

- `PdfDocument.text(page)` (method): The text of a page, as it is read in the page (a newline where the line changes, a space

  between the words). It replaces pypdf's `extract_text`.

  **Args**

  - `page` (int): the number of the page, from 1

- `PdfDocument.unlock(password)` (method): Opens a PDF that asks a password. Returns True when it is open, False when the password is

  wrong (or when the document did not need one: then it is open already).


## `PdfView`


The state of a PDF viewer: the document it shows, the page, the zoom, the scrolling, the
selection of text and the search.

Create it once, outside the update function, and draw it with `pdf_view`. In the continuous mode
(the default) the pages follow one another in a scroll area and `page` is the one at the top of
the window; in the single page mode only the current page is shown.

Properties: `doc` (a `PdfDocument` or None, write only), `page` (from 1, read it to show a page
number, write it to go to a page), `pages`, `zoom` (1.0 is 100%, 96 dpi; writing it turns `fit`
off), `fit` (`"width"`, `"page"` or `"none"`), `continuous`, `rotation` (0, 90, 180 or 270, the
look only), `dark` (draw the pages in dark colors), `tool` (`"select"` selects text with the
mouse, `"hand"` drags the pages), `busy` (pages are being drawn), `selected_text`, and `matches`,
`match_index`, `searching` for the search.

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
query = Str("")

def update_func(ctx):
    if button_clicked("Fit the page"):
        view.fit = "page"
    if text_edit_singleline(query).changed:
        view.search(query.value)
    label(f"page {view.page} / {view.pages}, {view.matches} matches")
    pdf_view(view)
```


- `PdfView.busy` (attribute): True while pages of the view are being drawn (their place shows a "…")
- `PdfView.clear_search()` (method): Ends the search and removes its marks
- `PdfView.clear_selection()` (method): Removes the selection
- `PdfView.continuous` (attribute): True to scroll through all the pages, False to show the current page only
- `PdfView.copy_selection()` (method): Puts the selected text in the clipboard (at the next frame)
- `PdfView.dark` (attribute): True to draw the pages in dark colors (light text on a dark sheet; the images are kept)
- `PdfView.doc` (attribute): The document shown (write only): a `PdfDocument`, or None to show nothing. Giving the document

  that is shown already changes nothing.

- `PdfView.fit` (attribute): `"width"`, `"page"` or `"none"`
- `PdfView.go_to(page, y=0.0)` (method): Goes to a page, from 1, and to a height in it: points from its top (0 shows the top of the

  page, a bookmark or a link of the document gives its own).

- `PdfView.match_index` (attribute): The current match, from 1 (0 when there is none)
- `PdfView.matches` (attribute): Number of matches found so far
- `PdfView.next_match()` (method): Goes to the next match (the first one after the last)
- `PdfView.next_page()` (method): Goes to the next page (one page further, the last one stays)
- `PdfView.page` (attribute): The page at the top of the window, from 1. Writing it goes to that page.
- `PdfView.pages` (attribute): Number of pages of the document shown, 0 without one
- `PdfView.previous_match()` (method): Goes to the previous match (the last one before the first)
- `PdfView.previous_page()` (method): Goes to the previous page
- `PdfView.rotate(degrees=90)` (method): Turns the pages by a number of degrees (90 clockwise, -90 the other way)
- `PdfView.rotation` (attribute): How the pages are turned on the screen, in degrees clockwise: 0, 90, 180 or 270. It changes

  the look only, not the file.

- `PdfView.search(text, accents=False)` (method): Looks for a text in all the pages, in a thread. The matches are marked in the pages as they are

  found and the view goes to the first one; `matches` counts them and `next_match` and
  `previous_match` go from one to another. An empty text ends the search.

  **Args**

  - `text` (str): the text to find, in a page it may continue on the next line
  - `accents` (bool): True tells "é" from "e" and "É" from "é", False (default) ignores the case and the accents

- `PdfView.searching` (attribute): True while the search reads pages
- `PdfView.selected_text` (attribute): The text that is selected, from the pages whose text is read (the ones that were shown)
- `PdfView.tool` (attribute): `"select"` (default): a drag selects text, a double click a word, a triple click a line, Ctrl+C

  copies. `"hand"`: a drag moves the pages.

- `PdfView.zoom` (attribute): The zoom, 1.0 is 100% (96 dpi). The value that is used at the last frame when `fit` is on.

  Writing it sets `fit` to `"none"`.


## `pdf_view(view, **kwargs)`

Shows the pages of a PDF in a viewer that takes all the space available (or the height given).
The pages are drawn by threads: the window never waits for them, a page that is not ready shows
a "…" (or a blurry picture of itself while a sharper one is drawn).

The wheel and the scroll bars scroll, Ctrl + wheel (or a pinch) zooms around the pointer, and
with the pointer over the viewer PageUp / PageDown go a page (a window in the continuous mode)
further, Home and End to the first and the last page, Ctrl + and Ctrl - zoom, Ctrl 0 fits the
width. A drag selects text (see `PdfView.tool`), a double click a word, a triple click a line,
Ctrl+C copies it, Escape removes the selection. The links of the pages work: a click goes to the
page or opens the address in the browser. The matches of `PdfView.search` are marked.

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
- `dark` (bool): draws the page in dark colors, False by default

**Example:**

```python
def sidebar():
    def thumbnails():
        for p in range(1, doc.pages + 1):
            if pdf_thumbnail(doc, p, selected=p == view.page):
                view.page = p
    scroll_area_vertical(thumbnails)
```
