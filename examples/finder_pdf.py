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

import json
import logging
import os
import queue
import subprocess
import sys
import threading

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


def read_pdf_info_here(path):
    """the information of a PDF as a dict with the keys of COLUMNS (None where unknown), read by
    this process"""
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


# pypdf is plain Python: read in a thread of the app it holds the GIL and the window stutters while a
# big PDF is read. The files are read by other processes instead, which the window does not wait for.
WORKERS = 2  # one for the index, one for the preview
_idle = queue.Queue()
_count = [0]
_lock = threading.Lock()


def _python():
    """python.exe: the pythonw.exe of a window has no standard output to answer on"""
    exe = sys.executable
    if os.path.basename(exe).lower() == "pythonw.exe":
        console = os.path.join(os.path.dirname(exe), "python.exe")
        if os.path.exists(console):
            return console
    return exe


def _start_worker():
    flags = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0
    return subprocess.Popen(
        [_python(), "-X", "utf8", os.path.abspath(__file__), "--worker"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, creationflags=flags,
    )


def _take_worker():
    try:
        return _idle.get_nowait()
    except queue.Empty:
        pass
    with _lock:
        if _count[0] < WORKERS:
            _count[0] += 1
            try:
                return _start_worker()
            except OSError:
                _count[0] -= 1
                return None
    return _idle.get()  # all are busy: wait for one


def read_pdf_info(path):
    """the information of a PDF as a dict with the keys of COLUMNS (None where unknown), read by a
    worker process so that the window does not stutter. Safe to call from any thread."""
    worker = _take_worker()
    if worker is None:
        return read_pdf_info_here(path)
    try:
        worker.stdin.write(json.dumps(str(path)).encode() + b"\n")
        worker.stdin.flush()
        answer = worker.stdout.readline()
        if not answer:
            raise OSError("worker gone")
        info = json.loads(answer)
        _idle.put(worker)
        return {column: info.get(column) for column in COLUMNS}
    except Exception:  # the worker died: this file is read here, the next ones get a new worker
        worker.kill()
        with _lock:
            _count[0] -= 1
        return read_pdf_info_here(path)


def _serve():
    """the loop of a worker process: a path per line on the input, the information on the output"""
    for line in sys.stdin.buffer:
        info = read_pdf_info_here(json.loads(line))
        sys.stdout.buffer.write(json.dumps(info).encode() + b"\n")
        sys.stdout.buffer.flush()


if __name__ == "__main__" and "--worker" in sys.argv:
    _serve()


def restrictions(info):
    """the names of what the PDF forbids, as a list (empty: everything is allowed or unknown)"""
    return [name for column, (_, name) in PERMISSIONS.items() if info.get(column) == 0]
