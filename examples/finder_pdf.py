"""What a PDF says about itself, read with pypdf, for the index of the Finder example.

    pip install "pypdf[crypto]"

    info = read_pdf_info("contract.pdf")
    info["pages"], info["encrypted"], info["can_print"], info["title"], info["created"]

Nothing is raised: a file that cannot be read gives the reason in info["error"]. The file is read
through a stream that is closed at once, so it can be deleted or moved afterwards (Windows).

The permissions (can_print, can_copy...) are those of the PDF; only the viewers respect them. A
PDF that asks a password to be opened (needs_password) gives neither its pages nor its metadata,
the permissions are still read, they are not encrypted. One that has only an owner password opens
with an empty one, which is how every viewer opens it.
"""

import logging

from pypdf import PdfReader
from pypdf.constants import UserAccessPermissions as Access

logging.getLogger("pypdf").setLevel(logging.ERROR)  # a damaged PDF is told in info["error"], not in the console

# column -> the right of the PDF it tells, and its name for the window
PERMISSIONS = {
    "can_print": (Access.PRINT, "impression"),
    "can_copy": (Access.EXTRACT, "copie du contenu"),
    "can_modify": (Access.MODIFY, "modification"),
    "can_annotate": (Access.ADD_OR_MODIFY, "annotations"),
    "can_fill_forms": (Access.FILL_FORM_FIELDS, "remplissage des formulaires"),
    "can_assemble": (Access.ASSEMBLE_DOC, "assemblage des pages"),
}
TEXT_FIELDS = {
    "title": "/Title", "author": "/Author", "subject": "/Subject", "keywords": "/Keywords",
    "creator": "/Creator", "producer": "/Producer",
}
COLUMNS = (
    "pages", "encrypted", "needs_password", *PERMISSIONS, *TEXT_FIELDS,
    "created", "modified", "pdf_version", "error",
)


def clean(value):
    """a metadata text as a short plain string, or None if empty"""
    text = str(value).replace("\x00", "").strip() if value is not None else ""
    return text or None


def timestamp(read):
    """a date of the metadata as seconds since the epoch, or None (missing or malformed)"""
    try:
        date = read()
        return date.timestamp() if date else None
    except Exception:
        return None


def read_pdf_info(path):
    """the information of a PDF as a dict with the keys of COLUMNS (None where unknown)"""
    info = dict.fromkeys(COLUMNS)
    try:
        with open(path, "rb") as stream:
            reader = PdfReader(stream, strict=False)
            info["pdf_version"] = clean(reader.pdf_header.replace("%PDF-", ""))
            info["encrypted"] = int(reader.is_encrypted)
            info["needs_password"] = 0
            if reader.is_encrypted:
                opened = int(reader.decrypt(""))  # 0: not opened, 1: user, 2: owner password
                info["needs_password"] = int(opened == 0)
            rights = reader.user_access_permissions if reader.is_encrypted else None
            for column, (flag, _) in PERMISSIONS.items():
                info[column] = 1 if rights is None else int(bool(rights & flag))
            if info["needs_password"]:
                return info
            info["pages"] = len(reader.pages)
            meta = reader.metadata
            if meta:
                for column, name in TEXT_FIELDS.items():
                    info[column] = clean(meta.get(name))
                info["created"] = timestamp(lambda: meta.creation_date)
                info["modified"] = timestamp(lambda: meta.modification_date)
    except Exception as error:  # damaged file, unsupported encryption, AES without cryptography...
        info["error"] = f"{type(error).__name__}: {error}"[:300]
    return info


def restrictions(info):
    """the names of what the PDF forbids, as a list (empty: everything is allowed or unknown)"""
    return [name for column, (_, name) in PERMISSIONS.items() if info.get(column) == 0]
