//! Compact commit wire format shared with the native hosts.
//!
//! The format is deliberately boring: a magic header, a count, then a
//! sequence of tagged operations. Strings are length-prefixed UTF-8. It is
//! only a transport format; the Rust `Op`/`Props` types remain the source of
//! truth.

use crate::diff::Op;
use crate::element::Props;

pub const MAGIC: &[u8; 4] = b"CHT1";

const OP_CREATE: u8 = 1;
const OP_UPDATE: u8 = 2;
const OP_CHILDREN: u8 = 3;
const OP_LAYOUT: u8 = 4;
const OP_REMOVE: u8 = 5;
const OP_TEXT: u8 = 6;
const OP_UPDATE_PROPS: u8 = 7;
const OP_INVOKE: u8 = 8;

const TYPE_STRING: u8 = 1;
const TYPE_F32: u8 = 2;
const TYPE_BOOL: u8 = 3;
const TYPE_U32: u8 = 4;

const PROP_NAMES: &[&str] = &[
    "text",
    "size",
    "color",
    "bg",
    "pad",
    "padH",
    "padV",
    "padBottom",
    "radius",
    "weight",
    "width",
    "height",
    "bold",
    "font",
    "lineHeight",
    "lines",
    "borderWidth",
    "borderColor",
    "opacity",
    "shadow",
    "clip",
    "animate",
    "wrap",
    "placeholderColor",
    "align",
    "placeholder",
    "value",
    "label",
    "testId",
    "gap",
    "alignItems",
    "justify",
    "absolute",
    "top",
    "left",
    "contentHeight",
    "scrollTo",
    "glassStyle",
    "glassTint",
    "glassInteractive",
    "contentInsetTop",
    "contentInsetBottom",
    "bottom",
    "mapLatitude",
    "mapLongitude",
    "mapLatitudeDelta",
    "mapLongitudeDelta",
    "mapMarkers",
    "mapDark",
    "mapShowsUser",
    "paged",
    "scrollX",
    "src",
    "fit",
    "contentWidth",
    "pressable",
    "native",
    "nativeProps",
    "translateX",
    "translateY",
    "scale",
    "rotate",
    "transitionMs",
    "spring",
    "gestures",
    "refresh",
    "refreshing",
    // Append only: hosts index this list, and skip ids they don't know.
    "pageScale",
    "drag",
    "dragSnaps",
    "dragEdge",
];

pub(crate) fn encode(ops: &[Op]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ops.len() * 64 + 16);
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, ops.len() as u32);

    for op in ops {
        match op {
            Op::Create {
                id,
                kind,
                props,
                source,
                root,
            } => {
                out.push(OP_CREATE);
                put_u32(&mut out, *id);
                put_string(&mut out, kind.as_str());
                out.push(if *root { 1 } else { 0 });
                put_source(&mut out, source.as_ref());
                put_props(&mut out, props);
            }
            Op::Update { id, props } => {
                out.push(OP_UPDATE);
                put_u32(&mut out, *id);
                put_props(&mut out, props);
            }
            Op::UpdateProps { id, props } => {
                out.push(OP_UPDATE_PROPS);
                put_u32(&mut out, *id);
                put_props(&mut out, props);
            }
            Op::Text { id, text } => {
                out.push(OP_TEXT);
                put_u32(&mut out, *id);
                put_string(&mut out, text);
            }
            Op::Children { id, children } => {
                out.push(OP_CHILDREN);
                put_u32(&mut out, *id);
                put_u32(&mut out, children.len() as u32);
                for child in children {
                    put_u32(&mut out, *child);
                }
            }
            Op::Layout {
                id,
                x,
                y,
                width,
                height,
            } => {
                out.push(OP_LAYOUT);
                put_u32(&mut out, *id);
                put_f32(&mut out, *x);
                put_f32(&mut out, *y);
                put_f32(&mut out, *width);
                put_f32(&mut out, *height);
            }
            Op::Remove { id } => {
                out.push(OP_REMOVE);
                put_u32(&mut out, *id);
            }
            Op::Invoke {
                module,
                method,
                args,
            } => {
                out.push(OP_INVOKE);
                put_string(&mut out, module);
                put_string(&mut out, method);
                put_string(&mut out, args);
            }
        }
    }

    out
}

fn put_prop_id(out: &mut Vec<u8>, name: &str) {
    let id = PROP_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .expect("known prop name") as u16;
    out.extend_from_slice(&id.to_le_bytes());
}

fn put_source(out: &mut Vec<u8>, source: Option<&crate::element::SourceLocation>) {
    match source {
        Some(source) => {
            out.push(1);
            put_string(out, &source.file);
            put_u32(out, source.line);
            put_u32(out, source.column);
        }
        None => out.push(0),
    }
}

fn put_props(out: &mut Vec<u8>, props: &Props) {
    put_u32(out, prop_count(props) as u32);

    macro_rules! string_prop {
        ($name:literal, $value:expr) => {
            if let Some(value) = &$value {
                put_prop_id(out, $name);
                out.push(TYPE_STRING);
                put_string(out, value);
            }
        };
    }
    macro_rules! f32_prop {
        ($name:literal, $value:expr) => {
            if let Some(value) = $value {
                put_prop_id(out, $name);
                out.push(TYPE_F32);
                put_f32(out, value);
            }
        };
    }
    macro_rules! bool_prop {
        ($name:literal, $value:expr) => {
            if let Some(value) = $value {
                put_prop_id(out, $name);
                out.push(TYPE_BOOL);
                out.push(if value { 1 } else { 0 });
            }
        };
    }
    macro_rules! u32_prop {
        ($name:literal, $value:expr) => {
            if let Some(value) = $value {
                put_prop_id(out, $name);
                out.push(TYPE_U32);
                put_u32(out, value);
            }
        };
    }

    string_prop!("text", props.text);
    f32_prop!("size", props.size);
    string_prop!("color", props.color);
    string_prop!("bg", props.bg);
    f32_prop!("pad", props.pad);
    f32_prop!("padH", props.pad_h);
    f32_prop!("padV", props.pad_v);
    f32_prop!("padBottom", props.pad_bottom);
    f32_prop!("radius", props.radius);
    f32_prop!("weight", props.weight);
    f32_prop!("width", props.width);
    f32_prop!("height", props.height);
    bool_prop!("bold", props.bold);
    string_prop!("font", props.font);
    f32_prop!("lineHeight", props.line_height);
    u32_prop!("lines", props.lines);
    f32_prop!("borderWidth", props.border_width);
    string_prop!("borderColor", props.border_color);
    f32_prop!("opacity", props.opacity);
    bool_prop!("shadow", props.shadow);
    bool_prop!("clip", props.clip);
    bool_prop!("animate", props.animate);
    bool_prop!("wrap", props.wrap);
    string_prop!("placeholderColor", props.placeholder_color);
    string_prop!("align", props.align);
    string_prop!("placeholder", props.placeholder);
    string_prop!("value", props.value);
    string_prop!("label", props.label);
    string_prop!("testId", props.test_id);
    f32_prop!("gap", props.gap);
    string_prop!("alignItems", props.align_items);
    string_prop!("justify", props.justify);
    bool_prop!("absolute", props.absolute);
    f32_prop!("top", props.top);
    f32_prop!("left", props.left);
    f32_prop!("contentHeight", props.content_height);
    f32_prop!("scrollTo", props.scroll_to);
    string_prop!("glassStyle", props.glass_style);
    string_prop!("glassTint", props.glass_tint);
    bool_prop!("glassInteractive", props.glass_interactive);
    f32_prop!("contentInsetTop", props.content_inset_top);
    f32_prop!("contentInsetBottom", props.content_inset_bottom);
    f32_prop!("bottom", props.bottom);
    f32_prop!("mapLatitude", props.map_latitude);
    f32_prop!("mapLongitude", props.map_longitude);
    f32_prop!("mapLatitudeDelta", props.map_latitude_delta);
    f32_prop!("mapLongitudeDelta", props.map_longitude_delta);
    string_prop!("mapMarkers", props.map_markers);
    bool_prop!("mapDark", props.map_dark);
    bool_prop!("mapShowsUser", props.map_shows_user);
    bool_prop!("paged", props.paged);
    f32_prop!("scrollX", props.scroll_x);
    string_prop!("src", props.src);
    string_prop!("fit", props.fit);
    f32_prop!("contentWidth", props.content_width);
    bool_prop!("pressable", props.pressable);
    string_prop!("native", props.native);
    string_prop!("nativeProps", props.native_props);
    f32_prop!("translateX", props.translate_x);
    f32_prop!("translateY", props.translate_y);
    f32_prop!("scale", props.scale);
    f32_prop!("rotate", props.rotate);
    f32_prop!("transitionMs", props.transition_ms);
    bool_prop!("spring", props.spring);
    string_prop!("gestures", props.gestures);
    bool_prop!("refresh", props.refresh);
    bool_prop!("refreshing", props.refreshing);
    f32_prop!("pageScale", props.page_scale);
    string_prop!("drag", props.drag);
    string_prop!("dragSnaps", props.drag_snaps);
    f32_prop!("dragEdge", props.drag_edge);
}

fn prop_count(props: &Props) -> usize {
    let mut count = 0usize;
    macro_rules! count {
        ($value:expr) => {
            if $value.is_some() {
                count += 1;
            }
        };
    }
    count!(props.text);
    count!(props.size);
    count!(props.color);
    count!(props.bg);
    count!(props.pad);
    count!(props.pad_h);
    count!(props.pad_v);
    count!(props.pad_bottom);
    count!(props.radius);
    count!(props.weight);
    count!(props.width);
    count!(props.height);
    count!(props.bold);
    count!(props.font);
    count!(props.line_height);
    count!(props.lines);
    count!(props.border_width);
    count!(props.border_color);
    count!(props.opacity);
    count!(props.shadow);
    count!(props.clip);
    count!(props.animate);
    count!(props.wrap);
    count!(props.placeholder_color);
    count!(props.align);
    count!(props.placeholder);
    count!(props.value);
    count!(props.label);
    count!(props.test_id);
    count!(props.gap);
    count!(props.align_items);
    count!(props.justify);
    count!(props.absolute);
    count!(props.top);
    count!(props.left);
    count!(props.content_height);
    count!(props.scroll_to);
    count!(props.glass_style);
    count!(props.glass_tint);
    count!(props.glass_interactive);
    count!(props.content_inset_top);
    count!(props.content_inset_bottom);
    count!(props.bottom);
    count!(props.map_latitude);
    count!(props.map_longitude);
    count!(props.map_latitude_delta);
    count!(props.map_longitude_delta);
    count!(props.map_markers);
    count!(props.map_dark);
    count!(props.map_shows_user);
    count!(props.paged);
    count!(props.scroll_x);
    count!(props.src);
    count!(props.fit);
    count!(props.content_width);
    count!(props.pressable);
    count!(props.native);
    count!(props.native_props);
    count!(props.translate_x);
    count!(props.translate_y);
    count!(props.scale);
    count!(props.rotate);
    count!(props.transition_ms);
    count!(props.spring);
    count!(props.gestures);
    count!(props.refresh);
    count!(props.refreshing);
    count!(props.page_scale);
    count!(props.drag);
    count!(props.drag_snaps);
    count!(props.drag_edge);
    count
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    put_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{column, text};

    /// Props travel by number: each host must list them in this order.
    #[test]
    fn hosts_number_props_the_same_way() {
        let swift = include_str!("../../../ios/Cheetah/WireProps.swift");
        let body = swift.split("enum Prop:").nth(1).expect("the Prop enum");
        let body = body.split("static let").next().expect("its cases");
        let names: Vec<&str> = body
            .lines()
            .filter_map(|line| line.trim().strip_prefix("case "))
            .flat_map(|line| line.split(','))
            .map(str::trim)
            .collect();
        assert_eq!(names, PROP_NAMES);

        let kotlin = include_str!(
            "../../../android/app/src/main/java/com/example/rustui/RustHost.kt"
        );
        let list = kotlin.split("val propNames = arrayOf(").nth(1).expect("propNames");
        let list = list.split(')').next().expect("its end");
        let names: Vec<&str> = list.split(',').map(|name| name.trim().trim_matches('"')).collect();
        assert_eq!(names, PROP_NAMES);
    }

    #[test]
    fn encodes_sparse_property_update() {
        let ops = vec![Op::UpdateProps {
            id: 7,
            props: Props {
                bg: Some("#111111".into()),
                ..Props::default()
            },
        }];
        let bytes = encode(&ops);
        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(bytes[8], OP_UPDATE_PROPS);
        assert!(bytes.len() < 40);
    }

    #[test]
    fn encodes_native_invoke() {
        let ops = vec![Op::Invoke {
            module: "haptics".into(),
            method: "light".into(),
            args: "{}".into(),
        }];
        let bytes = encode(&ops);
        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(bytes[8], OP_INVOKE);
    }

    #[test]
    fn encodes_header_and_create() {
        let element = column().child(text("hello").width(20.0));
        let root = crate::tree::build(element);
        let ops = crate::diff::diff(None, &crate::tree::flatten(&root));
        let bytes = encode(&ops);
        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            ops.len()
        );
        assert!(bytes.len() > 16);
    }
}
