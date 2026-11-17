# PDF

What a PDF says about itself, read by the Rust code of pyegui (the `craft-pdf` reader): `pdf_info` gives the pages, the version, the metadata, the dates and what the file forbids, with the GIL released so it can be called from a thread. Nothing to install: it replaces pypdf. Only in a build with the `pdf` Cargo feature, which is on by default.

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
