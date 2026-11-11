import sys

from pyegui import *

# the backdrop blurs what is behind the window: "acrylic" on Windows, "liquid_glass" on macOS 26.
# Pass another one as argument to try it: mica, tabbed, acrylic, blur, vibrancy, liquid_glass or liquid_glass_clear
DEFAULT_BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)
BACKDROP = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_BACKDROP

LIGHT, DARK, SYSTEM = 0, 1, 2

styled = Bool(False)
theme = Int(
    DARK
    if (BACKDROP or "").endswith("dark")
    else LIGHT
    if (BACKDROP or "").endswith("light")
    else SYSTEM
)
glass = Bool(True)
window_open = Bool(True)
volume = Float(0.6)
name = Str("")
notifications = Bool(True)

ACCENT = "#3b82f6"


def apply_style(ctx):
    ctx.reset_style()
    if glass.value:
        ctx.set_glass_style(mode="full")
        ctx.set_style(selection_color=(59, 130, 246, 160), heading_size=22)
    else:
        ctx.set_style(rounding=6)


def apply_theme(ctx):
    if theme.value == LIGHT:
        ctx.set_light_theme()
    elif theme.value == DARK:
        ctx.set_dark_theme()
    else:
        ctx.set_system_theme()

    # the tint of mica, tabbed and acrylic follows the theme of Windows, not the one of the app
    base = (BACKDROP or "").removesuffix("_light").removesuffix("_dark")
    if base in (Backdrop.MICA, Backdrop.TABBED, Backdrop.ACRYLIC):
        ctx.set_backdrop(
            base + {LIGHT: "_light", DARK: "_dark", SYSTEM: ""}[theme.value]
        )


def theme_selector(ctx):
    before = theme.value
    selectable_value(theme, LIGHT, "Light")
    selectable_value(theme, DARK, "Dark")
    selectable_value(theme, SYSTEM, "System")
    if theme.value != before:
        apply_theme(ctx)


def card(title, contents):
    def ui():
        strong(title)
        contents()

    # a translucent card with a light border and a soft shadow
    frame(
        ui,
        fill=(255, 255, 255, 40),
        stroke=(1, (255, 255, 255, 90)),
        rounding=16,
        inner_margin=14,
        shadow=(0, 6, 24, 0, (0, 0, 0, 40)),
    )


def buttons():
    with Layout(LayoutType.Horizontal):
        button_clicked("Default")
        button_clicked(
            "Accent", fill=ACCENT, text_color="#ffffff", rounding=20, min_size=(90, 30)
        )
        button_clicked(
            "Outline",
            fill=(0, 0, 0, 0),
            stroke=(1.5, ACCENT),
            text_color=ACCENT,
            rounding=20,
            min_size=(90, 30),
        )
        button_clicked("Danger", fill="#ef4444cc", text_color="#ffffff", rounding=8)
        button_clicked("Text only", frame=False)


def settings():
    text_edit_singleline(name, hint_text="Your name")
    slider_float(volume, 0, 1, "volume")
    checkbox(notifications, "Notifications")


def window_ui():
    label("Windows can have their own frame.")
    if button_clicked("Close", fill=ACCENT, text_color="#ffffff", rounding=12):
        window_open.value = False


def update_func(ctx):
    if not styled.value:
        apply_style(ctx)
        apply_theme(ctx)
        styled.value = True

    top_panel(
        "header",
        lambda: heading("Glass demo"),
        show_separator_line=False,
        inner_margin=12,
    )

    label(
        f"Backdrop: {BACKDROP}. Run with mica, tabbed, acrylic, blur, vibrancy, liquid_glass or liquid_glass_clear as argument."
    )

    def toolbar():
        if checkbox_changed(glass, "Glass style"):
            apply_style(ctx)
        toggle_value(window_open, "Window")
        separator()
        theme_selector(ctx)

    horizontal(toolbar)
    add_space(10)

    card("Buttons", buttons)
    add_space(10)
    card("Settings", settings)
    add_space(10)

    group(
        lambda: label("A group with a custom fill and rounding"),
        fill=(59, 130, 246, 50),
        rounding=12,
    )

    window(
        "Styled window",
        window_ui,
        open=window_open,
        default_pos=(520, 140),
        fill=(255, 255, 255, 70),
        stroke=(1, (255, 255, 255, 140)),
        rounding=20,
        inner_margin=16,
        shadow=(0, 12, 40, 0, (0, 0, 0, 60)),
    )


def checkbox_changed(value, text):
    before = value.value
    checkbox(value, text)
    return value.value != before


if __name__ == "__main__":
    run_native(
        f"pyegui glass - {BACKDROP}",
        update_func,
        inner_width=900,
        inner_height=560,
        backdrop=BACKDROP,
    )
