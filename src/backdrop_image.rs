//! A picture as the backdrop of the window: `Context.set_backdrop_image`. It is drawn under
//! everything, so it shows where the layout rules or the glass style leave the window clear, with a
//! luminance, a saturation and an opacity that can be changed at any time.
//!
//! The file is read and scaled down by a thread (the window never waits for it); the filters are
//! applied to that copy and the result kept as a texture until a setting changes.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle, TextureOptions};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// The longest side of the working copy: more is lost on a window, and costs at each filter change
const MAX_SIDE: u32 = 1920;

#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Cover,
    Contain,
    Stretch,
    Center,
}

/// a luminance: a factor, or the one that brings the picture to the brightness that suits the theme
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Lum {
    Value(f32),
    Auto,
}

/// the mean brightness (0 to 1) the "auto" luminance aims at: a dark picture under the light theme
/// is lifted, a bright one under the dark theme is toned down
const AUTO_LIGHT: f32 = 0.65;
const AUTO_DARK: f32 = 0.25;

#[derive(Clone, PartialEq)]
struct Settings {
    source: String,
    /// (light theme, dark theme)
    luminance: (Lum, Lum),
    saturation: (f32, f32),
    blur: f32,
    /// 1.0 as it is, less flattens the picture toward its mean brightness (the text reads better over it)
    contrast: (f32, f32),
    /// the veil over the picture, 0 to 1: black under the dark theme, white under the light one
    scrim: (f32, f32),
    opacity: f32,
    fit: Fit,
}

/// what `set_backdrop_image` asked for; none when there is no picture
static WANTED: Mutex<Option<Settings>> = Mutex::new(None);

type Loaded = Arc<Mutex<Option<Result<image::RgbaImage, String>>>>;

#[derive(Default)]
struct Cache {
    source: String,
    loading: Option<Loaded>,
    original: Option<image::RgbaImage>,
    /// the mean brightness of the picture, 0 to 1
    mean: f32,
    error: Option<String>,
    /// the luminance, the saturation and the blur of the texture
    applied: Option<(f32, f32, f32, f32)>,
    texture: Option<TextureHandle>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn parse_fit(fit: &str) -> PyResult<Fit> {
    match fit {
        "cover" => Ok(Fit::Cover),
        "contain" => Ok(Fit::Contain),
        "stretch" => Ok(Fit::Stretch),
        "center" => Ok(Fit::Center),
        other => Err(PyValueError::new_err(format!("fit is \"cover\", \"contain\", \"stretch\" or \"center\", not {other:?}"))),
    }
}

/// a path, or a `file://` uri (as `Path.as_uri()` gives it, with the % codes)
fn to_path(source: &str) -> String {
    let Some(rest) = source.strip_prefix("file://") else { return source.to_string() };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 0 {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8_lossy(&out).into_owned();
    // file:///C:/dir is /C:/dir
    if cfg!(windows) && path.len() > 2 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':' {
        return path[1..].to_string();
    }
    path
}

/// a number for both themes, or a pair (light, dark)
pub(crate) fn pair(value: &Bound<'_, PyAny>, name: &str) -> PyResult<(f32, f32)> {
    if let Ok(v) = value.extract::<f32>() {
        return Ok((v, v));
    }
    value
        .extract::<(f32, f32)>()
        .map_err(|_| PyValueError::new_err(format!("{name} is a number, or a pair (light, dark)")))
}

fn lum(value: &Bound<'_, PyAny>) -> PyResult<Lum> {
    if let Ok(text) = value.extract::<String>() {
        return if text == "auto" { Ok(Lum::Auto) } else { Err(PyValueError::new_err("luminance is a number, \"auto\", or a pair (light, dark)")) };
    }
    Ok(Lum::Value(value.extract::<f32>()?.clamp(0.0, 4.0)))
}

/// the luminance for both themes: a value or "auto", or a pair (light, dark) of them
pub(crate) fn lum_pair(value: &Bound<'_, PyAny>) -> PyResult<(Lum, Lum)> {
    if let Ok((a, b)) = value.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>() {
        return Ok((lum(&a)?, lum(&b)?));
    }
    let one = lum(value)?;
    Ok((one, one))
}

pub(crate) fn set(
    source: Option<&str>,
    luminance: (Lum, Lum),
    saturation: (f32, f32),
    blur: f32,
    contrast: (f32, f32),
    scrim: (f32, f32),
    opacity: f32,
    fit: &str,
) -> PyResult<()> {
    let wanted = match source {
        Some(source) if !source.is_empty() => Some(Settings {
            source: source.to_string(),
            luminance,
            saturation: (saturation.0.clamp(0.0, 4.0), saturation.1.clamp(0.0, 4.0)),
            blur: blur.clamp(0.0, 200.0),
            contrast: (contrast.0.clamp(0.0, 2.0), contrast.1.clamp(0.0, 2.0)),
            scrim: (scrim.0.clamp(0.0, 0.95), scrim.1.clamp(0.0, 0.95)),
            opacity: opacity.clamp(0.0, 1.0),
            fit: parse_fit(fit)?,
        }),
        _ => None,
    };
    if let Ok(mut slot) = WANTED.lock() {
        *slot = wanted;
    }
    Ok(())
}

fn start_loading(ctx: &egui::Context, cache: &mut Cache, source: &str) {
    let slot: Loaded = Arc::new(Mutex::new(None));
    let (job, ctx) = (Arc::clone(&slot), ctx.clone());
    let path = to_path(source);
    std::thread::spawn(move || {
        let result = image::open(&path).map_err(|e| e.to_string()).map(|img| {
            let img = if img.width().max(img.height()) > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
            img.to_rgba8()
        });
        if let Ok(mut s) = job.lock() {
            *s = Some(result);
        }
        ctx.request_repaint();
    });
    *cache = Cache { source: source.to_string(), loading: Some(slot), ..Cache::default() };
}

/// the pixels blurred (by a radius in pixels of the working copy), then with the saturation and the
/// luminance applied (1.0 leaves them as they are). A blur works on a smaller copy, the texture
/// is stretched back when it is drawn: what is blurred loses nothing, and it is much faster
fn filtered(original: &image::RgbaImage, luminance: f32, saturation: f32, contrast: f32, pivot: f32, blur: f32) -> ColorImage {
    let small;
    let original = if blur > 0.5 {
        let factor = (blur / 4.0).max(1.0);
        let (w, h) = ((original.width() as f32 / factor).max(2.0) as u32, (original.height() as f32 / factor).max(2.0) as u32);
        let reduced = image::imageops::resize(original, w, h, image::imageops::FilterType::Triangle);
        small = image::imageops::blur(&reduced, blur / factor);
        &small
    } else {
        original
    };
    let size = [original.width() as usize, original.height() as usize];
    let mut pixels = Vec::with_capacity(size[0] * size[1]);
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    for p in original.pixels() {
        let [r, g, b, a] = p.0;
        let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
        let gray = 0.299 * r + 0.587 * g + 0.114 * b;
        // the contrast turns about the mean brightness of the picture, as it is after the luminance
        let mix = |c: f32| pivot + ((gray + (c - gray) * saturation) * luminance - pivot) * contrast;
        pixels.push(Color32::from_rgba_unmultiplied(clamp(mix(r)), clamp(mix(g)), clamp(mix(b)), a));
    }
    ColorImage { size, source_size: egui::Vec2::new(size[0] as f32, size[1] as f32), pixels }
}

/// Draws the picture, if there is one, in `area` (the window): called at each frame before the app
pub(crate) fn paint(ctx: &egui::Context, area: Rect) {
    let wanted = WANTED.lock().ok().and_then(|w| w.clone());
    CACHE.with_borrow_mut(|cache| {
        let Some(s) = wanted else {
            if !cache.source.is_empty() {
                *cache = Cache::default();
            }
            return;
        };
        if cache.source != s.source {
            start_loading(ctx, cache, &s.source);
        }
        if let Some(slot) = cache.loading.clone() {
            let result = slot.lock().ok().and_then(|mut r| r.take());
            match result {
                Some(Ok(img)) => {
                    let sum: f64 = img.pixels().map(|p| 0.299 * f64::from(p.0[0]) + 0.587 * f64::from(p.0[1]) + 0.114 * f64::from(p.0[2])).sum();
                    cache.mean = (sum / (img.width() as f64 * img.height() as f64 * 255.0)) as f32;
                    cache.original = Some(img);
                    cache.loading = None;
                }
                Some(Err(e)) => {
                    cache.error = Some(e);
                    cache.loading = None;
                }
                None => ctx.request_repaint_after(Duration::from_millis(50)),
            }
        }
        // the filters of the theme in use
        let dark = ctx.global_style().visuals.dark_mode;
        let pick = |p: (f32, f32)| if dark { p.1 } else { p.0 };
        let luminance = match if dark { s.luminance.1 } else { s.luminance.0 } {
            Lum::Value(v) => v,
            Lum::Auto => (if dark { AUTO_DARK } else { AUTO_LIGHT } / cache.mean.max(0.02)).clamp(0.3, 2.0),
        };
        let saturation = pick(s.saturation);
        let contrast = pick(s.contrast);
        let pivot = cache.mean * luminance * 255.0;
        if let Some(original) = &cache.original {
            if cache.applied != Some((luminance, saturation, contrast, s.blur)) || cache.texture.is_none() {
                let image = filtered(original, luminance, saturation, contrast, pivot, s.blur);
                match &mut cache.texture {
                    Some(texture) => texture.set(image, TextureOptions::LINEAR),
                    None => cache.texture = Some(ctx.load_texture("pyegui_backdrop_image", image, TextureOptions::LINEAR)),
                }
                cache.applied = Some((luminance, saturation, contrast, s.blur));
            }
        }
        let Some(texture) = &cache.texture else { return };
        let natural = egui::vec2(
            cache.original.as_ref().map_or(1.0, |o| o.width() as f32),
            cache.original.as_ref().map_or(1.0, |o| o.height() as f32),
        );
        let scale = |k: f32| natural * k;
        let size = match s.fit {
            Fit::Stretch => area.size(),
            Fit::Cover => scale((area.width() / natural.x).max(area.height() / natural.y)),
            Fit::Contain => scale((area.width() / natural.x).min(area.height() / natural.y)),
            // the points of the screen, not those of the working copy
            Fit::Center => natural,
        };
        let rect = Rect::from_center_size(area.center(), size);
        let painter = ctx.layer_painter(egui::LayerId::background()).with_clip_rect(area);
        let tint = Color32::from_white_alpha((s.opacity * 255.0).round() as u8);
        painter.image(texture.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
        let veil = if dark { s.scrim.1 } else { s.scrim.0 };
        if veil > 0.0 {
            let (base, alpha) = (if dark { 0 } else { 255 }, (veil * s.opacity * 255.0).round() as u8);
            painter.rect_filled(area, 0.0, Color32::from_rgba_unmultiplied(base, base, base, alpha));
        }
    });
}

/// the reason a picture could not be shown, for `Context.backdrop_image_error`
pub(crate) fn error() -> Option<String> {
    CACHE.with_borrow(|c| c.error.clone())
}
