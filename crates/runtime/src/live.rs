//! Live literal overrides (`devserver` feature).
//!
//! When only literal values change in view code (`text("…")`, `.width(12.0)`,
//! `.bg("#…")`), the dev server sends the new values instead of rebuilding.
//! Each edit is keyed by the source location of the element's constructor,
//! which `Element::new` records through `#[track_caller]` (`source-map`), and
//! is applied to the view tree on every render until the next rebuild.

use crate::dynprops;
use crate::element::Element;
use std::collections::HashMap;

/// A literal argument value (the same values `.view` attributes take).
pub use crate::dynprops::DynValue as LiteralValue;

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
    dynprops::supports(method, arg)
}

/// Apply overrides to a freshly built view tree.
pub(crate) fn apply(element: &mut Element, overrides: &LiteralOverrides) {
    if let Some(source) = &element.source {
        if let Some(edits) = overrides.get(&source.file, source.line, source.column) {
            for edit in edits {
                let _ = dynprops::set(&mut element.props, &edit.method, edit.arg, &edit.value);
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
