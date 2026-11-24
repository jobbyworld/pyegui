Examples
===========

widget gallery
--------------

Most widgets, one after the other.

.. literalinclude:: ../examples/widget_gallery.py
   :language: python
   :linenos:

finder
-------------

A mini Finder, the reference for a real desktop layout: sidebar, toolbar, file views, preview,
a dock of panes, tags, PDF viewer, terminals.

.. literalinclude:: ../examples/finder.py
   :language: python
   :linenos:

The Finders keep their tags, favorites, notes and index (files and PDF page counts) in one SQLite file,
documented in ``docs/finder-database.md`` of the repository.
