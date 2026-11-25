# Text, links and markdown

Functions that show text. They are called every frame, in the order the text appears.

## `heading(text)`

Show large text

**Example:**

```python
heading("hello")
```


## `label(text, size=None, truncate=False, selectable=None)`

Show some text.

**Args**

- `text` (str): the text
- `size` (float): font size of this text only. Default is the size of the body text
- `truncate` (bool): keep the text on one line and end it with … when it is too long for the width, instead of wrapping it. Default is False
- `selectable` (bool): whether the user can select the text to copy it. Default is True. Set it to False when the label is inside an interactive() area, otherwise clicking the text starts a text selection instead of reaching the click of the area around it

**Example:**

```python
label("some text")
label("📁", size=48)
label(file_name, truncate=True)
```


## `small(text)`

Show small text.

**Example:**

```python
small("hello")
```


## `strong(text)`

Show text that stand out a bit (e.g. slightly brighter).

**Example:**

```python
strong("hello")
```


## `weak(text)`

Show text that is weaker (fainter color).

**Example:**

```python
weak("hello")
```


## `monospace(text)`

Show monospace (fixed width) text.

**Example:**

```python
monospace("hello")
```


## `code(text)`

Show text as monospace with a gray background.

**Example:**

```python
code("print(42 + 27)")
```


## `code_view(text, language='', **kwargs)`

Shows source code with syntax highlighting and line numbers, in a scroll area. The text can be
selected and copied.

Highlighted languages: Python, JavaScript and TypeScript, Rust, shell (bash, zsh, and
PowerShell as shell), C and C++, JSON, SQL, TOML and YAML, reStructuredText ("rst": titles, directives, comments, lists, fields and inline markup) and Gherkin ("feature": the keywords of the blocks and of the steps in English and French, tags, comments, tables, doc strings and the parameters of an outline). Another language is shown plain.

**Args**

- `text` (str): the code
- `language` (str): the language or the extension of the file: "python", "py", ".rs", "ps1"...

**Keyword args**

- `line_numbers` (bool): the numbers of the lines in a column at the left. Default is True
- `id` (str): id of the scroll area, needed if there are several code views. Default is "code_view"

**Example:**

```python
code_view(Path("app.py").read_text(), "py")
```


## `colored_label(color, text)`

Shows a text in the given color.

**Args**

- `color` (color): (r, g, b), (r, g, b, a) or "#rrggbb[aa]"
- `text` (str): the text

**Example:**

```python
colored_label((220, 50, 50), "Error")
colored_label("#22c55e", "Saved")
```


## `badge(text, color=None)`

A small colored label, for a status or a tag.

**Args**

- `text` (str): the text
- `color` (color): the color of the text, the background is a tint of it. Default is the accent

**Example:**

```python
badge("Done", "#22c55e")
badge("Blocked", (239, 68, 68))
```


## `hyperlink(url)`

A clickable hyperlink

**Example:**

```python
hyperlink("https://github.com/emilk/egui")
```


## `hyperlink_to(label, url)`

A clickable hyperlink with label

**Example:**

```python
hyperlink_to("egui on GitHub", "https://www.github.com/emilk/egui/")
```


## `link_clicked(label)`

Clickable text, that looks like a hyperlink.
To link to a web page, use hyperlink or hyperlink_to.

**Example:**

```python
if link_clicked("egui on GitHub"):
  print("clicked on a fake link")
```


## `label_response(text, size=None)`

Shows a text that can be clicked and returns a Response: clicked, double_clicked,
secondary_clicked and hovered.

**Args**

- `text` (str): the text
- `size` (float): font size of the text. Default is the size of the body text

**Example:**

```python
response = label_response("📁", size=48)
if response.double_clicked:
    open_folder()
```


## `markdown(text, **kwargs)`

Shows markdown text: headings, lists, tables, quotes, code blocks, links and images.

**Keyword args**

- `scrollable` (bool): show the text in a scroll area, drawing only the visible part. Faster for long texts
- `id` (str): id of the scroll area, needed if there are several scrollable markdown texts
- `max_image_width` (int): max width of the images
- `default_width` (int): width of the text, default is the available width
- `max_width` (float): max width of the whole document, in points. A wider area leaves the rest empty. Default is the available width
- `align` (str): where a document limited by ``max_width`` is placed: "left" (default), "center" or "right"
- `table_width` (str): how the tables are drawn. "scroll" (default): their natural width, with a horizontal scroll if they are wider than the document. "reformat": the cells wrap their text so that the table fits the document (the width of ``max_width``). "full_width": the same, but the table fits the whole available width, out of the column of ``max_width``. The cells keep bold, code, links and images; the alignment of the columns is not applied
- code_width, image_width, mermaid_width (str): what these blocks do when ``max_width`` limits the document. "reformat" (default): the block is fitted to the column of ``max_width``. "full_width": the block takes the whole available width, out of the column
- `mermaid_theme` (str): theme of the mermaid diagrams: "default", "dark", "forest", "neutral" or "base".
- Default follows the theme of the app
- `open_links` (bool): a click on a link opens it in the browser. False: it is only returned, so that the app opens it itself, e.g. a link to another markdown file. Default is True

**Returns**

- str | None: the url of the link clicked during this frame, as written in the text (a relative
- path stays relative), else None

Code blocks marked ``mermaid`` are drawn as diagrams: flowcharts, sequence, class, state, ER, pie,
gantt and more. A diagram that does not parse shows its error and its source.

With a ``table_width`` other than "scroll", or a "full_width" block, the text is cut around those blocks.
Only the blocks at the top level of the text are concerned (not those in a list or a quote), a reference
link must be defined in the same part of the text as its use, and with ``scrollable`` the whole text is
laid out, not only its visible part (slower for very long texts).

**Example:**

```python
markdown('''
# Title
Some **bold** and *italic* text, a [link](https://github.com/emilk/egui).

- item 1
- item 2

~~~python
print("hello")
~~~

~~~mermaid
flowchart LR
    A[Start] --> B{OK?}
    B -->|Yes| C[Done]
~~~
''')
```


## `preload_markdown_fonts()`

Loads, on a thread of its own, the system fonts that the mermaid diagrams of `markdown` need.

Without it the first diagram loads them (about two seconds, on a thread too: the text of the diagram
shows a placeholder until then). Call it once the app is running to have them ready before the first
markdown with a diagram is opened. Does nothing the second time.

**Example:**

```python
preload_markdown_fonts()  # at the start of the app
```


## `markdown_to_pdf(text, path=None, **kwargs)`

Turns a markdown text into a PDF, drawn like `markdown()` is.

The text is drawn by the same code as the viewer: headings, lists, tables, quotes, code blocks with their
colors, links, images and mermaid diagrams. The pages are cut between two lines of text, and a heading
is never alone at the bottom of a page. The text stays text (vector, selectable), the headings become
the bookmarks of the file and the links become links of the PDF. No window is needed and no ui is
involved: it can be called from a thread.

**Args**

- `text` (str): the markdown
- `path` (str | None): the file to write. None: the bytes of the PDF are returned

**Keyword args**

- `theme` (str): "light" (the default), "dark" (dark pages) or "print" (black text, white paper)
- `title` (str): title of the document, of its cover and of `{title}` in the header and the footer.
- Default is the first heading of level 1
- `author` (str): author written in the properties of the PDF
- `cover` (dict | bool): a cover page: `dict(title=..., subtitle=..., author=..., date=...)`, every key is
- optional. True is a cover with the title only
- `toc` (bool | str | dict): a table of contents with the page of every heading, linked to the pages.
- A str is its title, a dict is `dict(title="Contents", depth=3)`. Default is no table of contents
- `bookmarks` (bool): the headings as the bookmarks of the PDF. Default is True
- `header` (str | dict): text at the top of the pages, or `dict(left=..., center=..., right=...)`
- `footer` (bool | str | dict): same as `header`. `{page}`, `{pages}` and `{title}` are replaced.
- Default is "{page} / {pages}" in the center, False for none. No header and no footer on the cover
- `page_size` (str | tuple): "A3", "A4" (the default), "A5", "Letter", "Legal" or (width, height) in mm
- `margin` (float): margin of the pages in mm. Default is 20
- `base_dir` (str): the folder that the relative paths of the images are read from

**Returns**

- bytes | None: the PDF when `path` is None

**Example:**

```python
markdown_to_pdf(Path("README.md").read_text(encoding="utf-8"), "README.pdf",
                theme="light", cover=dict(subtitle="Version 1.0", author="Joachim"),
                toc=True, footer="{title}  -  {page} / {pages}", base_dir=".")
```
