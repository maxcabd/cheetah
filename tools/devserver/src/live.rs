//! Live literal edits: tell a literal-only change apart from a code change.
//!
//! The running binary was built from a *baseline* of each source file. On
//! save, [`diff`] parses the baseline and the new text, finds every view
//! builder chain rooted at a Cheetah constructor (`text("…").size(14.0)`), and
//! checks that the file is otherwise identical once those chains' literal
//! arguments are masked out. If so, the changed literals become overrides
//! keyed by the constructor's *baseline* position, which is what
//! `#[track_caller]` recorded in the binary.

use runtime::dev::{LiteralEdit, LiteralOverrides, LiteralValue};
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprLit, ExprMethodCall, ExprUnary, Lit, UnOp};

/// Element constructors in `runtime::element` (all `#[track_caller]`).
const CONSTRUCTORS: &[&str] = &[
    "column", "row", "text", "button", "scroll", "list", "glass", "sheet", "map", "image",
    "hscroll", "native", "input", "spacer", "gap_w",
];

#[derive(Debug, PartialEq)]
pub enum Change {
    /// Same as the baseline.
    Unchanged,
    /// Only literal arguments changed; apply these instead of rebuilding.
    Literals(Vec<Located>),
    /// Anything else: needs a rebuild or hot patch.
    Code(String),
}

/// Edits for one element constructor at a baseline position.
#[derive(Debug, PartialEq)]
pub struct Located {
    pub line: u32,
    pub column: u32,
    pub edits: Vec<LiteralEdit>,
}

#[derive(Debug)]
struct Arg {
    literal: Option<LiteralValue>,
    bytes: std::ops::Range<usize>,
}

#[derive(Debug)]
struct Call {
    name: String,
    args: Vec<Arg>,
}

#[derive(Debug)]
struct Chain {
    line: u32,
    column: u32,
    calls: Vec<Call>,
}

#[derive(Default)]
struct Collector {
    chains: Vec<Chain>,
    roots: HashSet<usize>,
}

fn literal(expr: &Expr) -> Option<LiteralValue> {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Str(s) => Some(LiteralValue::Str(s.value())),
            Lit::Float(f) => f.base10_parse::<f64>().ok().map(LiteralValue::Num),
            Lit::Int(i) => i.base10_parse::<f64>().ok().map(LiteralValue::Num),
            Lit::Bool(b) => Some(LiteralValue::Bool(b.value)),
            _ => None,
        },
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => match literal(expr)? {
            LiteralValue::Num(n) => Some(LiteralValue::Num(-n)),
            _ => None,
        },
        _ => None,
    }
}

fn args<'a>(items: impl IntoIterator<Item = &'a Expr>) -> Vec<Arg> {
    items
        .into_iter()
        .map(|expr| Arg {
            literal: literal(expr),
            bytes: expr.span().byte_range(),
        })
        .collect()
}

fn constructor(call: &ExprCall) -> Option<String> {
    let Expr::Path(path) = call.func.as_ref() else {
        return None;
    };
    let name = path.path.segments.last()?.ident.to_string();
    CONSTRUCTORS.contains(&name.as_str()).then_some(name)
}

impl Collector {
    fn record(&mut self, base: &ExprCall, name: String, methods: Vec<&ExprMethodCall>) {
        let start = base.span().byte_range().start;
        if !self.roots.insert(start) {
            return; // a longer chain over the same constructor was recorded
        }
        let position = base.span().start();
        let mut calls = vec![Call {
            name,
            args: args(&base.args),
        }];
        for method in methods.into_iter().rev() {
            calls.push(Call {
                name: method.method.to_string(),
                args: args(&method.args),
            });
        }
        self.chains.push(Chain {
            line: position.line as u32,
            // proc-macro2 columns are 0-based; Location::caller's are 1-based.
            column: position.column as u32 + 1,
            calls,
        });
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let mut methods = vec![node];
        let mut receiver = node.receiver.as_ref();
        while let Expr::MethodCall(inner) = receiver {
            methods.push(inner);
            receiver = inner.receiver.as_ref();
        }
        if let Expr::Call(base) = receiver {
            if let Some(name) = constructor(base) {
                self.record(base, name, methods);
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some(name) = constructor(node) {
            self.record(node, name, Vec::new());
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn chains(source: &str) -> Result<Vec<Chain>, String> {
    let file = syn::parse_file(source).map_err(|e| format!("parse error: {e}"))?;
    let mut collector = Collector::default();
    collector.visit_file(&file);
    let mut chains = collector.chains;
    chains.sort_by_key(|c| (c.line, c.column));
    Ok(chains)
}

/// The source with every literal argument of every chain blanked out.
fn masked(source: &str, chains: &[Chain]) -> String {
    let mut ranges: Vec<_> = chains
        .iter()
        .flat_map(|c| c.calls.iter())
        .flat_map(|call| call.args.iter())
        .filter(|arg| arg.literal.is_some())
        .map(|arg| arg.bytes.clone())
        .collect();
    ranges.sort_by_key(|r| r.start);
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for range in ranges {
        if range.start < at || range.end > source.len() {
            continue;
        }
        out.push_str(&source[at..range.start]);
        out.push('\u{1}');
        at = range.end;
    }
    out.push_str(&source[at..]);
    out
}

/// Classify the change from `baseline` to `current` for one file.
pub fn diff(baseline: &str, current: &str) -> Change {
    if baseline == current {
        return Change::Unchanged;
    }
    let (old, new) = match (chains(baseline), chains(current)) {
        (Ok(old), Ok(new)) => (old, new),
        (_, Err(e)) | (Err(e), _) => return Change::Code(e),
    };
    if masked(baseline, &old) != masked(current, &new) {
        return Change::Code("code outside literal arguments changed".into());
    }
    // Masked texts match, so the chains line up one to one.
    if old.len() != new.len() {
        return Change::Code("view structure changed".into());
    }

    let mut located = Vec::new();
    for (before, after) in old.iter().zip(&new) {
        let mut edits = Vec::new();
        for (index, (a, b)) in before.calls.iter().zip(&after.calls).enumerate() {
            if a.name != b.name || a.args.len() != b.args.len() {
                return Change::Code("view structure changed".into());
            }
            for (arg, (x, y)) in a.args.iter().zip(&b.args).enumerate() {
                if x.literal == y.literal {
                    continue;
                }
                let Some(value) = y.literal.clone() else {
                    return Change::Code(format!("`{}` argument is no longer a literal", b.name));
                };
                if !runtime::live::supports(&b.name, arg) {
                    return Change::Code(format!("`{}` can't be changed live", b.name));
                }
                edits.push(LiteralEdit {
                    call: index,
                    method: b.name.clone(),
                    arg,
                    value,
                });
            }
        }
        if !edits.is_empty() {
            located.push(Located {
                line: before.line,
                column: before.column,
                edits,
            });
        }
    }
    if located.is_empty() {
        // Only comments/formatting inside literal-masked regions; treat as code
        // so nothing is silently skipped.
        return Change::Code("non-literal text changed".into());
    }
    Change::Literals(located)
}

/// Collect per-file results into overrides for the runtime.
pub fn overrides<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [Located])>,
) -> LiteralOverrides {
    let mut out = LiteralOverrides::default();
    for (file, located) in files {
        for item in located {
            out.insert(file, item.line, item.column, item.edits.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r##"
fn view() -> Element {
    column()
        .gap(8.0)
        .child(text("Ready to discover?").size(28.0).color("#ffffff"))
        .child(row().width(120.0).border(1.0, "#404040"))
}
"##;

    #[test]
    fn literal_edits_are_located_at_the_baseline_constructor() {
        let new = BASE
            .replace("Ready to discover?", "Hello")
            .replace(".size(28.0)", ".size(32.0)")
            .replace("\"#404040\"", "\"#ff0000\"");
        let Change::Literals(located) = diff(BASE, &new) else {
            panic!("expected literal-only change: {:?}", diff(BASE, &new));
        };
        assert_eq!(located.len(), 2);
        let text = &located[0];
        assert_eq!((text.line, text.column), (5, 16));
        assert_eq!(text.edits[0].method, "text");
        assert_eq!(text.edits[0].value, LiteralValue::Str("Hello".into()));
        assert_eq!(text.edits[1].method, "size");
        let row = &located[1];
        assert_eq!(row.edits[0].method, "border");
        assert_eq!(row.edits[0].arg, 1);
    }

    #[test]
    fn code_changes_fall_back_to_rebuild() {
        let added = BASE.replace(".gap(8.0)", ".gap(8.0).child(spacer())");
        assert!(matches!(diff(BASE, &added), Change::Code(_)));
        let logic = BASE.replace("fn view()", "fn render()");
        assert!(matches!(diff(BASE, &logic), Change::Code(_)));
        let not_literal = BASE.replace(".size(28.0)", ".size(SIZE)");
        assert!(matches!(diff(BASE, &not_literal), Change::Code(_)));
        assert_eq!(diff(BASE, BASE), Change::Unchanged);
    }

    #[test]
    fn positions_match_track_caller() {
        // The column this module computes must equal what `#[track_caller]`
        // records for a constructor, or overrides would never match.
        let element = runtime::text("probe");
        let source = element.source.expect("source-map feature");
        let file = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/live.rs"),
        )
        .unwrap();
        let found = chains(&file).unwrap().into_iter().any(|c| {
            c.line == source.line && c.column == source.column && c.calls[0].name == "text"
        });
        assert!(found, "no chain at {}:{}", source.line, source.column);
    }
}
