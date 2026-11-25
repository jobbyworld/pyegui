# Meta properties: what is left

Done in `finder-rs` (PR #42): definitions and typed values, closed / open / multiple lists, inheritance and folder
summaries, columns in the list and tree views, formula properties, rules of the indexation. See
[finder-database.md](finder-database.md) for the tables.

## To do

- **`finder.py`**: only the schema (`meta_defs`, `meta_values`, `meta_rules`) is declared. Port a `MetaStore` (the
  `MetaStore` stays in `examples/finder.py` like `TagStore`), the "Propriétés" pane, the window of the definitions, the
  property columns (`extra_columns`) and the rules. The logic to reuse is in `pyegui_core::meta` (`resolve`,
  `eval_formula`, `Pattern`, `parse_input`): expose it with `#[pyfunction]`s or rewrite it in Python.
- **Several items selected**: the pane asks for a single item. Show the common value or "mixte" and set a value on all
  of them (like the tags pane does).
- **Edit in the cells**: the cells of the property columns are read-only, the values are edited in the pane.
- **Rules**
  - run when a folder is only visited (`index_children`), not just after an indexation or with "Appliquer";
  - a level made of a literal and a name (`P-{num}`), and a name used on several levels;
  - a preview of what a rule would write before it is applied.
- **Formulas**: conditions (`{a if b else c}`), a date with a format (`{modified:%Y}`), the properties of the parent
  folder (`{parent.Client}`).
- **Types not done**: `multichoice` is `choice` with `multiple`; still to do `color`, `daterange` (sort by start, filter
  by overlap, summary as the envelope), `rating`, `url`, `link` to another file, `duration`.
- **Lists shared** by several properties (a table `meta_lists`): the ids of the options already make it possible
  without a migration of the values.
- **Moving or renaming** a file outside the app loses its values (they are attached to the path, like the tags). Check
  that a move made in the app updates `meta_values` as it does `items`.
- **Right-click on a header** to show or hide the property columns (today: the checkboxes of the pane).
