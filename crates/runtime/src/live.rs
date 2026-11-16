//! Live literal overrides (`devserver` feature).
//!
//! When only literal values change in view code (`text("…")`, `.width(12.0)`,
//! `.bg("#…")`), the dev server sends the new values instead of rebuilding.
//! Each edit is keyed by the source location of the element's constructor,
//! which `Element::new` records through `#[track_caller]` (`source-map`), and
//! is applied to the view tree on every render until the next rebuild.

use crate::element::{Element, Kind, Props};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum LiteralValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// One literal argument with a new value. `call` 0 is the constructor
/// (`text("…")`), 1.. are the chained builder methods in order.
#[derive(Clone, Debug, PartialEq)]
pub struct LiteralEdit {
    pub call: usize,
    pub method: String,
    pub arg: usize,
    pub value: LiteralValue,
}

/// Edits for each file that has an element constructor at one position.
type EditsByFile = Vec<(String, Vec<LiteralEdit>)>;

/// Constructor location (file with `/` separators, line, column) -> edits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiteralOverrides {
    by_position: HashMap<(u32, u32), EditsByFile>,
}

impl LiteralOverrides {
    pub fn is_empty(&self) -> bool {
        self.by_position.is_empty()
    }

    pub fn insert(&mut self, file: &str, line: u32, column: u32, edits: Vec<LiteralEdit>) {
        self.by_position
            .entry((line, column))
            .or_default()
            .push((normalize_path(file), edits));
    }

    fn get(&self, file: &str, line: u32, column: u32) -> Option<&[LiteralEdit]> {
        let file = normalize_path(file);
        self.by_position
            .get(&(line, column))?
            .iter()
            .find(|(key, _)| same_file(key, &file))
            .map(|(_, edits)| edits.as_slice())
    }
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Paths may be workspace-relative or absolute depending on the build; treat
/// one ending with the other at a `/` boundary as the same file.
fn same_file(a: &str, b: &str) -> bool {
    let ends = |long: &str, short: &str| {
        long.ends_with(short)
            && (long.len() == short.len() || long.as_bytes()[long.len() - short.len() - 1] == b'/')
    };
    ends(a, b) || ends(b, a)
}

/// Whether the dev server can apply a change to this argument live.
/// Anything else is treated as a code change and goes through a rebuild.
pub fn supports(method: &str, arg: usize) -> bool {
    set_field(
        &mut Props::default(),
        method,
        arg,
        &LiteralValue::Bool(false),
    )
    .is_some()
        || set_field(&mut Props::default(), method, arg, &LiteralValue::Num(0.0)).is_some()
        || set_field(
            &mut Props::default(),
            method,
            arg,
            &LiteralValue::Str(String::new()),
        )
        .is_some()
}

fn num(value: &LiteralValue) -> Option<f32> {
    match value {
        LiteralValue::Num(n) => Some(*n as f32),
        _ => None,
    }
}

fn string(value: &LiteralValue) -> Option<String> {
    match value {
        LiteralValue::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn boolean(value: &LiteralValue) -> Option<bool> {
    match value {
        LiteralValue::Bool(b) => Some(*b),
        _ => None,
    }
}

/// Set the prop a builder argument maps to. `None` if unsupported or the
/// value has the wrong type.
fn set_field(props: &mut Props, method: &str, arg: usize, value: &LiteralValue) -> Option<()> {
    macro_rules! set {
        ($field:ident, $conv:ident) => {{
            props.$field = Some($conv(value)?);
        }};
    }
    match (method, arg) {
        // Constructors (call 0).
        ("text" | "button", 0) => set!(text, string),
        ("image" | "src", 0) => set!(src, string),
        ("input" | "placeholder", 0) => set!(placeholder, string),
        ("input", 1) | ("value", 0) => set!(value, string),
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
        ("border", 0) => set!(border_width, num),
        ("border", 1) => set!(border_color, string),
        ("opacity", 0) => set!(opacity, num),
        ("shadow", 0) => set!(shadow, boolean),
        ("clip", 0) => set!(clip, boolean),
        ("animate", 0) => set!(animate, boolean),
        ("wrap", 0) => set!(wrap, boolean),
        ("placeholder_color", 0) => set!(placeholder_color, string),
        ("align", 0) => set!(align, string),
        ("label", 0) => set!(label, string),
        ("gap", 0) => set!(gap, num),
        ("align_items", 0) => set!(align_items, string),
        ("justify", 0) => set!(justify, string),
        ("top", 0) => set!(top, num),
        ("left", 0) => set!(left, num),
        ("bottom", 0) => set!(bottom, num),
        ("glass_style", 0) => set!(glass_style, string),
        ("glass_tint", 0) => set!(glass_tint, string),
        ("glass_interactive", 0) => set!(glass_interactive, boolean),
        ("content_width", 0) => set!(content_width, num),
        ("content_height", 0) => set!(content_height, num),
        ("fit", 0) => set!(fit, string),
        ("paged", 0) => set!(paged, boolean),
        ("pressable", 0) => set!(pressable, boolean),
        ("map_dark", 0) => set!(map_dark, boolean),
        ("map_shows_user", 0) => set!(map_shows_user, boolean),
        _ => return None,
    }
    Some(())
}

/// Apply overrides to a freshly built view tree.
pub(crate) fn apply(element: &mut Element, overrides: &LiteralOverrides) {
    if let Some(source) = &element.source {
        if let Some(edits) = overrides.get(&source.file, source.line, source.column) {
            for edit in edits {
                // A `native("…")` constructor's argument is its module name.
                if edit.call == 0 && edit.method == "native" && element.kind == Kind::Native {
                    if let Some(module) = string(&edit.value) {
                        element.props.native = Some(module);
                    }
                    continue;
                }
                let _ = set_field(&mut element.props, &edit.method, edit.arg, &edit.value);
            }
        }
    }
    for child in &mut element.children {
        apply(child, overrides);
    }
}

#[cfg(all(test, feature = "source-map"))]
mod tests {
    use super::*;
    use crate::element::{column, text};

    #[test]
    fn overrides_apply_by_constructor_location() {
        let label = text("before").size(12.0);
        let source = label.source.clone().expect("source-map records locations");
        let mut root = column().child(label);

        let mut overrides = LiteralOverrides::default();
        overrides.insert(
            &source.file,
            source.line,
            source.column,
            vec![
                LiteralEdit {
                    call: 0,
                    method: "text".into(),
                    arg: 0,
                    value: LiteralValue::Str("after".into()),
                },
                LiteralEdit {
                    call: 1,
                    method: "size".into(),
                    arg: 0,
                    value: LiteralValue::Num(18.0),
                },
            ],
        );
        apply(&mut root, &overrides);
        assert_eq!(root.children[0].props.text.as_deref(), Some("after"));
        assert_eq!(root.children[0].props.size, Some(18.0));
    }

    #[test]
    fn paths_match_across_separators_and_prefixes() {
        assert!(same_file("apps/x/src/a.rs", "apps/x/src/a.rs"));
        assert!(same_file("C:/repo/apps/x/src/a.rs", "apps/x/src/a.rs"));
        assert!(!same_file("apps/x/src/ba.rs", "a.rs"));
        assert_eq!(normalize_path(r"apps\x\a.rs"), "apps/x/a.rs");
    }

    #[test]
    fn unsupported_arguments_are_rejected() {
        assert!(supports("width", 0));
        assert!(supports("border", 1));
        assert!(!supports("child", 0));
        assert!(!supports("key", 0));
    }
}
