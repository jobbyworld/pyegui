"""Markdown editor: the source on the left, the live preview on the right, mermaid diagrams included.

Edit the text and the preview follows. Code blocks marked mermaid are drawn as diagrams.
"""

import sys

from pyegui import *

# acrylic on Windows, liquid glass on macOS, no backdrop elsewhere
BACKDROP = {"win32": "acrylic", "darwin": "liquid_glass"}.get(sys.platform)

LIGHT, DARK, SYSTEM = 0, 1, 2

source = Str('''# Markdown and mermaid

Some **bold**, *italic* and `code`, a [link](https://github.com/emilk/egui).

- edit the text on the left
- the preview follows

```mermaid
flowchart LR
    A[Write] --> B{Looks good?}
    B -->|Yes| C[Publish]
    B -->|No| A
```

```mermaid
sequenceDiagram
    participant U as User
    participant A as App
    U->>A: Type
    A-->>U: Preview
```

```python
print("code blocks stay code")
```
''')
theme = Int(SYSTEM)
started = Bool(False)


def source_ui():
    code_editor(source, desired_width=float("inf"), desired_rows=30)


def preview_ui():
    markdown(source.value, scrollable=True, id="preview")


dock = Dock(
    DockContainer(
        "horizontal",
        [DockPane("Source", source_ui), DockPane("Preview", preview_ui)],
    ),
    glass=True,
)


def apply_theme(ctx):
    if theme.value == LIGHT:
        ctx.set_light_theme()
    elif theme.value == DARK:
        ctx.set_dark_theme()
    else:
        ctx.set_system_theme()
    # the glass style depends on the theme
    ctx.set_glass_style()


def top_bar(ctx):
    def bar():
        before = theme.value
        selectable_value(theme, LIGHT, "Light")
        selectable_value(theme, DARK, "Dark")
        selectable_value(theme, SYSTEM, "System")
        if theme.value != before:
            apply_theme(ctx)

    horizontal(bar)


def update_func(ctx):
    if not started.value:
        apply_theme(ctx)
        started.value = True

    top_panel("top", lambda: top_bar(ctx), inner_margin=8, show_separator_line=False)
    dock_area(dock)


if __name__ == "__main__":
    run_native("Markdown", update_func, inner_width=1100, inner_height=700, backdrop=BACKDROP)
