//! Setting element props by builder name at runtime.
//!
//! Shared by `.view` templates (attributes like `gap=12`) and the dev
//! server's live literal edits (`.gap(12.0)` changed in Rust source). Names
//! are the builder methods of [`Element`](crate::element::Element); `arg` is
//! the argument position (`border(width, color)`, `input(placeholder, value)`).

use crate::element::Props;

#[derive(Clone, Debug, PartialEq)]
pub enum DynValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

fn num(value: &DynValue) -> Option<f32> {
    match value {
        DynValue::Num(n) => Some(*n as f32),
        _ => None,
    }
}

fn string(value: &DynValue) -> Option<String> {
    match value {
        DynValue::Str(s) => Some(s.clone()),
        DynValue::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => Some(format!("{}", *n as i64)),
        DynValue::Num(n) => Some(n.to_string()),
        DynValue::Bool(b) => Some(b.to_string()),
    }
}

fn boolean(value: &DynValue) -> Option<bool> {
    match value {
        DynValue::Bool(b) => Some(*b),
        _ => None,
    }
}

/// Whether `name` (argument `arg`) maps to a prop.
pub fn supports(name: &str, arg: usize) -> bool {
    [
        DynValue::Bool(false),
        DynValue::Num(0.0),
        DynValue::Str(String::new()),
    ]
    .iter()
    .any(|value| set(&mut Props::default(), name, arg, value).is_some())
}

/// Set the prop builder `name` argument `arg` maps to. `None` if the name is
/// unknown or the value has the wrong type for it.
pub fn set(props: &mut Props, name: &str, arg: usize, value: &DynValue) -> Option<()> {
    macro_rules! set {
        ($field:ident, $conv:ident) => {{
            props.$field = Some($conv(value)?);
        }};
    }
    match (name, arg) {
        // Constructor arguments.
        ("text" | "button", 0) => set!(text, string),
        ("image" | "src", 0) => set!(src, string),
        ("input" | "placeholder", 0) => set!(placeholder, string),
        ("input", 1) | ("value", 0) => set!(value, string),
        ("native", 0) => set!(native, string),
        ("gap_w", 0) => set!(width, num),
        // Builder methods.
        ("size", 0) => set!(size, num),
        ("color", 0) => set!(color, string),
        ("bg", 0) => set!(bg, string),
        ("pad", 0) => set!(pad, num),
        ("pad_h", 0) => set!(pad_h, num),
        ("pad_v", 0) => set!(pad_v, num),
        ("pad_bottom", 0) => set!(pad_bottom, num),
        ("radius", 0) => set!(radius, num),
        ("weight", 0) => set!(weight, num),
        ("width", 0) => set!(width, num),
        ("height", 0) => set!(height, num),
        ("bold", 0) => set!(bold, boolean),
        ("font", 0) => set!(font, string),
        ("line_height", 0) => set!(line_height, num),
        ("lines", 0) => props.lines = Some(num(value)?.max(0.0) as u32),
        ("border" | "border_width", 0) => set!(border_width, num),
        ("border", 1) | ("border_color", 0) => set!(border_color, string),
        ("opacity", 0) => set!(opacity, num),
        ("shadow", 0) => set!(shadow, boolean),
        ("clip", 0) => set!(clip, boolean),
        ("animate", 0) => set!(animate, boolean),
        ("wrap", 0) => set!(wrap, boolean),
        ("placeholder_color", 0) => set!(placeholder_color, string),
        ("align", 0) => set!(align, string),
        ("label", 0) => set!(label, string),
        ("test_id", 0) => set!(test_id, string),
        ("gap", 0) => set!(gap, num),
        ("align_items", 0) => set!(align_items, string),
        ("justify", 0) => set!(justify, string),
        ("absolute", 0) => set!(absolute, boolean),
        ("top", 0) => set!(top, num),
        ("left", 0) => set!(left, num),
        ("bottom", 0) => set!(bottom, num),
        ("glass_style", 0) => set!(glass_style, string),
        ("glass_tint", 0) => set!(glass_tint, string),
        ("glass_interactive", 0) => set!(glass_interactive, boolean),
        ("content_width", 0) => set!(content_width, num),
        ("content_height", 0) => set!(content_height, num),
        ("content_inset_top", 0) => set!(content_inset_top, num),
        ("content_inset_bottom", 0) => set!(content_inset_bottom, num),
        ("scroll_to", 0) => set!(scroll_to, num),
        ("scroll_x", 0) => set!(scroll_x, num),
        ("fit", 0) => set!(fit, string),
        ("paged", 0) => set!(paged, boolean),
        ("pressable", 0) => set!(pressable, boolean),
        ("native_props", 0) => set!(native_props, string),
        ("map_markers", 0) => set!(map_markers, string),
        ("map_dark", 0) => set!(map_dark, boolean),
        ("map_shows_user", 0) => set!(map_shows_user, boolean),
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_props_with_type_checks() {
        let mut props = Props::default();
        assert!(set(&mut props, "width", 0, &DynValue::Num(12.0)).is_some());
        assert!(set(&mut props, "border", 1, &DynValue::Str("#fff".into())).is_some());
        assert!(set(&mut props, "width", 0, &DynValue::Str("wide".into())).is_none());
        assert_eq!(props.width, Some(12.0));
        assert_eq!(props.border_color.as_deref(), Some("#fff"));
        assert!(supports("gap", 0));
        assert!(!supports("child", 0));
        assert!(!supports("key", 0));
    }

    #[test]
    fn numbers_render_as_text_without_trailing_zeros() {
        let mut props = Props::default();
        set(&mut props, "text", 0, &DynValue::Num(3.0)).unwrap();
        assert_eq!(props.text.as_deref(), Some("3"));
    }
}
