"""What a PDF says about itself, for the index of the Finder example: `pyegui.pdf_info`.

    info = read_pdf_info("contract.pdf")
    info["pages"], info["encrypted"], info["can_print"], info["title"], info["created"]

Nothing is raised: a file that cannot be read gives the reason in info["error"]. The file is read
and parsed by the Rust code of pyegui with the GIL released, so it can be called from any thread
without the window stuttering (nothing to install, no worker process: it replaces pypdf).

The permissions (can_print, can_copy...) are those of the PDF; only the viewers respect them. A
PDF that asks a password to be opened (needs_password) gives neither its pages nor its metadata,
the permissions are still read, they are not encrypted. One that has only an owner password opens
with an empty one, which is how every viewer opens it.
"""

import pyegui

# column -> its name for the window
PERMISSIONS = {
    "can_print": "impression",
    "can_copy": "copie du contenu",
    "can_modify": "modification",
    "can_annotate": "annotations",
    "can_fill_forms": "remplissage des formulaires",
    "can_assemble": "assemblage des pages",
}
TEXT_FIELDS = ("title", "author", "subject", "keywords", "creator", "producer")
COLUMNS = (
    "pages", "encrypted", "needs_password", *PERMISSIONS, *TEXT_FIELDS,
    "created", "modified", "pdf_version", "error",
)


def read_pdf_info(path):
    """the information of a PDF as a dict with the keys of COLUMNS (None where unknown). Safe to
    call from any thread."""
    info = pyegui.pdf_info(str(path))
    return {column: info.get(column) for column in COLUMNS}


def restrictions(info):
    """the names of what the PDF forbids, as a list (empty: everything is allowed or unknown)"""
    return [name for column, name in PERMISSIONS.items() if info.get(column) == 0]
