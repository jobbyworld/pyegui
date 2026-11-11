"""Atelier: a demo application with a sidebar, a toolbar, tabs and a status bar, in a modern
glass style.

The sidebar is glass and goes under the window buttons, the content is an almost opaque sheet.
The View menu switches the glass mode, the accent color, the theme and the tab style.

Arguments pick them at start: light, dark, full, noglass, classic, violet, green, orange, and
the first tab: tasks, notes or settings.
"""

import math
import sys

from pyegui import *

BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)
MACOS = sys.platform == "darwin"

LIGHT, DARK, SYSTEM = 0, 1, 2
NO_GLASS, PARTIAL, FULL = 0, 1, 2
ACCENTS = {"Blue": "#3b82f6", "Violet": "#8b5cf6", "Green": "#10b981", "Orange": "#f97316"}
PAGES = [("▦", "Overview"), ("☰", "Tasks"), ("▤", "Notes"), ("⚙", "Settings")]

STATUS_COLORS = {"Done": "#22c55e", "In review": "#a855f7", "In progress": "#3b82f6", "To do": "#94a3b8"}
PRIORITY_COLORS = {"High": "#ef4444", "Medium": "#f59e0b", "Low": "#64748b"}

args = sys.argv[1:]

# state

theme = Int(DARK if "dark" in args else LIGHT if "light" in args else SYSTEM)
glass = Int(NO_GLASS if "noglass" in args else FULL if "full" in args else PARTIAL)
pill = Bool("classic" not in args)
accent_name = Str(next((name for name in ACCENTS if name.lower() in args), "Blue"))
show_sidebar = Bool(True)
about_open = Bool(False)
status = Str("Ready")
search = Str("")
started = Bool(False)
context = []  # the Context, kept on the first frame for the settings pane

name = Str("Ada Lovelace")
email = Str("ada@example.com")
notifications = Bool(True)
weekly_report = Bool(False)
compact = Bool(False)

tasks = [
    ["Design the landing page", "Ada", "In progress", "High"],
    ["Write the release notes", "Grace", "To do", "Medium"],
    ["Fix the login timeout", "Alan", "In review", "High"],
    ["Update the dependencies", "Ada", "Done", "Low"],
    ["Plan the Q3 roadmap", "Grace", "To do", "Medium"],
    ["Translate the onboarding", "Linus", "In progress", "Low"],
    ["Audit the accessibility", "Alan", "To do", "High"],
    ["Migrate the billing service", "Linus", "In review", "Medium"],
    ["Clean up the feature flags", "Grace", "Done", "Low"],
]

# no colors: they come from the accent of the theme
velocity = PlotBars([(i, v) for i, v in enumerate([12, 15, 11, 18, 21, 19, 24, 27])], name="Done tasks")
trend = PlotLine([(x / 4, 10 + 2.2 * x / 4 + math.sin(x / 2)) for x in range(29)], name="Trend", width=2)

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
        colored_label("#22c55e" if good else "#ef4444", delta)

    return card


def overview_ui():
    heading("Overview")
    weak("Sprint 14 · 8 days left")
    add_space(10)

    cards = [
        stat_card("Open tasks", "42", "▼ 6 since Monday", True),
        stat_card("In review", "9", "▲ 3 since Monday", False),
        stat_card("Velocity", "27", "▲ 12% this sprint", True),
    ]

    def column(i):
        frame(cards[i], inner_margin=14, rounding=12, fill=(127, 127, 127, 16), stroke=(1, (127, 127, 127, 40)))

    columns(3, column)
    add_space(14)
    strong("Velocity")
    plot("velocity", [velocity, trend], height=240, allow_zoom=False, allow_drag=False, allow_scroll=False)


def tasks_ui():
    with Layout(LayoutType.Horizontal):
        text_edit_singleline(search, hint_text="Search tasks")
        if button_clicked("+ New task"):
            status.value = "New task created"
    add_space(8)
    rows = [t for t in tasks if search.value.lower() in t[0].lower()]

    def cell(row, col):
        task = rows[row]
        if col == 0:
            label(task[0])
        elif col == 1:
            weak(task[1])
        elif col == 2:
            badge(task[2], STATUS_COLORS[task[2]])
        else:
            badge(task[3], PRIORITY_COLORS[task[3]])

    table_ui(["Task", "Owner", "Status", "Priority"], len(rows), cell, id="tasks",
             column_widths=[280, 90, 110, "remainder"])


def notes_ui():
    markdown(NOTES, scrollable=True, id="notes")


def settings_ui():
    heading("Settings")
    add_space(8)

    def accents():
        for accent in ACCENTS:
            if selectable_label_clicked(accent_name.value == accent, accent):
                accent_name.value = accent
                context[0].set_accent(ACCENTS[accent])

    def rows():
        label("Name")
        text_edit_singleline(name)
        end_row()
        label("Email")
        text_edit_singleline(email)
        end_row()
        label("Accent")
        horizontal(accents)
        end_row()
        label("Notifications")
        switch(notifications)
        end_row()
        label("Weekly report")
        switch(weekly_report, "Every Monday")
        end_row()
        label("Compact mode")
        switch(compact)
        end_row()

    grid("settings", rows, num_columns=2, spacing=(28, 12))
    add_space(12)
    if button_clicked("Save"):
        status.value = "Settings saved"


dock = Dock(
    DockContainer(
        "tabs",
        [
            DockPane("Overview", overview_ui),
            DockPane("Tasks", tasks_ui),
            DockPane("Notes", notes_ui, closable=True),
            DockPane("Settings", settings_ui, closable=True),
        ],
    )
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
    if glass.value != NO_GLASS:
        ctx.set_glass_style(accent=ACCENTS[accent_name.value], mode="full" if glass.value == FULL else "partial")
    else:
        ctx.set_accent(ACCENTS[accent_name.value])
    dock.set_style(glass=glass.value != NO_GLASS, tab_style="pill" if pill.value else "classic")
    if BACKDROP == Backdrop.ACRYLIC:
        ctx.set_backdrop({LIGHT: Backdrop.ACRYLIC_LIGHT, DARK: Backdrop.ACRYLIC_DARK, SYSTEM: Backdrop.ACRYLIC}[theme.value])


def toolbar(ctx):
    def file_menu():
        if button_clicked("New task"):
            status.value = "New task created"
        if button_clicked("Export…"):
            status.value = "Exported"

    def view_menu():
        before = (theme.value, glass.value, pill.value, accent_name.value)
        toggle_value(show_sidebar, "Sidebar")
        separator()
        small("THEME")
        radio_value(theme, LIGHT, "Light")
        radio_value(theme, DARK, "Dark")
        radio_value(theme, SYSTEM, "System")
        separator()
        small("GLASS")
        radio_value(glass, PARTIAL, "Partial")
        radio_value(glass, FULL, "Full")
        radio_value(glass, NO_GLASS, "Off")
        separator()
        small("ACCENT")
        for accent in ACCENTS:
            if radio_clicked(accent_name.value == accent, accent):
                accent_name.value = accent
        separator()
        checkbox(pill, "Pill tabs")
        if (theme.value, glass.value, pill.value, accent_name.value) != before:
            apply_style(ctx)

    def help_menu():
        if button_clicked("About Atelier"):
            about_open.value = True

    def bar():
        if MACOS and not show_sidebar.value:
            add_space(64)  # the window buttons are over the toolbar when the sidebar is hidden
        menu_button("File", file_menu)
        menu_button("View", view_menu)
        menu_button("Help", help_menu)

    menu_bar(bar)


def sidebar():
    if MACOS:
        add_space(26)  # the window buttons are over the sidebar
    heading("Atelier")
    weak("Product team")
    add_space(14)

    active = dock.active_title
    for icon, title in PAGES:
        if selectable_label_clicked(active == title, f"{icon}   {title}"):
            if title not in dock.titles:
                status.value = f"{title} was closed"
            else:
                dock.set_active(title)
                status.value = f"{title} opened"

    add_space(18)
    weak("Recent")
    for project in ["Website redesign", "Mobile app", "API v3"]:
        if link_clicked(project):
            status.value = f"{project} opened"

    # the profile at the bottom of the sidebar
    add_space(max(available_height() - 52, 8))
    separator()
    strong(name.value)
    weak(email.value)


def about_ui():
    heading("Atelier")
    label("A demo of pyegui: sidebar, toolbar, tabs and status bar.")
    if button_clicked("Close"):
        about_open.value = False


def update_func(ctx):
    if not started.value:
        context.append(ctx)
        apply_style(ctx)
        for arg in args:
            if arg in ("tasks", "notes", "settings"):
                dock.set_active(arg.capitalize())
        started.value = True

    # the sidebar comes first, so it takes the whole height of the window
    left_panel(
        "sidebar",
        sidebar,
        expanded=show_sidebar,
        default_size=220,
        inner_margin=14,
        show_separator_line=False,
    )
    top_panel("toolbar", lambda: toolbar(ctx), inner_margin=8, show_separator_line=False)
    bottom_panel("status", lambda: small(status.value), inner_margin=10, show_separator_line=False)
    dock_area(dock)

    if about_open.value:
        window("About", about_ui, open=about_open, collapsible=False, resizable=False)


if __name__ == "__main__":
    run_native(
        "Atelier",
        update_func,
        inner_width=1180,
        inner_height=760,
        backdrop=BACKDROP,
        fullsize_content=True,
    )
