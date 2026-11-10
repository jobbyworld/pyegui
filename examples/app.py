"""Atelier: a demo application with a menu bar, a sidebar, tabs and a status bar.

The View menu switches the theme, the tab style and the glass style.
"""

import math
import sys

from pyegui import *

BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)

LIGHT, DARK, SYSTEM = 0, 1, 2
PAGES = ["Overview", "Projects", "Team", "Settings"]

# state

theme = Int(DARK if "dark" in sys.argv else LIGHT if "light" in sys.argv else SYSTEM)
glass = Bool("noglass" not in sys.argv)
pill = Bool("classic" not in sys.argv)
page = Int(0)
show_sidebar = Bool(True)
about_open = Bool(False)
status = Str("Ready")
search = Str("")
selected_task = Int(-1)
started = Bool(False)

name = Str("Ada Lovelace")
email = Str("ada@example.com")
notifications = Bool(True)
weekly_report = Bool(False)
accent = RGBA(0.23, 0.51, 0.96, 1.0)

tasks = [
    ["Design the landing page", "Ada", "In progress", "High"],
    ["Write the release notes", "Grace", "To do", "Medium"],
    ["Fix the login timeout", "Alan", "In review", "High"],
    ["Update the dependencies", "Ada", "Done", "Low"],
    ["Plan the Q3 roadmap", "Grace", "To do", "Medium"],
    ["Translate the onboarding", "Linus", "In progress", "Low"],
    ["Audit the accessibility", "Alan", "To do", "High"],
] * 3

velocity = PlotBars([(i, v) for i, v in enumerate([12, 15, 11, 18, 21, 19, 24, 27])], name="Done tasks")
trend = PlotLine([(x / 4, 10 + 2.2 * x / 4 + math.sin(x / 2)) for x in range(29)], name="Trend")

NOTES = '''# Release 2.4

The release brings **faster sync** and a new *onboarding*.

- sync runs in the background
- the onboarding has 3 steps

```mermaid
flowchart LR
    A[Draft] --> B{Review}
    B -->|Approved| C[Release]
    B -->|Changes| A
```
'''


# panes

def stat_card(title, value, delta, good):
    def card():
        weak(title)
        heading(value)
        colored_label((34, 197, 94) if good else (239, 68, 68), delta)

    return card


def overview_ui():
    heading("Overview")
    weak("Sprint 14 · 8 days left")
    add_space(8)

    cards = [
        stat_card("Open tasks", "42", "▼ 6 since Monday", True),
        stat_card("In review", "9", "▲ 3 since Monday", False),
        stat_card("Velocity", "27", "▲ 12% this sprint", True),
    ]

    def column(i):
        frame(cards[i], inner_margin=12, rounding=12, fill=(127, 127, 127, 18), stroke=(1, (127, 127, 127, 50)))

    columns(3, column)
    add_space(12)
    strong("Velocity")
    plot("velocity", [velocity, trend], height=220)


def tasks_ui():
    with Layout(LayoutType.Horizontal):
        text_edit_singleline(search, hint_text="Search tasks")
        if button_clicked("New task"):
            status.value = "New task created"
    add_space(6)
    rows = [t for t in tasks if search.value.lower() in t[0].lower()]
    table(["Task", "Owner", "Status", "Priority"], rows, selected_task, id="tasks")


def notes_ui():
    markdown(NOTES, scrollable=True, id="notes")


def settings_ui():
    heading("Settings")
    add_space(6)

    def rows():
        label("Name")
        text_edit_singleline(name)
        end_row()
        label("Email")
        text_edit_singleline(email)
        end_row()
        label("Accent")
        color_edit_button_rgba(accent)
        end_row()
        label("Notifications")
        toggle_value(notifications, "Enabled")
        end_row()
        label("Weekly report")
        checkbox(weekly_report, "Send every Monday")
        end_row()

    grid("settings", rows, num_columns=2, spacing=(24, 10))
    add_space(10)
    if button_clicked("Save"):
        status.value = "Settings saved"


panes = [
    DockPane("Overview", overview_ui),
    DockPane("Tasks", tasks_ui),
    DockPane("Notes", notes_ui, closable=True),
    DockPane("Settings", settings_ui, closable=True),
]
# a pane name as argument opens it first, e.g. python app.py tasks
for arg in sys.argv[1:]:
    for i, pane in enumerate(panes):
        if pane.title.lower() == arg:
            panes.insert(0, panes.pop(i))

dock = Dock(
    DockContainer("tabs", panes),
    glass=glass.value,
    tab_style="pill" if pill.value else "classic",
)


# chrome

def apply_style(ctx):
    if theme.value == LIGHT:
        ctx.set_light_theme()
    elif theme.value == DARK:
        ctx.set_dark_theme()
    else:
        ctx.set_system_theme()

    ctx.reset_style()
    if glass.value:
        ctx.set_glass_style()
    dock.set_style(glass=glass.value, tab_style="pill" if pill.value else "classic")
    if BACKDROP == Backdrop.ACRYLIC:
        ctx.set_backdrop({LIGHT: Backdrop.ACRYLIC_LIGHT, DARK: Backdrop.ACRYLIC_DARK, SYSTEM: Backdrop.ACRYLIC}[theme.value])


def menus(ctx):
    def file_menu():
        if button_clicked("New task"):
            status.value = "New task created"
        if button_clicked("Export…"):
            status.value = "Exported"
        separator()
        if button_clicked("Quit"):
            status.value = "Quit is disabled in the demo"

    def view_menu():
        toggle_value(show_sidebar, "Sidebar")
        separator()
        before = (theme.value, glass.value, pill.value)
        radio_value(theme, LIGHT, "Light")
        radio_value(theme, DARK, "Dark")
        radio_value(theme, SYSTEM, "System")
        separator()
        checkbox(glass, "Glass")
        checkbox(pill, "Pill tabs")
        if (theme.value, glass.value, pill.value) != before:
            apply_style(ctx)

    def help_menu():
        if button_clicked("About Atelier"):
            about_open.value = True

    menu_button("File", file_menu)
    menu_button("View", view_menu)
    menu_button("Help", help_menu)


def sidebar():
    heading("Atelier")
    weak("Product team")
    add_space(10)

    for i, title in enumerate(PAGES):
        if selectable_label_clicked(page.value == i, title):
            page.value = i
            status.value = f"{title} opened"

    add_space(14)
    small("RECENT")
    for project in ["Website redesign", "Mobile app", "API v3"]:
        if link_clicked(project):
            status.value = f"{project} opened"

    add_space(14)
    separator()
    strong(name.value)
    weak(email.value)


def about_ui():
    heading("Atelier")
    label("A demo of pyegui: menu bar, sidebar, tabs and status bar.")
    if button_clicked("Close"):
        about_open.value = False


def update_func(ctx):
    if not started.value:
        apply_style(ctx)
        started.value = True

    top_panel("menu", lambda: menu_bar(lambda: menus(ctx)), inner_margin=6, show_separator_line=False)
    bottom_panel("status", lambda: small(status.value), inner_margin=6, show_separator_line=False)
    left_panel(
        "sidebar",
        sidebar,
        expanded=show_sidebar,
        default_size=210,
        inner_margin=14,
        show_separator_line=False,
    )
    dock_area(dock)

    if about_open.value:
        window("About", about_ui, open=about_open, collapsible=False, resizable=False)


if __name__ == "__main__":
    run_native("Atelier", update_func, inner_width=1180, inner_height=760, backdrop=BACKDROP)
