//! Helpers for the shared pieces in `views/widgets.view`.

use crate::nightshade::*;

impl NightshadeApp {
    /// Chip width for a label (the chip text is one line).
    pub(crate) fn chip_width(label: &str) -> f32 {
        label.chars().count() as f32 * 8.0 + 30.0
    }
}
