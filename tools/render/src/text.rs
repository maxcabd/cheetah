//! Fonts, measuring and drawing text. The app's own fonts come from
//! `ios/Cheetah/Assets`. The system font (SF on iOS) can't be shipped, so
//! Satoshi stands in for it, for drawing and for measuring alike: like iOS,
//! layout measures every text in the system font, whatever it's drawn in.

use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont};
use runtime::layout::TextMeasurer;
use std::collections::HashMap;
use std::path::Path;
use tiny_skia::{Color, Mask, Pixmap};

pub struct Fonts {
    faces: HashMap<&'static str, FontVec>,
}

/// `font` values in `.view` files and the files they name (as the iOS host's
/// `makeFont`).
const FACES: [(&str, &str); 9] = [
    ("Satoshi-Regular", "Satoshi-Regular.otf"),
    ("Satoshi-Medium", "Satoshi-Medium.otf"),
    ("Satoshi-Bold", "Satoshi-Bold.otf"),
    ("Brice-SemiBold", "Brice-Semi-Bold.otf"),
    ("Brice-Bold", "Brice-Bold.otf"),
    ("GeneralSans-Regular", "GeneralSans-Regular.otf"),
    ("GeneralSans-Medium", "GeneralSans-Medium.otf"),
    ("GeneralSans-Semibold", "GeneralSans-Semibold.otf"),
    ("GeneralSans-Bold", "GeneralSans-Bold.otf"),
];

impl Fonts {
    pub fn load(assets: &Path) -> Result<Self, String> {
        let mut faces = HashMap::new();
        for (name, file) in FACES {
            let path = assets.join(file);
            let data =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let face = FontVec::try_from_vec(data)
                .map_err(|_| format!("{}: not a font", path.display()))?;
            faces.insert(name, face);
        }
        Ok(Self { faces })
    }

    /// The face for a `.view` `font` (and `bold`), as the iOS host picks it.
    pub fn face(&self, family: Option<&str>, bold: bool) -> &FontVec {
        let name = match family.unwrap_or("") {
            "satoshi" => "Satoshi-Regular",
            "satoshi-medium" => "Satoshi-Medium",
            "satoshi-bold" => "Satoshi-Bold",
            "brice" | "brice-semibold" => "Brice-SemiBold",
            "brice-bold" => "Brice-Bold",
            "general" => "GeneralSans-Regular",
            "general-medium" => "GeneralSans-Medium",
            "general-semibold" => "GeneralSans-Semibold",
            "general-bold" => "GeneralSans-Bold",
            _ => self.system_name(bold),
        };
        &self.faces[name]
    }

    fn system_name(&self, bold: bool) -> &'static str {
        if bold {
            "Satoshi-Bold"
        } else {
            "Satoshi-Medium"
        }
    }

    pub fn system(&self, bold: bool) -> &FontVec {
        &self.faces[self.system_name(bold)]
    }
}

/// A line's height at `size`: the face's ascent, descent and gap.
pub fn line_height(face: &FontVec, size: f32) -> f32 {
    let scaled = face.as_scaled(PxScale::from(size));
    scaled.ascent() - scaled.descent() + scaled.line_gap()
}

fn advance(face: &FontVec, size: f32, text: &str) -> f32 {
    let scaled = face.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut previous: Option<GlyphId> = None;
    for character in text.chars() {
        let id = scaled.glyph_id(character);
        if let Some(previous) = previous {
            width += scaled.kern(previous, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

/// `text` broken into lines no wider than `max_width`, at spaces where it
/// can, and within a word where it must.
pub fn wrap(face: &FontVec, size: f32, text: &str, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if advance(face, size, &candidate) <= max_width
                || line.is_empty() && advance(face, size, word) <= max_width
            {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word wider than the line: break it.
            let mut piece = String::new();
            for character in word.chars() {
                let next = format!("{piece}{character}");
                if !piece.is_empty() && advance(face, size, &next) > max_width {
                    lines.push(std::mem::take(&mut piece));
                    piece.push(character);
                } else {
                    piece = next;
                }
            }
            line = piece;
        }
        lines.push(line);
    }
    lines
}

/// Cut `line` to fit `max_width`, ending in an ellipsis.
fn truncate(face: &FontVec, size: f32, line: &str, max_width: f32) -> String {
    if advance(face, size, line) <= max_width {
        return line.to_string();
    }
    let mut kept: String = line.to_string();
    while !kept.is_empty() && advance(face, size, &format!("{}…", kept.trim_end())) > max_width {
        kept.pop();
    }
    format!("{}…", kept.trim_end())
}

/// Layout's text sizes, measured as the iOS host does: in the system font,
/// one point wider than the text, rounded up.
pub struct Measurer<'a> {
    pub fonts: &'a Fonts,
}

impl TextMeasurer for Measurer<'_> {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        let size = font_size.max(1.0);
        let face = self.fonts.system(bold);
        let lines = wrap(face, size, text, max_width.max(1.0));
        let width = lines
            .iter()
            .map(|line| advance(face, size, line))
            .fold(0.0, f32::max);
        let height = lines.len().max(1) as f32 * line_height(face, size);
        ((width.min(max_width)).ceil() + 1.0, height.ceil())
    }
}

pub struct TextStyle<'a> {
    pub face: &'a FontVec,
    pub size: f32,
    pub color: Color,
    /// "start" (also "natural"), "center" or "end".
    pub align: &'a str,
    /// 0: as many as fit.
    pub lines: usize,
    pub line_height: Option<f32>,
}

/// Draw `text` in the box (left, top, width, height), centered vertically as
/// a UILabel does, in points; `scale` is pixels per point.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    scale: f32,
    text: &str,
    style: &TextStyle,
    (left, top, width, height): (f32, f32, f32, f32),
) {
    if text.is_empty() || width <= 0.0 {
        return;
    }
    let face = style.face;
    let natural = line_height(face, style.size);
    let step = style.line_height.unwrap_or(natural).max(1.0);
    let mut lines = wrap(face, style.size, text, width + 0.5);
    let fit = ((height + 0.5) / step).floor().max(1.0) as usize;
    let limit = if style.lines == 0 {
        fit
    } else {
        style.lines.min(fit.max(1))
    };
    if lines.len() > limit {
        let rest = lines[limit - 1..].join(" ");
        lines.truncate(limit - 1);
        lines.push(truncate(face, style.size, &rest, width));
    }
    let block = lines.len() as f32 * step;
    let mut baseline_top = top + ((height - block) / 2.0).max(0.0);
    let scaled = face.as_scaled(PxScale::from(style.size * scale));
    for line in &lines {
        let line_width = advance(face, style.size, line);
        let x = match style.align {
            "center" => left + (width - line_width) / 2.0,
            "end" | "right" => left + width - line_width,
            _ => left,
        };
        // The line's glyphs sit centered in its step, as with a line height.
        let baseline = baseline_top
            + (step - natural) / 2.0
            + face.as_scaled(PxScale::from(style.size)).ascent();
        let mut pen = x * scale;
        let mut previous: Option<GlyphId> = None;
        for character in line.chars() {
            let id = scaled.glyph_id(character);
            if let Some(previous) = previous {
                pen += scaled.kern(previous, id);
            }
            let glyph = id.with_scale_and_position(
                PxScale::from(style.size * scale),
                ab_glyph::point(pen, baseline * scale),
            );
            pen += scaled.h_advance(id);
            previous = Some(id);
            if let Some(outlined) = face.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    blend(pixmap, mask, px, py, style.color, coverage);
                });
            }
        }
        baseline_top += step;
    }
}

/// `color` over the pixel at (x, y), by `coverage`, inside `mask`.
pub fn blend(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    x: i32,
    y: i32,
    color: Color,
    coverage: f32,
) {
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let index = (y * width + x) as usize;
    let clip = mask.map_or(1.0, |mask| mask.data()[index] as f32 / 255.0);
    let alpha = color.alpha() * coverage.clamp(0.0, 1.0) * clip;
    if alpha <= 0.0 {
        return;
    }
    let pixel = &mut pixmap.pixels_mut()[index];
    let keep = 1.0 - alpha;
    let total = ((alpha * 255.0) + pixel.alpha() as f32 * keep)
        .round()
        .min(255.0) as u8;
    // Premultiplied: no channel above alpha, rounding included.
    let mix = |source: f32, destination: u8| {
        ((source * alpha * 255.0) + destination as f32 * keep)
            .round()
            .min(total as f32) as u8
    };
    let mixed = tiny_skia::PremultipliedColorU8::from_rgba(
        mix(color.red(), pixel.red()),
        mix(color.green(), pixel.green()),
        mix(color.blue(), pixel.blue()),
        total,
    );
    if let Some(mixed) = mixed {
        *pixel = mixed;
    }
}
