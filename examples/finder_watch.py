"""Watching folders for changes, for the Finder example, with the `watchdog` library.

    pip install watchdog

    watcher = Watcher()
    watcher.add("/docs")            # the folder and all its sub-folders
    ...
    for event in watcher.recent():  # newest first: {"time", "kind", "path", "root", "text"}
        print(event["text"])

watchdog asks the system to tell what happens (ReadDirectoryChangesW on Windows, FSEvents on macOS,
inotify on Linux) and calls the handler of this module from its own thread: nothing reads the
folders again and again. The handler keeps the creations, deletions, modifications and renames in a
list the window reads, newest first.
"""

import threading
import time
from pathlib import Path

from watchdog.events import FileSystemEventHandler
from watchdog.observers import Observer

CREATED, MODIFIED, DELETED, RENAMED, PROBLEM = "créé", "modifié", "supprimé", "renommé", "problème"
MAX_EVENTS = 2000  # kept in memory, the oldest are dropped
SAME_WRITE = 0.5  # seconds: the "modified" events of one write on the disk are told once


class Watcher:
    def __init__(self):
        self.lock = threading.Lock()
        self.roots = {}  # path -> {"watch": the watchdog handle, "events": count}
        self.events = []  # oldest first
        self.version = 0  # grows at every event, so a view can tell it has something new
        self.observer = None  # started with the first folder
        self.last_modified = {}  # path -> time of the last "modified" told

    def paths(self):
        with self.lock:
            return list(self.roots)

    def watching(self, path):
        with self.lock:
            return Path(path) in self.roots

    def count(self, path):
        with self.lock:
            return self.roots.get(Path(path), {}).get("events", 0)

    def add(self, path):
        """starts watching a folder and everything in it; a problem is told in the events"""
        path = Path(path)
        with self.lock:
            if path in self.roots:
                return
        if self.observer is None:
            self.observer = Observer()
            self.observer.daemon = True
            self.observer.start()
        try:
            watch = self.observer.schedule(Handler(self, path), str(path), recursive=True)
        except OSError as error:  # no such folder, no right, too many watches on Linux...
            self.log(PROBLEM, path, path, f"{path} : impossible de surveiller ({error})")
            return
        with self.lock:
            self.roots[path] = {"watch": watch, "events": 0}

    def remove(self, path):
        path = Path(path)
        with self.lock:
            state = self.roots.pop(path, None)
        if state and self.observer:
            self.observer.unschedule(state["watch"])

    def stop(self):
        """stops watching everything. Call it before the program ends: on Windows watchdog starts
        a thread that is not a daemon, and Python waits for it forever when the window closes"""
        observer, self.observer = self.observer, None
        with self.lock:
            self.roots.clear()
        if observer:
            observer.stop()
            observer.join(timeout=3)

    def recent(self, limit=500):
        """the latest events, newest first"""
        with self.lock:
            return self.events[-limit:][::-1]

    def clear(self):
        with self.lock:
            self.events.clear()
            self.version += 1

    def log(self, kind, path, root, text):
        with self.lock:
            self.events.append(
                {"time": time.time(), "kind": kind, "path": str(path), "root": str(root), "text": text}
            )
            del self.events[:-MAX_EVENTS]
            if root in self.roots:
                self.roots[root]["events"] += 1
            self.version += 1

    def modified_again(self, path):
        """True if this file was already told as modified a moment ago (a write makes several)"""
        now = time.monotonic()
        with self.lock:
            last = self.last_modified.get(path)
            self.last_modified[path] = now
            if len(self.last_modified) > 5000:  # forget the old ones
                self.last_modified = {p: t for p, t in self.last_modified.items() if now - t < 60}
        return last is not None and now - last < SAME_WRITE


class Handler(FileSystemEventHandler):
    """turns the events of watchdog into lines of the log, for one watched folder"""

    def __init__(self, watcher, root):
        self.watcher = watcher
        self.root = root

    def relative(self, path):
        try:
            return str(Path(path).relative_to(self.root))
        except ValueError:  # moved from or to a place outside the folder
            return str(path)

    def on_any_event(self, event):
        kind = event.event_type
        if kind == "created":
            self.tell(CREATED, event.src_path, self.relative(event.src_path))
        elif kind == "deleted":
            self.tell(DELETED, event.src_path, self.relative(event.src_path))
        elif kind == "modified":
            # a folder changes with its content: not told; a file is told once per write
            if not event.is_directory and not self.watcher.modified_again(event.src_path):
                self.tell(MODIFIED, event.src_path, self.relative(event.src_path))
        elif kind == "moved":
            text = f"{self.relative(event.src_path)} → {self.relative(event.dest_path)}"
            self.tell(RENAMED, event.dest_path, text)
        # opened, closed...: only reading, nothing changed

    def tell(self, kind, path, text):
        self.watcher.log(kind, path, self.root, f"{kind} : {text}")
