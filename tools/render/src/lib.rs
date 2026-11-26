//! Paint a laid-out Cheetah screen to a PNG, on any machine: the painter
//! behind `cheetah-render`, and behind the dev server showing agents the
//! live screen.

pub mod icons;
pub mod paint;
pub mod svg;
pub mod text;

use runtime::layout::Frame;
use runtime::tree::UiNode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The app's bundled fonts and photos in this checkout.
pub fn default_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ios/Cheetah/Assets")
}

/// `root`, laid out in `frames`, on a `width` x `height` point screen at
/// `scale` pixels per point: the PNG, and what's on it (for `--tree`).
pub fn draw(
    root: &UiNode,
    frames: &HashMap<u32, Frame>,
    (width, height): (f32, f32),
    scale: f32,
    assets: &Path,
) -> Result<(Vec<u8>, Vec<serde_json::Value>), String> {
    let fonts = text::Fonts::load(assets)?;
    let (pixels_wide, pixels_high) = (
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(pixels_wide, pixels_high).ok_or("bad size")?;
    pixmap.fill(paint::color("#000000", 1.0));
    let mut painter = paint::Painter::new(&fonts, frames, assets.to_path_buf(), scale);
    painter.paint(&mut pixmap, root);
    let png = pixmap.encode_png().map_err(|error| error.to_string())?;
    Ok((png, painter.drawn))
}
