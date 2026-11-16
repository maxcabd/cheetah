//! Small shared view pieces: icons, chips, buttons.

use crate::nightshade::*;

impl NightshadeApp {
    pub(crate) fn icon(name: &str, size: f32, color: &str) -> Element {
        native("icon")
            .native_props(format!(r#"{{"name":"{name}","color":"{color}"}}"#))
            .width(size)
            .height(size)
            .size(size)
            .color(color)
    }

    pub(crate) fn glass_icon(icon_name: &str, key: &str) -> Element {
        glass()
            .key(key)
            .pressable(true)
            .width(38.0)
            .height(38.0)
            .radius(19.0)
            .glass_style("regular")
            .glass_tint("#00000047")
            .glass_interactive(true)
            .align_items("center")
            .justify("center")
            .child(Self::icon(icon_name, 16.0, "#ffffff"))
    }

    pub(crate) fn action_button(
        icon_name: &str,
        key: &str,
        background: &str,
        size: f32,
    ) -> Element {
        row()
            .key(key)
            .pressable(true)
            .width(size)
            .height(size)
            .radius(size / 2.0)
            .bg(background)
            .align_items("center")
            .justify("center")
            .child(Self::icon(icon_name, 16.0, "#ffffff"))
    }

    /// Chip width for a label (the chip text is one line).
    pub(crate) fn chip_width(label: &str) -> f32 {
        label.chars().count() as f32 * 8.0 + 30.0
    }

    pub(crate) fn chip(label: &str, key: &str) -> Element {
        text(label)
            .key(key)
            .pressable(true)
            .size(14.0)
            .font("satoshi-bold")
            .color("#ffffff")
            .bg(CHIP_BG)
            .border(1.0, "#ffffff14")
            .pad_h(14.0)
            .width(Self::chip_width(label))
            .height(42.0)
            .lines(1)
            .align("center")
            .radius(18.0)
    }
}
