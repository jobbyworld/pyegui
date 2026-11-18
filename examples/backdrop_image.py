"""A picture as the backdrop of the window, with sliders for its luminance, saturation and opacity.

    python examples/backdrop_image.py [picture]

The picture is drawn under everything (`ctx.set_backdrop_image`) and shows where the layout rules
leave the window clear: here a transparent sidebar and a translucent sheet in the boxes.
"""

import sys
from pathlib import Path

from pyegui import *

picture = Str(sys.argv[1] if len(sys.argv) > 1 else "")
# one setting for each theme: the picture follows the theme of the window
luminance = {"light": Float(1.0), "dark": Float(0.6)}
saturation = {"light": Float(1.0), "dark": Float(0.9)}
blur = Float(0.0)
opacity = Float(1.0)
fit = Int(0)  # the index of FITS
FITS = ["cover", "contain", "stretch", "center"]
started = Bool(False)


def settings():
    """the picture and its filters are given at each frame: the picture is read once, a filter that
    changed is applied again to the copy in memory"""
    path = picture.value.strip().strip("'\"")
    ctx.set_backdrop_image(
        path if path and Path(path).is_file() else None,
        luminance=(luminance["light"].value, luminance["dark"].value),
        saturation=(saturation["light"].value, saturation["dark"].value),
        blur=blur.value,
        opacity=opacity.value,
        fit=FITS[fit.value],
    )


def controls():
    strong("Image de fond")
    text_edit_singleline(picture, hint_text="Chemin de l'image")
    if ctx.backdrop_image_error():
        label("⚠ " + ctx.backdrop_image_error())
    add_space(8)
    weak("Thème clair")
    slider_float(luminance["light"], 0.0, 2.0, "Luminance")
    slider_float(saturation["light"], 0.0, 2.0, "Saturation")
    weak("Thème sombre")
    slider_float(luminance["dark"], 0.0, 2.0, "Luminance")
    slider_float(saturation["dark"], 0.0, 2.0, "Saturation")
    weak("Commun")
    slider_float(blur, 0.0, 60.0, "Flou")
    slider_float(opacity, 0.0, 1.0, "Transparence")
    add_space(8)
    segmented_control(fit, [(None, name) for name in FITS])
    add_space(8)
    if button_clicked("Réinitialiser"):
        for holder in (*luminance.values(), *saturation.values(), opacity):
            holder.value = 1.0
        blur.value = 0.0


def update_func(context):
    global ctx
    ctx = context
    if not started.value:
        started.value = True
        ctx.set_glass_style(mode="full", opacity=0.4)
    settings()
    sidebar_panel(controls)
    heading("Backdrop")
    label("The picture is under the window; the boxes are translucent glass.")
    weak("Change the sliders: the luminance and the saturation are applied to the picture, the opacity lets the window behind show through.")


run_native(
    "Backdrop image",
    update_func,
    transparent=True,
    layout=dict(gap=8, rounding=12, sheet=(20, 20, 24, 140)),
)
