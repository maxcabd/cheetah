//! `.view` templates: declarative UI the runtime interprets.
//!
//! Views describe structure and style; Rust owns state and logic and passes
//! a data model in. Because templates are interpreted, the dev server can
//! swap them in a running app with no compile, so new or removed views show
//! up immediately. Release builds embed the same files (`include_str!`).
//!
//! ```text
//! # Comments start with `#` or `//`.
//! const CHIP_BG = "#242424"
//!
//! component Suggestions
//!   column gap=14 pad_h=24
//!     text "Ready to discover?" size=20 font="brice-semibold"
//!     for chip, index in chips
//!       use Chip label=chip.label key="suggest:{index}"
//!     if loading
//!       text "Searching…"
//!     else
//!       spacer
//!
//! component Chip
//!   text label key=key pressable=true bg=CHIP_BG radius=18
//! ```
//!
//! - Elements are the constructors in [`crate::element`]; positional values
//!   are their arguments (`text "…"`, `input "placeholder" value`).
//! - `name=value` attributes are builder methods (`gap=14` is `.gap(14.0)`),
//!   plus `key=`. Values are numbers, strings (with `{path}` interpolation),
//!   `true`/`false`, or paths into the model, loop variables, component
//!   arguments, and `const`s.
//! - `for item[, index] in path`, `if [!]path`, `if path == value`, `else`.
//! - Choose a value inline: `bg=glass ? CLEAR : SHEET_BG`, with the same
//!   conditions as `if` (`!path ? a : b`, `path == value ? a : b`).
//! - `use Component arg=value ...` renders another component.
//! - `style name = attr=value ...` names a set of attributes; `.name` on an
//!   element applies it in place, so later attributes override it. Styles
//!   may include styles defined above them.
//! - Indentation (spaces) nests children.

use crate::dynprops::{self, DynValue};
use crate::element::{
    button, column, gap_w, glass, hscroll, image, input, list, map, native, row, scroll, sheet,
    spacer, text, Element, Kind, Props, SourceLocation,
};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;

const CONSTRUCTORS: &[&str] = &[
    "column", "row", "text", "button", "scroll", "list", "glass", "sheet", "map", "image",
    "hscroll", "native", "input", "spacer", "gap_w",
];
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct ViewError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for ViewError {}

// ---------------------------------------------------------------------------
// Syntax tree
// ---------------------------------------------------------------------------

type Path = Vec<String>;

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String),
    Path(Path),
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Value(DynValue),
    Template(Vec<Part>),
    Path(Path),
    /// `cond ? then : otherwise`.
    Choose(Box<Cond>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
enum Cond {
    Truthy {
        path: Path,
        negate: bool,
    },
    Compare {
        path: Path,
        equal: bool,
        value: Expr,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Element {
        ctor: String,
        args: Vec<Expr>,
        attrs: Vec<(String, Expr)>,
        children: Vec<Node>,
        /// Filled in once everything is parsed (`Views::prepare`).
        plan: Plan,
    },
    For {
        item: String,
        index: Option<String>,
        list: Path,
        body: Vec<Node>,
    },
    If {
        cond: Cond,
        then: Vec<Node>,
        otherwise: Vec<Node>,
    },
    Use {
        name: String,
        args: Vec<(String, Expr)>,
        line: usize,
    },
}

/// How to set an element's props: start from `base`, which has every value
/// known when the views were parsed, then apply `sets` in order.
#[derive(Clone, Debug, Default, PartialEq)]
struct Plan {
    base: Props,
    /// The element itself (kind, source), props aside.
    shell: Option<Shell>,
    sets: Vec<Set>,
    key: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
struct Shell {
    kind: Kind,
    source: Option<SourceLocation>,
}

#[derive(Clone, Debug, PartialEq)]
struct Set {
    /// The attribute, or the constructor for its positional argument `arg`.
    name: String,
    arg: usize,
    value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
struct Component {
    body: Vec<Node>,
}

/// A parsed set of `.view` files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Views {
    components: HashMap<String, Component>,
    consts: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Str(String),
    Num(f64),
    Eq,
    EqEq,
    NotEq,
    Bang,
    Comma,
    Question,
    Colon,
    /// `.name`: apply a style.
    Style(String),
}

struct Line {
    number: usize,
    indent: usize,
    tokens: Vec<Token>,
}

fn lex_line(text: &str, file: &str, number: usize) -> Result<Vec<Token>, ViewError> {
    let err = |message: String| ViewError {
        file: file.into(),
        line: number,
        message,
    };
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            let mut value = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(err("unterminated string".into())),
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some('\\') => {
                        match chars.get(i + 1) {
                            Some('n') => value.push('\n'),
                            Some(other) => value.push(*other),
                            None => return Err(err("unterminated string".into())),
                        }
                        i += 2;
                    }
                    Some(other) => {
                        value.push(*other);
                        i += 1;
                    }
                }
            }
            tokens.push(Token::Str(value));
        } else if c == '=' && chars.get(i + 1) == Some(&'=') {
            tokens.push(Token::EqEq);
            i += 2;
        } else if c == '!' && chars.get(i + 1) == Some(&'=') {
            tokens.push(Token::NotEq);
            i += 2;
        } else if c == '=' {
            tokens.push(Token::Eq);
            i += 1;
        } else if c == '!' {
            tokens.push(Token::Bang);
            i += 1;
        } else if c == ',' {
            tokens.push(Token::Comma);
            i += 1;
        } else if c == '?' {
            tokens.push(Token::Question);
            i += 1;
        } else if c == ':' {
            tokens.push(Token::Colon);
            i += 1;
        } else if c == '.'
            && chars
                .get(i + 1)
                .is_some_and(|n| n.is_alphabetic() || *n == '_')
        {
            let start = i + 1;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Style(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit()
            || (c == '-'
                && chars
                    .get(i + 1)
                    .is_some_and(|n| n.is_ascii_digit() || *n == '.'))
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            let value = raw
                .parse::<f64>()
                .map_err(|_| err(format!("bad number `{raw}`")))?;
            tokens.push(Token::Num(value));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            tokens.push(Token::Word(chars[start..i].iter().collect()));
        } else {
            return Err(err(format!("unexpected `{c}`")));
        }
    }
    Ok(tokens)
}

fn lex(file: &str, source: &str) -> Result<Vec<Line>, ViewError> {
    let mut lines = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if raw.starts_with('\t') {
            return Err(ViewError {
                file: file.into(),
                line: number,
                message: "indent with spaces, not tabs".into(),
            });
        }
        let indent = raw.len() - raw.trim_start().len();
        lines.push(Line {
            number,
            indent,
            tokens: lex_line(trimmed, file, number)?,
        });
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

type Attrs = Vec<(String, Expr)>;

struct Parser<'a> {
    file: &'a str,
    lines: Vec<Line>,
    at: usize,
    styles: &'a HashMap<String, Attrs>,
}

fn path_of(word: &str) -> Path {
    word.split('.').map(str::to_string).collect()
}

/// Split `"a {path} b"` into text and paths; `{{` and `}}` are literal braces.
fn template(text: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                let mut path = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    path.push(next);
                }
                if !closed {
                    literal.push('{');
                    literal.push_str(&path);
                    break;
                }
                if !literal.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut literal)));
                }
                parts.push(Part::Path(path_of(path.trim())));
            }
            other => literal.push(other),
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Text(literal));
    }
    parts
}

impl<'a> Parser<'a> {
    fn err(&self, line: usize, message: impl Into<String>) -> ViewError {
        ViewError {
            file: self.file.into(),
            line,
            message: message.into(),
        }
    }

    fn expr(&self, token: &Token, line: usize) -> Result<Expr, ViewError> {
        Ok(match token {
            Token::Num(n) => Expr::Value(DynValue::Num(*n)),
            Token::Str(s) if s.contains('{') || s.contains('}') => Expr::Template(template(s)),
            Token::Str(s) => Expr::Value(DynValue::Str(s.clone())),
            Token::Word(w) if w == "true" => Expr::Value(DynValue::Bool(true)),
            Token::Word(w) if w == "false" => Expr::Value(DynValue::Bool(false)),
            Token::Word(w) => Expr::Path(path_of(w)),
            other => return Err(self.err(line, format!("expected a value, found {other:?}"))),
        })
    }

    /// `cond ? then : otherwise` at the start of `tokens`, with how many
    /// tokens it used; `None` when the value isn't a choice.
    fn choose(&self, tokens: &[Token], line: usize) -> Result<Option<(Expr, usize)>, ViewError> {
        let (cond, used) = match tokens {
            [Token::Bang, Token::Word(path), Token::Question, ..] => (
                Cond::Truthy {
                    path: path_of(path),
                    negate: true,
                },
                2,
            ),
            [Token::Word(path), op @ (Token::EqEq | Token::NotEq), value, Token::Question, ..] => (
                Cond::Compare {
                    path: path_of(path),
                    equal: *op == Token::EqEq,
                    value: self.expr(value, line)?,
                },
                3,
            ),
            [Token::Word(path), Token::Question, ..] => (
                Cond::Truthy {
                    path: path_of(path),
                    negate: false,
                },
                1,
            ),
            _ => return Ok(None),
        };
        match &tokens[used..] {
            [Token::Question, then, Token::Colon, otherwise, ..] => Ok(Some((
                Expr::Choose(
                    Box::new(cond),
                    Box::new(self.expr(then, line)?),
                    Box::new(self.expr(otherwise, line)?),
                ),
                used + 4,
            ))),
            _ => Err(self.err(line, "expected `cond ? value : value`")),
        }
    }

    /// `name=value .style name=value ...`, styles expanded in place.
    fn attrs(&self, tokens: &[Token], line: usize) -> Result<Attrs, ViewError> {
        let mut attrs = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            match (&tokens[i], tokens.get(i + 1), tokens.get(i + 2)) {
                (Token::Style(name), ..) => {
                    let style = self
                        .styles
                        .get(name)
                        .ok_or_else(|| self.err(line, format!("unknown style `.{name}`")))?;
                    attrs.extend(style.iter().cloned());
                    i += 1;
                }
                (Token::Word(name), Some(Token::Eq), Some(_)) => {
                    if let Some((choose, used)) = self.choose(&tokens[i + 2..], line)? {
                        attrs.push((name.clone(), choose));
                        i += 2 + used;
                    } else {
                        attrs.push((name.clone(), self.expr(&tokens[i + 2], line)?));
                        i += 3;
                    }
                }
                (other, ..) => {
                    return Err(self.err(line, format!("expected `name=value`, found {other:?}")))
                }
            }
        }
        Ok(attrs)
    }

    /// Children of the line at `self.at - 1`, i.e. following lines indented
    /// deeper than `parent_indent`.
    fn block(&mut self, parent_indent: usize) -> Result<Vec<Node>, ViewError> {
        let mut nodes = Vec::new();
        let Some(first) = self.lines.get(self.at) else {
            return Ok(nodes);
        };
        if first.indent <= parent_indent {
            return Ok(nodes);
        }
        let indent = first.indent;
        while let Some(line) = self.lines.get(self.at) {
            if line.indent < indent {
                break;
            }
            if line.indent > indent {
                return Err(self.err(line.number, "unexpected indent"));
            }
            nodes.push(self.node()?);
        }
        Ok(nodes)
    }

    fn node(&mut self) -> Result<Node, ViewError> {
        let line = &self.lines[self.at];
        let (number, indent, tokens) = (line.number, line.indent, line.tokens.clone());
        self.at += 1;
        let Some(Token::Word(head)) = tokens.first() else {
            return Err(self.err(number, "expected an element, `for`, `if`, or `use`"));
        };
        match head.as_str() {
            "for" => {
                // for item[, index] in path
                let (item, index, list) = match &tokens[1..] {
                    [Token::Word(item), Token::Word(kw), Token::Word(list)] if kw == "in" => {
                        (item.clone(), None, list.clone())
                    }
                    [Token::Word(item), Token::Comma, Token::Word(index), Token::Word(kw), Token::Word(list)]
                        if kw == "in" =>
                    {
                        (item.clone(), Some(index.clone()), list.clone())
                    }
                    _ => return Err(self.err(number, "expected `for item[, index] in list`")),
                };
                let body = self.block(indent)?;
                Ok(Node::For {
                    item,
                    index,
                    list: path_of(&list),
                    body,
                })
            }
            "if" => {
                let cond = match &tokens[1..] {
                    [Token::Word(path)] => Cond::Truthy {
                        path: path_of(path),
                        negate: false,
                    },
                    [Token::Bang, Token::Word(path)] => Cond::Truthy {
                        path: path_of(path),
                        negate: true,
                    },
                    [Token::Word(path), op @ (Token::EqEq | Token::NotEq), value] => {
                        Cond::Compare {
                            path: path_of(path),
                            equal: *op == Token::EqEq,
                            value: self.expr(value, number)?,
                        }
                    }
                    _ => {
                        return Err(self.err(number, "expected `if [!]path` or `if path == value`"))
                    }
                };
                let then = self.block(indent)?;
                let mut otherwise = Vec::new();
                if let Some(next) = self.lines.get(self.at) {
                    if next.indent == indent
                        && matches!(next.tokens.as_slice(), [Token::Word(w)] if w == "else")
                    {
                        self.at += 1;
                        otherwise = self.block(indent)?;
                    }
                }
                Ok(Node::If {
                    cond,
                    then,
                    otherwise,
                })
            }
            "else" => Err(self.err(number, "`else` without a matching `if`")),
            "use" => {
                let Some(Token::Word(name)) = tokens.get(1) else {
                    return Err(self.err(number, "expected `use ComponentName`"));
                };
                if tokens[2..].iter().any(|t| matches!(t, Token::Style(_))) {
                    return Err(self.err(number, "styles apply to elements, not `use`"));
                }
                let args = self.attrs(&tokens[2..], number)?;
                if self.lines.get(self.at).is_some_and(|l| l.indent > indent) {
                    return Err(self.err(number, "`use` takes no children"));
                }
                Ok(Node::Use {
                    name: name.clone(),
                    args,
                    line: number,
                })
            }
            ctor if CONSTRUCTORS.contains(&ctor) => {
                let ctor = ctor.to_string();
                let mut args = Vec::new();
                let mut i = 1;
                // Positional arguments come before the first `name=`.
                while i < tokens.len()
                    && !matches!(tokens[i], Token::Style(_))
                    && tokens.get(i + 1) != Some(&Token::Eq)
                {
                    if let Some((choose, used)) = self.choose(&tokens[i..], number)? {
                        args.push(choose);
                        i += used;
                    } else {
                        args.push(self.expr(&tokens[i], number)?);
                        i += 1;
                    }
                }
                for position in 0..args.len() {
                    if !dynprops::supports(&ctor, position) {
                        return Err(self.err(
                            number,
                            format!("`{ctor}` takes no argument {}", position + 1),
                        ));
                    }
                }
                let attrs = self.attrs(&tokens[i..], number)?;
                for (name, _) in &attrs {
                    if name != "key" && !dynprops::supports(name, 0) {
                        return Err(self.err(number, format!("unknown attribute `{name}`")));
                    }
                }
                let children = self.block(indent)?;
                Ok(Node::Element {
                    ctor,
                    args,
                    attrs,
                    children,
                    plan: Plan::default(),
                })
            }
            other => Err(self.err(number, format!("unknown element `{other}`"))),
        }
    }
}

impl Views {
    /// Parse `(file name, source)` pairs into one set of components.
    pub fn parse(files: &[(&str, &str)]) -> Result<Self, ViewError> {
        let mut views = Views::default();
        let mut lexed = Vec::new();
        for (file, source) in files {
            lexed.push((*file, lex(file, source)?));
        }

        // Styles first, so any component can use any style.
        let mut styles: HashMap<String, Attrs> = HashMap::new();
        for (file, lines) in &lexed {
            for line in lines.iter().filter(|line| line.indent == 0) {
                let [Token::Word(kw), rest @ ..] = line.tokens.as_slice() else {
                    continue;
                };
                if kw != "style" {
                    continue;
                }
                let parser = Parser {
                    file,
                    lines: Vec::new(),
                    at: 0,
                    styles: &styles,
                };
                let (name, attrs) = match rest {
                    [Token::Word(name), Token::Eq, attrs @ ..] if !attrs.is_empty() => {
                        (name.clone(), parser.attrs(attrs, line.number)?)
                    }
                    _ => {
                        return Err(
                            parser.err(line.number, "expected `style name = attr=value ...`")
                        )
                    }
                };
                for (attr, _) in &attrs {
                    if !dynprops::supports(attr, 0) {
                        return Err(parser.err(line.number, format!("unknown attribute `{attr}`")));
                    }
                }
                if styles.contains_key(&name) {
                    return Err(parser.err(line.number, format!("style `{name}` is defined twice")));
                }
                styles.insert(name, attrs);
            }
        }

        for (file, lines) in lexed {
            let mut parser = Parser {
                file,
                lines,
                at: 0,
                styles: &styles,
            };
            while parser.at < parser.lines.len() {
                let line = &parser.lines[parser.at];
                let (number, indent, tokens) = (line.number, line.indent, line.tokens.clone());
                if indent != 0 {
                    return Err(
                        parser.err(number, "expected `component` or `const` at the top level")
                    );
                }
                parser.at += 1;
                match tokens.as_slice() {
                    [Token::Word(kw), ..] if kw == "style" => {}
                    [Token::Word(kw), Token::Word(name)] if kw == "component" => {
                        let body = parser.block(0)?;
                        if body.is_empty() {
                            return Err(parser.err(number, format!("component `{name}` is empty")));
                        }
                        if views.components.contains_key(name) {
                            return Err(
                                parser.err(number, format!("component `{name}` is defined twice"))
                            );
                        }
                        views.components.insert(name.clone(), Component { body });
                    }
                    [Token::Word(kw), Token::Word(name), Token::Eq, value] if kw == "const" => {
                        let value = match parser.expr(value, number)? {
                            Expr::Value(v) => to_json(&v),
                            // Only escaped braces (`{{`, `}}`): still a literal.
                            Expr::Template(parts)
                                if parts.iter().all(|part| matches!(part, Part::Text(_))) =>
                            {
                                Value::String(
                                    parts
                                        .iter()
                                        .map(|part| match part {
                                            Part::Text(text) => text.as_str(),
                                            Part::Path(_) => "",
                                        })
                                        .collect(),
                                )
                            }
                            _ => return Err(parser.err(number, "a const must be a literal")),
                        };
                        views.consts.insert(name.clone(), value);
                    }
                    _ => {
                        return Err(
                            parser.err(number, "expected `component Name` or `const NAME = value`")
                        )
                    }
                }
            }
        }
        // `use` targets must exist.
        for component in views.components.values() {
            views.check_uses(&component.body)?;
        }
        views.prepare();
        Ok(views)
    }

    /// Work out each element's `Plan`. Literals, and consts no loop variable
    /// or component argument is named after, are the same in every render.
    fn prepare(&mut self) {
        let mut bound = HashSet::new();
        for component in self.components.values() {
            bound_names(&component.body, &mut bound);
        }
        let fixed: HashMap<String, Value> = self
            .consts
            .iter()
            .filter(|(name, _)| !bound.contains(*name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        for component in self.components.values_mut() {
            prepare_nodes(&mut component.body, &fixed);
        }
    }

    fn check_uses(&self, nodes: &[Node]) -> Result<(), ViewError> {
        for node in nodes {
            match node {
                Node::Use { name, line, .. } if !self.components.contains_key(name) => {
                    return Err(ViewError {
                        file: String::new(),
                        line: *line,
                        message: format!("unknown component `{name}`"),
                    });
                }
                Node::Element { children, .. } => self.check_uses(children)?,
                Node::For { body, .. } => self.check_uses(body)?,
                Node::If {
                    then, otherwise, ..
                } => {
                    self.check_uses(then)?;
                    self.check_uses(otherwise)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn has(&self, component: &str) -> bool {
        self.components.contains_key(component)
    }

    /// A `const` from the views, e.g. a color or a shape's native_props.
    pub fn constant(&self, name: &str) -> Value {
        self.consts.get(name).cloned().unwrap_or(Value::Null)
    }

    /// Render `component` with `model` (any serializable data) as one element.
    pub fn render(&self, component: &str, model: &Value) -> Element {
        let Some(found) = self.components.get(component) else {
            return text(format!("missing view `{component}`")).color("#ff5555");
        };
        let mut scope = Scope {
            views: self,
            model,
            frames: Vec::new(),
        };
        let mut out = Vec::new();
        scope.nodes(&found.body, 0, &mut out);
        if out.len() == 1 {
            out.pop().unwrap()
        } else {
            column().children(out)
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Names loops and component arguments give a value.
fn bound_names(nodes: &[Node], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            Node::Element { children, .. } => bound_names(children, out),
            Node::For {
                item, index, body, ..
            } => {
                out.insert(item.clone());
                out.extend(index.clone());
                bound_names(body, out);
            }
            Node::If {
                then, otherwise, ..
            } => {
                bound_names(then, out);
                bound_names(otherwise, out);
            }
            Node::Use { args, .. } => out.extend(args.iter().map(|(name, _)| name.clone())),
        }
    }
}

fn prepare_nodes(nodes: &mut [Node], consts: &HashMap<String, Value>) {
    for node in nodes {
        match node {
            Node::Element {
                ctor,
                args,
                attrs,
                children,
                plan,
            } => {
                *plan = make_plan(ctor, args, attrs, consts);
                prepare_nodes(children, consts);
            }
            Node::For { body, .. } => prepare_nodes(body, consts),
            Node::If {
                then, otherwise, ..
            } => {
                prepare_nodes(then, consts);
                prepare_nodes(otherwise, consts);
            }
            Node::Use { .. } => {}
        }
    }
}

/// `expr` with const paths replaced by their values.
fn fold(expr: &Expr, consts: &HashMap<String, Value>) -> Expr {
    match expr {
        Expr::Path(path) => match path.as_slice() {
            [name] => consts
                .get(name)
                .and_then(to_dyn)
                .map_or_else(|| expr.clone(), Expr::Value),
            _ => expr.clone(),
        },
        Expr::Choose(cond, then, otherwise) => Expr::Choose(
            cond.clone(),
            Box::new(fold(then, consts)),
            Box::new(fold(otherwise, consts)),
        ),
        other => other.clone(),
    }
}

/// Attributes that may write the same prop, so their order matters.
fn writes(name: &str) -> &str {
    match name {
        "border" | "border_width" | "border_color" => "border",
        "text" | "button" => "text",
        "image" | "src" => "src",
        "input" | "placeholder" | "value" => "input",
        "gap_w" | "width" => "width",
        "link" | "pressable" => "pressable",
        "map_region" | "map_latitude" | "map_longitude" | "map_latitude_delta"
        | "map_longitude_delta" => "map_region",
        other => other,
    }
}

fn make_plan(
    ctor: &str,
    args: &[Expr],
    attrs: &[(String, Expr)],
    consts: &HashMap<String, Value>,
) -> Plan {
    let element = construct(ctor);
    let mut plan = Plan {
        shell: Some(Shell {
            kind: element.kind,
            source: element.source,
        }),
        base: element.props,
        ..Plan::default()
    };
    let positional = args.iter().enumerate().map(|(arg, expr)| (ctor, arg, expr));
    let named = attrs.iter().map(|(name, expr)| (name.as_str(), 0, expr));
    for (name, arg, expr) in positional.chain(named) {
        if name == "key" {
            plan.key = Some(fold(expr, consts));
            continue;
        }
        match fold(expr, consts) {
            // Fixed, and nothing before it that changes could be overwritten
            // out of order.
            Expr::Value(value) if plan.sets.iter().all(|set| writes(&set.name) != writes(name)) => {
                let _ = dynprops::set(&mut plan.base, name, arg, &value);
            }
            value => plan.sets.push(Set {
                name: name.to_string(),
                arg,
                value,
            }),
        }
    }
    plan
}

fn construct(ctor: &str) -> Element {
    match ctor {
        "column" => column(),
        "row" => row(),
        "text" => text(""),
        "button" => button(""),
        "scroll" => scroll(),
        "list" => list(),
        "glass" => glass(),
        "sheet" => sheet(),
        "map" => map(),
        "image" => image(""),
        "hscroll" => hscroll(),
        "native" => native(""),
        "input" => input("", ""),
        "spacer" => spacer(),
        _ => gap_w(0.0),
    }
}

fn to_json(value: &DynValue) -> Value {
    match value {
        DynValue::Str(s) => Value::String(s.clone()),
        DynValue::Num(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        DynValue::Bool(b) => Value::Bool(*b),
    }
}

fn to_dyn(value: &Value) -> Option<DynValue> {
    match value {
        Value::String(s) => Some(DynValue::Str(s.clone())),
        Value::Number(n) => n.as_f64().map(DynValue::Num),
        Value::Bool(b) => Some(DynValue::Bool(*b)),
        _ => None,
    }
}

fn display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// Loop variables or a component's arguments. Few enough to search.
type Frame<'a> = Vec<(&'a str, Cow<'a, Value>)>;

struct Scope<'a> {
    views: &'a Views,
    model: &'a Value,
    frames: Vec<Frame<'a>>,
}

/// The value at `path` inside `value`.
fn walk<'v>(value: &'v Value, path: &[String]) -> Option<&'v Value> {
    path.iter().try_fold(value, |current, segment| {
        match segment.parse::<usize>() {
            Ok(index) => current.get(index),
            Err(_) => current.get(segment),
        }
    })
}

impl<'a> Scope<'a> {
    /// Borrowed from the model or views wherever possible: a place passed
    /// down through components is never copied.
    fn lookup(&self, path: &[String]) -> Cow<'a, Value> {
        let null = || Cow::Owned(Value::Null);
        let Some((first, rest)) = path.split_first() else {
            return null();
        };
        let in_frames = self
            .frames
            .iter()
            .rev()
            .find_map(|frame| frame.iter().rev().find(|(name, _)| name == first));
        match in_frames {
            Some((_, Cow::Borrowed(value))) => walk(value, rest).map_or_else(null, Cow::Borrowed),
            Some((_, Cow::Owned(value))) => {
                walk(value, rest).map_or_else(null, |value| Cow::Owned(value.clone()))
            }
            None => self
                .views
                .consts
                .get(first)
                .or_else(|| self.model.get(first))
                .and_then(|value| walk(value, rest))
                .map_or_else(null, Cow::Borrowed),
        }
    }

    fn holds(&self, cond: &Cond) -> bool {
        match cond {
            Cond::Truthy { path, negate } => truthy(&self.lookup(path)) != *negate,
            Cond::Compare { path, equal, value } => {
                (*self.lookup(path) == *self.eval(value)) == *equal
            }
        }
    }

    fn eval(&self, expr: &Expr) -> Cow<'a, Value> {
        match expr {
            Expr::Choose(cond, then, otherwise) => {
                self.eval(if self.holds(cond) { then } else { otherwise })
            }
            Expr::Value(value) => Cow::Owned(to_json(value)),
            Expr::Path(path) => self.lookup(path),
            Expr::Template(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        Part::Text(text) => out.push_str(text),
                        Part::Path(path) => match &*self.lookup(path) {
                            Value::String(text) => out.push_str(text),
                            other => out.push_str(&display(other)),
                        },
                    }
                }
                Cow::Owned(Value::String(out))
            }
        }
    }

    /// Numbers for string props (e.g. `text=3`) are formatted by dynprops;
    /// wrong types are skipped.
    fn set(&self, props: &mut Props, set: &Set, expr: &Expr) {
        match expr {
            Expr::Value(value) => {
                let _ = dynprops::set(props, &set.name, set.arg, value);
            }
            Expr::Choose(cond, then, otherwise) => {
                self.set(props, set, if self.holds(cond) { then } else { otherwise })
            }
            _ => {
                let value = match self.eval(expr) {
                    Cow::Owned(Value::String(text)) => Some(DynValue::Str(text)),
                    value => to_dyn(&value),
                };
                if let Some(value) = value {
                    let _ = dynprops::set(props, &set.name, set.arg, &value);
                }
            }
        }
    }

    fn element(&mut self, ctor: &str, plan: &'a Plan, children: &'a [Node], depth: usize) -> Element {
        let mut element = match &plan.shell {
            Some(shell) => Element {
                kind: shell.kind,
                key: None,
                props: plan.base.clone(),
                source: shell.source.clone(),
                children: Vec::with_capacity(children.len()),
            },
            None => construct(ctor),
        };
        for set in &plan.sets {
            self.set(&mut element.props, set, &set.value);
        }
        if let Some(key) = &plan.key {
            let key = self.eval(key);
            if !key.is_null() {
                element.key = Some(display(&key));
            }
        }
        self.nodes(children, depth + 1, &mut element.children);
        element
    }

    fn nodes(&mut self, nodes: &'a [Node], depth: usize, out: &mut Vec<Element>) {
        if depth > MAX_DEPTH {
            out.push(text("view nesting too deep").color("#ff5555"));
            return;
        }
        for node in nodes {
            match node {
                Node::Element {
                    ctor,
                    children,
                    plan,
                    ..
                } => {
                    let element = self.element(ctor, plan, children, depth);
                    out.push(element);
                }
                Node::For {
                    item,
                    index,
                    list,
                    body,
                } => {
                    let items: Vec<Cow<'a, Value>> = match self.lookup(list) {
                        Cow::Borrowed(Value::Array(items)) => {
                            items.iter().map(Cow::Borrowed).collect()
                        }
                        Cow::Owned(Value::Array(items)) => {
                            items.into_iter().map(Cow::Owned).collect()
                        }
                        _ => Vec::new(),
                    };
                    for (position, value) in items.into_iter().enumerate() {
                        let mut frame: Frame<'a> = vec![(item.as_str(), value)];
                        if let Some(index) = index {
                            frame.push((index.as_str(), Cow::Owned(Value::from(position))));
                        }
                        self.frames.push(frame);
                        self.nodes(body, depth + 1, out);
                        self.frames.pop();
                    }
                }
                Node::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    let branch = if self.holds(cond) { then } else { otherwise };
                    self.nodes(branch, depth + 1, out);
                }
                Node::Use { name, args, .. } => {
                    let views = self.views;
                    let Some(component) = views.components.get(name) else {
                        out.push(text(format!("missing view `{name}`")).color("#ff5555"));
                        continue;
                    };
                    let frame: Frame<'a> = args
                        .iter()
                        .map(|(key, expr)| (key.as_str(), self.eval(expr)))
                        .collect();
                    // Components see only their arguments, consts, and the model.
                    let outer = std::mem::replace(&mut self.frames, vec![frame]);
                    self.nodes(&component.body, depth + 1, out);
                    self.frames = outer;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(source: &str) -> Views {
        Views::parse(&[("test.view", source)]).unwrap()
    }

    fn texts(element: &Element, out: &mut Vec<String>) {
        if let Some(text) = &element.props.text {
            out.push(text.clone());
        }
        for child in &element.children {
            texts(child, out);
        }
    }

    #[test]
    fn renders_elements_attributes_and_children() {
        let views = parse(
            r##"
const BG = "#101010"

component Card
  column gap=12 bg=BG key="card"
    text "Hello" size=20 bold=true
    row width=-1
      spacer
"##,
        );
        let root = views.render("Card", &Value::Null);
        assert_eq!(root.kind, Kind::Column);
        assert_eq!(root.key.as_deref(), Some("card"));
        assert_eq!(root.props.gap, Some(12.0));
        assert_eq!(root.props.bg.as_deref(), Some("#101010"));
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].props.text.as_deref(), Some("Hello"));
        assert_eq!(root.children[0].props.bold, Some(true));
        assert_eq!(root.children[1].props.width, Some(-1.0));
        assert_eq!(root.children[1].children[0].kind, Kind::Space);
    }

    #[test]
    fn consts_may_hold_escaped_braces() {
        let views = parse(
            r##"
const LOOK = "{{\"fill\":\"#fff\"}}"

component Shape
  native "shape" native_props=LOOK
"##,
        );
        let root = views.render("Shape", &Value::Null);
        assert_eq!(root.props.native_props.as_deref(), Some(r##"{"fill":"#fff"}"##));
    }

    #[test]
    fn values_can_be_chosen_inline() {
        let views = parse(
            r##"
const CLEAR = "#00000000"

component Pill
  row bg=glass ? CLEAR : "#171717" radius=!big ? 8 : 20 key=mode == "map" ? "on-map" : "elsewhere" width=4
    text big ? "Big {mode}" : "Small" size=12
"##,
        );
        let root = views.render("Pill", &serde_json::json!({"glass": true, "big": false, "mode": "map"}));
        assert_eq!(root.props.bg.as_deref(), Some("#00000000"));
        assert_eq!(root.props.radius, Some(8.0));
        assert_eq!(root.key.as_deref(), Some("on-map"));
        assert_eq!(root.props.width, Some(4.0), "attributes after a choice still apply");
        assert_eq!(root.children[0].props.text.as_deref(), Some("Small"));
        assert_eq!(root.children[0].props.size, Some(12.0));

        let root = views.render("Pill", &serde_json::json!({"glass": false, "big": true, "mode": "list"}));
        assert_eq!(root.props.bg.as_deref(), Some("#171717"));
        assert_eq!(root.props.radius, Some(20.0));
        assert_eq!(root.key.as_deref(), Some("elsewhere"));
        assert_eq!(root.children[0].props.text.as_deref(), Some("Big list"));
    }

    #[test]
    fn loops_conditions_interpolation_and_components() {
        let views = parse(
            r##"
component List
  column
    for place, index in places
      use Item name=place.name key="place:{index}"
    if empty
      text "Nothing here"
    else
      text "{count} places"
    if mode == "map"
      text "Map mode"

component Item
  text name key=key
"##,
        );
        let model = json!({
            "places": [{"name": "Java Haven"}, {"name": "Sushi Zen"}],
            "empty": false,
            "count": 2,
            "mode": "map",
        });
        let root = views.render("List", &model);
        let mut out = Vec::new();
        texts(&root, &mut out);
        assert_eq!(out, ["Java Haven", "Sushi Zen", "2 places", "Map mode"]);
        assert_eq!(root.children[1].key.as_deref(), Some("place:1"));
    }

    #[test]
    fn components_do_not_see_the_callers_loop_variables() {
        let views = parse(
            r##"
component Outer
  column
    for item in items
      use Inner

component Inner
  text "{item}"
"##,
        );
        let root = views.render("Outer", &json!({"items": ["a"]}));
        assert_eq!(root.children[0].props.text.as_deref(), Some(""));
    }

    #[test]
    fn errors_point_at_the_line() {
        let err = |source: &str| Views::parse(&[("bad.view", source)]).unwrap_err();
        let e = err("component A\n  column\n    text \"x\" sizee=3\n");
        assert_eq!(
            (e.line, e.message.as_str()),
            (3, "unknown attribute `sizee`")
        );
        assert!(err("component A\n  colum\n")
            .message
            .contains("unknown element"));
        assert!(err("component A\n  text \"unterminated\n")
            .message
            .contains("unterminated"));
        assert!(err("component A\n  use Missing\n")
            .message
            .contains("unknown component"));
        assert!(
            err("component A\n  column\n      text \"x\"\n    text \"y\"\n")
                .message
                .contains("indent")
        );
    }

    #[test]
    fn styles_expand_in_place_and_later_attributes_win() {
        let views = parse(
            r##"
component A
  column
    text "x" .body color="#ff0000"
    text "y" color="#00ff00" .body

style base = size=14 color="#ffffff"
style body = .base font="satoshi-medium" height=18
"##,
        );
        let root = views.render("A", &json!({}));
        let (x, y) = (&root.children[0].props, &root.children[1].props);
        assert_eq!(x.text.as_deref(), Some("x"));
        assert_eq!((x.size, x.height), (Some(14.0), Some(18.0)));
        assert_eq!(x.font.as_deref(), Some("satoshi-medium"));
        assert_eq!(x.color.as_deref(), Some("#ff0000"));
        assert_eq!(y.color.as_deref(), Some("#ffffff"));

        let err = |source: &str| Views::parse(&[("bad.view", source)]).unwrap_err().message;
        assert!(err("component A
  text \"x\" .nope
")
        .contains("unknown style `.nope`"));
        assert!(err("style s = sizee=1
")
        .contains("unknown attribute"));
        assert!(err("style s = .later
style later = size=1
")
        .contains("unknown style"));
    }

    #[test]
    fn doubled_braces_are_literal() {
        let views = parse(
            r##"
component Icon
  native "icon" native_props="{{\"name\":\"{name}\"}}"
"##,
        );
        let root = views.render("Icon", &json!({"name": "car"}));
        assert_eq!(
            root.props.native_props.as_deref(),
            Some(r#"{"name":"car"}"#)
        );
    }

    #[test]
    fn fixed_values_keep_their_order_with_ones_that_change() {
        let views = parse(
            r##"
const WIDE = 300
const NAME = "from const"

component A
  column
    row width=w width=5 border=1 border_color=tint border_color="#ffffff"
    row width=5 width=w
    row bg=dark ? BG : "#fff" width=WIDE
    use B NAME="from argument"
    for WIDE in items
      row width=WIDE

const BG = "#000"

component B
  text NAME
"##,
        );
        let model = json!({"w": 9, "tint": "#ff0000", "dark": true, "items": [7]});
        let root = views.render("A", &model);
        let kids: Vec<&Props> = root.children.iter().map(|child| &child.props).collect();
        assert_eq!(kids[0].width, Some(5.0));
        assert_eq!(kids[0].border_color.as_deref(), Some("#ffffff"));
        assert_eq!(kids[0].border_width, Some(1.0));
        assert_eq!(kids[1].width, Some(9.0));
        assert_eq!(kids[2].bg.as_deref(), Some("#000"));
        // Arguments and loop variables hide a const of the same name.
        assert_eq!(kids[2].width, Some(300.0));
        assert_eq!(kids[3].text.as_deref(), Some("from argument"));
        assert_eq!(kids[4].width, Some(7.0));
        // A value of the wrong type leaves what was there.
        let root = views.render("A", &json!({"w": "wide"}));
        assert_eq!(root.children[1].props.width, Some(5.0));
    }

    #[test]
    fn missing_values_render_empty_not_panic() {
        let views = parse("component A\n  text missing.path width=nope\n");
        let root = views.render("A", &json!({}));
        assert_eq!(root.props.text.as_deref(), Some(""));
        assert_eq!(root.props.width, None);
        assert_eq!(
            views.render("Nope", &json!({})).props.color.as_deref(),
            Some("#ff5555")
        );
    }
}
