use serde::Serialize;
#[cfg(feature = "source-map")]
use std::panic::Location;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

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
    Sheet,
    Map,
    Image,
    HScroll,
    Native,
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
            Kind::Sheet => "sheet",
            Kind::Map => "map",
            Kind::Image => "image",
            Kind::HScroll => "hscroll",
            Kind::Native => "native",
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
    pub pad_bottom: Option<f32>,
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
    pub font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder_color: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_latitude: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_longitude: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_latitude_delta: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_longitude_delta: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_markers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_dark: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_shows_user: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_props: Option<String>,
}

impl Props {
    /// Build the subset of properties that changed since `previous`.
    ///
    /// A removed property has no representation in the sparse update, so it
    /// falls back to a full update instead of leaving a stale native value.
    pub(crate) fn delta_from(&self, previous: &Self) -> Option<Self> {
        let mut delta = Self::default();
        let mut changed = false;

        macro_rules! changed_field {
            ($field:ident) => {
                if self.$field != previous.$field {
                    self.$field.as_ref()?;
                    delta.$field = self.$field.clone();
                    changed = true;
                }
            };
        }

        changed_field!(text);
        changed_field!(size);
        changed_field!(color);
        changed_field!(bg);
        changed_field!(pad);
        changed_field!(pad_h);
        changed_field!(pad_v);
        changed_field!(pad_bottom);
        changed_field!(radius);
        changed_field!(weight);
        changed_field!(width);
        changed_field!(height);
        changed_field!(bold);
        changed_field!(font);
        changed_field!(line_height);
        changed_field!(lines);
        changed_field!(border_width);
        changed_field!(border_color);
        changed_field!(opacity);
        changed_field!(shadow);
        changed_field!(clip);
        changed_field!(animate);
        changed_field!(wrap);
        changed_field!(placeholder_color);
        changed_field!(align);
        changed_field!(placeholder);
        changed_field!(value);
        changed_field!(label);
        changed_field!(test_id);
        changed_field!(gap);
        changed_field!(align_items);
        changed_field!(justify);
        changed_field!(absolute);
        changed_field!(top);
        changed_field!(left);
        changed_field!(content_height);
        changed_field!(scroll_to);
        changed_field!(glass_style);
        changed_field!(glass_tint);
        changed_field!(glass_interactive);
        changed_field!(content_inset_top);
        changed_field!(content_inset_bottom);
        changed_field!(bottom);
        changed_field!(map_latitude);
        changed_field!(map_longitude);
        changed_field!(map_latitude_delta);
        changed_field!(map_longitude_delta);
        changed_field!(map_markers);
        changed_field!(map_dark);
        changed_field!(map_shows_user);
        changed_field!(paged);
        changed_field!(scroll_x);
        changed_field!(src);
        changed_field!(fit);
        changed_field!(content_width);
        changed_field!(pressable);
        changed_field!(native);
        changed_field!(native_props);

        changed.then_some(delta)
    }

    #[track_caller]
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

    pub fn pad_bottom(mut self, value: f32) -> Self {
        self.pad_bottom = Some(value);
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

    pub fn font(mut self, value: impl Into<String>) -> Self {
        self.font = Some(value.into());
        self
    }

    pub fn line_height(mut self, value: f32) -> Self {
        self.line_height = Some(value);
        self
    }

    pub fn lines(mut self, value: u32) -> Self {
        self.lines = Some(value);
        self
    }

    pub fn border(mut self, width: f32, color: impl Into<String>) -> Self {
        self.border_width = Some(width);
        self.border_color = Some(color.into());
        self
    }

    pub fn opacity(mut self, value: f32) -> Self {
        self.opacity = Some(value);
        self
    }

    pub fn shadow(mut self, value: bool) -> Self {
        self.shadow = Some(value);
        self
    }

    pub fn clip(mut self, value: bool) -> Self {
        self.clip = Some(value);
        self
    }

    pub fn animate(mut self, value: bool) -> Self {
        self.animate = Some(value);
        self
    }

    pub fn wrap(mut self, value: bool) -> Self {
        self.wrap = Some(value);
        self
    }

    pub fn placeholder_color(mut self, value: impl Into<String>) -> Self {
        self.placeholder_color = Some(value.into());
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

    pub fn map_region(
        mut self,
        latitude: f32,
        longitude: f32,
        latitude_delta: f32,
        longitude_delta: f32,
    ) -> Self {
        self.map_latitude = Some(latitude);
        self.map_longitude = Some(longitude);
        self.map_latitude_delta = Some(latitude_delta);
        self.map_longitude_delta = Some(longitude_delta);
        self
    }

    pub fn map_markers(mut self, value: impl Into<String>) -> Self {
        self.map_markers = Some(value.into());
        self
    }

    pub fn map_dark(mut self, value: bool) -> Self {
        self.map_dark = Some(value);
        self
    }

    pub fn src(mut self, value: impl Into<String>) -> Self {
        self.src = Some(value.into());
        self
    }

    pub fn fit(mut self, value: impl Into<String>) -> Self {
        self.fit = Some(value.into());
        self
    }

    pub fn content_width(mut self, value: f32) -> Self {
        self.content_width = Some(value);
        self
    }

    pub fn pressable(mut self, value: bool) -> Self {
        self.pressable = Some(value);
        self
    }

    pub fn native(mut self, module: impl Into<String>) -> Self {
        self.native = Some(module.into());
        self
    }

    pub fn native_props(mut self, value: impl Into<String>) -> Self {
        self.native_props = Some(value.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct Element {
    pub kind: Kind,
    pub key: Option<String>,
    pub props: Props,
    pub source: Option<SourceLocation>,
    pub children: Vec<Element>,
}

impl Element {
    #[track_caller]
    pub fn new(kind: Kind) -> Self {
        #[cfg(feature = "source-map")]
        let source = {
            let location = Location::caller();
            Some(SourceLocation {
                file: location.file().to_string(),
                line: location.line(),
                column: location.column(),
            })
        };
        #[cfg(not(feature = "source-map"))]
        let source = None;

        Self {
            kind,
            key: None,
            props: Props::default(),
            source,
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

    pub fn pad_bottom(mut self, value: f32) -> Self {
        self.props.pad_bottom = Some(value);
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

    pub fn font(mut self, value: impl Into<String>) -> Self {
        self.props.font = Some(value.into());
        self
    }

    pub fn line_height(mut self, value: f32) -> Self {
        self.props.line_height = Some(value);
        self
    }

    pub fn lines(mut self, value: u32) -> Self {
        self.props.lines = Some(value);
        self
    }

    pub fn border(mut self, width: f32, color: impl Into<String>) -> Self {
        self.props.border_width = Some(width);
        self.props.border_color = Some(color.into());
        self
    }

    pub fn opacity(mut self, value: f32) -> Self {
        self.props.opacity = Some(value);
        self
    }

    pub fn shadow(mut self, value: bool) -> Self {
        self.props.shadow = Some(value);
        self
    }

    pub fn clip(mut self, value: bool) -> Self {
        self.props.clip = Some(value);
        self
    }

    pub fn animate(mut self, value: bool) -> Self {
        self.props.animate = Some(value);
        self
    }

    pub fn wrap(mut self, value: bool) -> Self {
        self.props.wrap = Some(value);
        self
    }

    pub fn placeholder_color(mut self, value: impl Into<String>) -> Self {
        self.props.placeholder_color = Some(value.into());
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

    pub fn map_region(
        mut self,
        latitude: f32,
        longitude: f32,
        latitude_delta: f32,
        longitude_delta: f32,
    ) -> Self {
        self.props.map_latitude = Some(latitude);
        self.props.map_longitude = Some(longitude);
        self.props.map_latitude_delta = Some(latitude_delta);
        self.props.map_longitude_delta = Some(longitude_delta);
        self
    }

    pub fn map_markers(mut self, value: impl Into<String>) -> Self {
        self.props.map_markers = Some(value.into());
        self
    }

    pub fn map_dark(mut self, value: bool) -> Self {
        self.props.map_dark = Some(value);
        self
    }

    pub fn map_shows_user(mut self, value: bool) -> Self {
        self.props.map_shows_user = Some(value);
        self
    }

    pub fn paged(mut self, value: bool) -> Self {
        self.props.paged = Some(value);
        self
    }

    pub fn scroll_x(mut self, value: f32) -> Self {
        self.props.scroll_x = Some(value);
        self
    }

    pub fn src(mut self, value: impl Into<String>) -> Self {
        self.props.src = Some(value.into());
        self
    }

    pub fn fit(mut self, value: impl Into<String>) -> Self {
        self.props.fit = Some(value.into());
        self
    }

    pub fn content_width(mut self, value: f32) -> Self {
        self.props.content_width = Some(value);
        self
    }

    pub fn pressable(mut self, value: bool) -> Self {
        self.props.pressable = Some(value);
        self
    }

    pub fn native(mut self, module: impl Into<String>) -> Self {
        self.props.native = Some(module.into());
        self
    }

    pub fn native_props(mut self, value: impl Into<String>) -> Self {
        self.props.native_props = Some(value.into());
        self
    }
}

#[track_caller]
pub fn column() -> Element {
    Element::new(Kind::Column)
}

#[track_caller]
pub fn row() -> Element {
    Element::new(Kind::Row)
}

#[track_caller]
pub fn text(value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Text);
    element.props.text = Some(value.into());
    element
}

#[track_caller]
pub fn button(value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Button);
    element.props.text = Some(value.into());
    element
}

#[track_caller]
pub fn scroll() -> Element {
    Element::new(Kind::Scroll)
}

#[track_caller]
pub fn list() -> Element {
    Element::new(Kind::List)
}

#[track_caller]
pub fn glass() -> Element {
    Element::new(Kind::Glass)
}

#[track_caller]
pub fn sheet() -> Element {
    native("sheet")
}

#[track_caller]
pub fn map() -> Element {
    native("map")
}

#[track_caller]
pub fn image(src: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Image);
    element.props.src = Some(src.into());
    element
}

#[track_caller]
pub fn hscroll() -> Element {
    Element::new(Kind::HScroll)
}

#[track_caller]
pub fn native(module: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Native);
    element.props.native = Some(module.into());
    element
}

#[track_caller]
pub fn input(placeholder: impl Into<String>, value: impl Into<String>) -> Element {
    let mut element = Element::new(Kind::Input);
    element.props.placeholder = Some(placeholder.into());
    element.props.value = Some(value.into());
    element
}

#[track_caller]
pub fn spacer() -> Element {
    Element::new(Kind::Space).weight(1.0)
}

#[track_caller]
pub fn gap_w(size: f32) -> Element {
    Element::new(Kind::Space).width(size)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "source-map")]
    #[test]
    fn constructors_capture_call_site() {
        let element = super::column();
        let source = element.source.expect("source location");
        assert!(source.file.ends_with("element.rs"));
        assert!(source.line > 0);
    }
}
