use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Column,
    Row,
    Text,
    Button,
    Scroll,
    List,
    Glass,
    Input,
    Space,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Column => "column",
            Kind::Row => "row",
            Kind::Text => "text",
            Kind::Button => "button",
            Kind::Scroll => "scroll",
            Kind::List => "list",
            Kind::Glass => "glass",
            Kind::Input => "input",
            Kind::Space => "space",
        }
    }
}

/// The property vocabulary shared with the platform host.
/// Width/height are in density-independent pixels; -1 means match-parent,
/// -2 means wrap-content.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Props {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad_h: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad_v: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_to: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass_tint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass_interactive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_inset_top: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_inset_bottom: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f32>,
}

impl Props {
    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    pub fn size(mut self, value: f32) -> Self {
        self.size = Some(value);
        self
    }

    pub fn color(mut self, value: impl Into<String>) -> Self {
        self.color = Some(value.into());
        self
    }

    pub fn bg(mut self, value: impl Into<String>) -> Self {
        self.bg = Some(value.into());
        self
    }

    pub fn pad(mut self, value: f32) -> Self {
        self.pad = Some(value);
        self
    }

    pub fn pad_h(mut self, value: f32) -> Self {
        self.pad_h = Some(value);
        self
    }

    pub fn pad_v(mut self, value: f32) -> Self {
        self.pad_v = Some(value);
        self
    }

    pub fn radius(mut self, value: f32) -> Self {
        self.radius = Some(value);
        self
    }

    pub fn weight(mut self, value: f32) -> Self {
        self.weight = Some(value);
        self
    }

    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(value);
        self
    }

    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(value);
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.bold = Some(value);
        self
    }

    pub fn align(mut self, value: impl Into<String>) -> Self {
        self.align = Some(value.into());
        self
    }

    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = Some(value.into());
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = Some(value.into());
        self
    }

    pub fn test_id(mut self, value: impl Into<String>) -> Self {
        self.test_id = Some(value.into());
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.gap = Some(value);
        self
    }

    pub fn align_items(mut self, value: impl Into<String>) -> Self {
        self.align_items = Some(value.into());
        self
    }

    pub fn justify(mut self, value: impl Into<String>) -> Self {
        self.justify = Some(value.into());
        self
    }

    pub fn absolute(mut self, value: bool) -> Self {
        self.absolute = Some(value);
        self
    }

    pub fn top(mut self, value: f32) -> Self {
        self.top = Some(value);
        self
    }

    pub fn left(mut self, value: f32) -> Self {
        self.left = Some(value);
        self
    }

    pub fn content_height(mut self, value: f32) -> Self {
        self.content_height = Some(value);
        self
    }

    pub fn scroll_to(mut self, value: f32) -> Self {
        self.scroll_to = Some(value);
        self
    }

    pub fn glass_style(mut self, value: impl Into<String>) -> Self {
        self.glass_style = Some(value.into());
        self
    }

    pub fn glass_tint(mut self, value: impl Into<String>) -> Self {
        self.glass_tint = Some(value.into());
        self
    }

    pub fn glass_interactive(mut self, value: bool) -> Self {
        self.glass_interactive = Some(value);
        self
    }

    pub fn content_inset_top(mut self, value: f32) -> Self {
        self.content_inset_top = Some(value);
        self
    }

    pub fn content_inset_bottom(mut self, value: f32) -> Self {
        self.content_inset_bottom = Some(value);
        self
    }

    pub fn bottom(mut self, value: f32) -> Self {
        self.bottom = Some(value);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Element {
    pub kind: Kind,
    pub key: Option<String>,
    pub props: Props,
    pub children: Vec<Element>,
}

impl Element {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            key: None,
            props: Props::default(),
            children: Vec::new(),
        }
    }

    pub fn key(mut self, value: impl Into<String>) -> Self {
        self.key = Some(value.into());
        self
    }

    pub fn prop(mut self, props: Props) -> Self {
        self.props = props;
        self
    }

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn size(mut self, value: f32) -> Self {
        self.props.size = Some(value);
        self
    }

    pub fn color(mut self, value: impl Into<String>) -> Self {
        self.props.color = Some(value.into());
        self
    }

    pub fn bg(mut self, value: impl Into<String>) -> Self {
        self.props.bg = Some(value.into());
        self
    }

    pub fn pad(mut self, value: f32) -> Self {
        self.props.pad = Some(value);
        self
    }

    pub fn pad_h(mut self, value: f32) -> Self {
        self.props.pad_h = Some(value);
        self
    }

    pub fn pad_v(mut self, value: f32) -> Self {
        self.props.pad_v = Some(value);
        self
    }

    pub fn radius(mut self, value: f32) -> Self {
        self.props.radius = Some(value);
        self
    }

    pub fn weight(mut self, value: f32) -> Self {
        self.props.weight = Some(value);
        self
    }

    pub fn width(mut self, value: f32) -> Self {
        self.props.width = Some(value);
        self
    }

    pub fn height(mut self, value: f32) -> Self {
        self.props.height = Some(value);
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.props.bold = Some(value);
        self
    }

    pub fn align(mut self, value: impl Into<String>) -> Self {
        self.props.align = Some(value.into());
        self
    }

    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.props.placeholder = Some(value.into());
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.props.value = Some(value.into());
        self
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.props.label = Some(value.into());
        self
    }

    pub fn test_id(mut self, value: impl Into<String>) -> Self {
        self.props.test_id = Some(value.into());
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.props.gap = Some(value);
        self
    }

    pub fn align_items(mut self, value: impl Into<String>) -> Self {
        self.props.align_items = Some(value.into());
        self
    }

    pub fn justify(mut self, value: impl Into<String>) -> Self {
        self.props.justify = Some(value.into());
        self
    }

    pub fn absolute(mut self) -> Self {
        self.props.absolute = Some(true);
        self
    }

    pub fn top(mut self, value: f32) -> Self {
        self.props.top = Some(value);
        self
    }

    pub fn left(mut self, value: f32) -> Self {
        self.props.left = Some(value);
        self
    }

    pub fn content_height(mut self, value: f32) -> Self {
        self.props.content_height = Some(value);
        self
    }

    pub fn scroll_to(mut self, value: f32) -> Self {
        self.props.scroll_to = Some(value);
        self
    }

    pub fn glass_style(mut self, value: impl Into<String>) -> Self {
        self.props.glass_style = Some(value.into());
        self
    }

    pub fn glass_tint(mut self, value: impl Into<String>) -> Self {
        self.props.glass_tint = Some(value.into());
        self
    }

    pub fn glass_interactive(mut self, value: bool) -> Self {
        self.props.glass_interactive = Some(value);
        self
    }

    pub fn content_inset_top(mut self, value: f32) -> Self {
        self.props.content_inset_top = Some(value);
        self
    }

    pub fn content_inset_bottom(mut self, value: f32) -> Self {
        self.props.content_inset_bottom = Some(value);
        self
    }

    pub fn bottom(mut self, value: f32) -> Self {
        self.props.bottom = Some(value);
        self
    }
}

pub fn column() -> Element {
    Element::new(Kind::Column)
}

pub fn row() -> Element {
    Element::new(Kind::Row)
}

pub fn text(value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Text);
    element.props.text = Some(value.into());
    element
}

pub fn button(value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Button);
    element.props.text = Some(value.into());
    element
}

pub fn scroll() -> Element {
    Element::new(Kind::Scroll)
}

pub fn list() -> Element {
    Element::new(Kind::List)
}

pub fn glass() -> Element {
    Element::new(Kind::Glass)
}

pub fn input(placeholder: impl Into<String>, value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Input);
    element.props.placeholder = Some(placeholder.into());
    element.props.value = Some(value.into());
    element
}

pub fn spacer() -> Element {
    Element::new(Kind::Space).weight(1.0)
}

pub fn gap_w(size: f32) -> Element {
    Element::new(Kind::Space).width(size)
}
