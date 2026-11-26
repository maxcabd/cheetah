//! Paint a laid-out tree the way the iOS host shows it: views, text, glass,
//! images, the map with its pins, and the native modules apps use (photo,
//! icon, shape, gradient, blur, sheet, page), drawn as close stand-ins.

use crate::icons;
use crate::svg;
use crate::text::{self, Fonts, TextStyle};
use runtime::element::{Kind, Props};
use runtime::layout::Frame;
use runtime::tree::UiNode;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern, Pixmap,
    Point, Rect, SpreadMode, Stroke, Transform,
};

pub struct Painter<'a> {
    pub fonts: &'a Fonts,
    pub frames: &'a HashMap<u32, Frame>,
    pub assets: PathBuf,
    /// Pixels per point.
    pub scale: f32,
    /// What was drawn where, for `--tree`.
    pub drawn: Vec<Value>,
    images: HashMap<String, Option<Pixmap>>,
}

/// Where a node is drawn: its origin in points, and what it inherits.
#[derive(Clone)]
struct Place {
    x: f32,
    y: f32,
    opacity: f32,
    mask: Option<Mask>,
}

impl<'a> Painter<'a> {
    pub fn new(
        fonts: &'a Fonts,
        frames: &'a HashMap<u32, Frame>,
        assets: PathBuf,
        scale: f32,
    ) -> Self {
        Self {
            fonts,
            frames,
            assets,
            scale,
            drawn: Vec::new(),
            images: HashMap::new(),
        }
    }

    pub fn paint(&mut self, pixmap: &mut Pixmap, root: &UiNode) {
        let place = Place {
            x: 0.0,
            y: 0.0,
            opacity: 1.0,
            mask: None,
        };
        self.node(pixmap, root, &place, None);
    }

    fn node(&mut self, pixmap: &mut Pixmap, node: &UiNode, parent: &Place, at: Option<(f32, f32)>) {
        let props = &*node.props;
        let Some(frame) = self.frames.get(&node.id).copied() else {
            return;
        };
        let opacity = parent.opacity * props.opacity.unwrap_or(1.0);
        if opacity <= 0.01 || hidden(props) {
            return;
        }
        // A map pin goes where its coordinate is (`at`), not where layout
        // put it.
        let (x, y) = match at {
            Some(point) => point,
            None => (
                parent.x + frame.x + props.translate_x.unwrap_or(0.0),
                parent.y + frame.y + props.translate_y.unwrap_or(0.0),
            ),
        };
        let (width, height) = (frame.width, frame.height);
        let radius = props
            .radius
            .unwrap_or(0.0)
            .min(width / 2.0)
            .min(height / 2.0)
            .max(0.0);
        let rect = (x, y, width, height);
        let mask = parent.mask.as_ref();
        self.record(node, rect);

        let native = props.native.as_deref().unwrap_or("");
        match (node.kind, native) {
            (Kind::Native, "sheet") => self.sheet(pixmap, mask, props, rect, opacity),
            (Kind::Native, "page") => {
                let background = config(props)["background"].as_str().map(str::to_string);
                self.fill(
                    pixmap,
                    mask,
                    rect,
                    0.0,
                    color(background.as_deref().unwrap_or("#0b0b0f"), opacity),
                );
            }
            (Kind::Native, "photo") => self.photo(pixmap, mask, props, rect, opacity),
            (Kind::Native, "icon") => self.icon(pixmap, mask, props, rect, opacity),
            (Kind::Native, "shape") => self.shape(pixmap, mask, props, rect, opacity),
            (Kind::Native, "gradient") => self.gradient(pixmap, mask, props, rect, opacity),
            (Kind::Native, "blur") => {
                self.fill(pixmap, mask, rect, radius, color("#0b0b0f8c", opacity))
            }
            (Kind::Map, _) | (Kind::Native, "map") => self.map(pixmap, mask, props, rect, opacity),
            (Kind::Glass, _) => {
                if let Some(bg) = props.bg.as_deref() {
                    self.fill(pixmap, mask, rect, radius, color(bg, opacity));
                }
                // Liquid Glass over a dark screen reads as a frosted, mostly
                // solid disc: a dark frost, a light sheen, then its tint.
                self.fill(pixmap, mask, rect, radius, color("#2a2a30c7", opacity));
                self.fill(pixmap, mask, rect, radius, color("#ffffff14", opacity));
                if let Some(tint) = props.glass_tint.as_deref() {
                    self.fill(pixmap, mask, rect, radius, color(tint, opacity));
                }
                self.border(pixmap, mask, rect, radius, 1.0, color("#ffffff2e", opacity));
            }
            _ => {
                if let Some(bg) = props.bg.as_deref() {
                    self.fill(pixmap, mask, rect, radius, color(bg, opacity));
                }
            }
        }
        if let Some(border) = props.border_width.filter(|width| *width > 0.0) {
            let border_color = props.border_color.as_deref().unwrap_or("#ffffff");
            self.border(
                pixmap,
                mask,
                rect,
                radius,
                border,
                color(border_color, opacity),
            );
        }
        match node.kind {
            Kind::Text | Kind::Button => self.label(
                pixmap,
                mask,
                props,
                rect,
                opacity,
                props.text.as_deref().unwrap_or(""),
            ),
            Kind::Input => {
                let value = props.value.as_deref().filter(|value| !value.is_empty());
                let (shown, tone) = match value {
                    Some(value) => (value, props.color.as_deref().unwrap_or("#ffffff")),
                    None => (
                        props.placeholder.as_deref().unwrap_or(""),
                        props.placeholder_color.as_deref().unwrap_or("#a3a3a3"),
                    ),
                };
                let mut input = props.clone();
                input.color = Some(tone.to_string());
                self.label(pixmap, mask, &input, rect, opacity, shown);
            }
            Kind::Image => {
                if let Some(source) = props.src.as_deref() {
                    let contain = props.fit.as_deref() == Some("contain");
                    self.image(pixmap, mask, source, rect, radius, opacity, contain);
                }
            }
            _ => {}
        }

        // Children: clipped where the host clips, scrolled where it scrolls.
        let scroll = matches!(node.kind, Kind::Scroll | Kind::HScroll | Kind::List);
        let clips = if scroll {
            props.clip != Some(false)
        } else {
            props.clip == Some(true)
                || (radius > 0.0 && node.kind != Kind::Native)
                || native == "map"
        };
        let child_mask = if clips {
            let path = rounded(self.px(rect), radius * self.scale);
            path.map(|path| {
                let mut clip = match &parent.mask {
                    Some(mask) => mask.clone(),
                    None => {
                        let mut full = Mask::new(pixmap.width(), pixmap.height()).expect("mask");
                        full.fill_path(
                            &PathBuilder::from_rect(
                                Rect::from_xywh(
                                    0.0,
                                    0.0,
                                    pixmap.width() as f32,
                                    pixmap.height() as f32,
                                )
                                .unwrap(),
                            ),
                            FillRule::Winding,
                            false,
                            Transform::identity(),
                        );
                        full
                    }
                };
                clip.intersect_path(&path, FillRule::Winding, true, Transform::identity());
                clip
            })
        } else {
            parent.mask.clone()
        };
        let place = Place {
            x: x - if node.kind == Kind::HScroll {
                props.scroll_x.unwrap_or(0.0)
            } else {
                0.0
            },
            y: y - if node.kind == Kind::Scroll {
                props.scroll_to.unwrap_or(0.0)
            } else {
                0.0
            },
            opacity,
            mask: child_mask,
        };
        let is_map = node.kind == Kind::Map || native == "map";
        for child in &node.children {
            let pin = if is_map {
                self.pin_point(props, rect, child)
            } else {
                None
            };
            self.node(pixmap, child, &place, pin);
        }
    }

    fn record(&mut self, node: &UiNode, (x, y, width, height): (f32, f32, f32, f32)) {
        let props = &*node.props;
        let interesting = node.key.is_some()
            || props.text.is_some()
            || props.label.is_some()
            || props.test_id.is_some();
        if !interesting {
            return;
        }
        let mut entry = serde_json::json!({
            "kind": node.kind.as_str(),
            "frame": [round(x), round(y), round(width), round(height)],
        });
        for (name, value) in [
            ("key", node.key.as_deref()),
            ("text", props.text.as_deref()),
            ("label", props.label.as_deref()),
            ("testId", props.test_id.as_deref()),
            ("native", props.native.as_deref()),
        ] {
            if let Some(value) = value {
                entry[name] = value.into();
            }
        }
        if props.pressable == Some(true) || props.link.is_some() {
            entry["pressable"] = true.into();
        }
        self.drawn.push(entry);
    }

    fn px(&self, (x, y, width, height): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        (
            x * self.scale,
            y * self.scale,
            width * self.scale,
            height * self.scale,
        )
    }

    fn fill(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        rect: (f32, f32, f32, f32),
        radius: f32,
        color: Color,
    ) {
        if let Some(path) = rounded(self.px(rect), radius * self.scale) {
            let mut paint = Paint::default();
            paint.set_color(color);
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
    }

    fn border(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        rect: (f32, f32, f32, f32),
        radius: f32,
        width: f32,
        color: Color,
    ) {
        let inset = width / 2.0;
        let (x, y, w, h) = rect;
        let inner = (x + inset, y + inset, w - width, h - width);
        if let Some(path) = rounded(self.px(inner), (radius - inset).max(0.0) * self.scale) {
            let mut paint = Paint::default();
            paint.set_color(color);
            paint.anti_alias = true;
            let stroke = Stroke {
                width: width * self.scale,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
        }
    }

    fn label(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
        content: &str,
    ) {
        let (x, y, width, height) = rect;
        let base = props.pad.unwrap_or(0.0);
        let horizontal = props.pad_h.unwrap_or(base);
        let vertical = props.pad_v.unwrap_or(base);
        let style = TextStyle {
            face: self
                .fonts
                .face(props.font.as_deref(), props.bold == Some(true)),
            size: props.size.unwrap_or(14.0).max(1.0),
            color: color(props.color.as_deref().unwrap_or("#ffffff"), opacity),
            align: props.align.as_deref().unwrap_or("start"),
            lines: props.lines.unwrap_or(0) as usize,
            line_height: props.line_height,
        };
        text::draw(
            pixmap,
            mask,
            self.scale,
            content,
            &style,
            (
                x + horizontal,
                y + vertical,
                width - 2.0 * horizontal,
                height - 2.0 * vertical,
            ),
        );
    }

    fn sheet(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let config = config(props);
        let radius = config["cornerRadius"].as_f64().unwrap_or(28.0) as f32;
        let background = config["background"]
            .as_str()
            .unwrap_or("#171717")
            .to_string();
        // Square at the bottom, where the screen's own corners round it.
        let (x, y, width, height) = rect;
        self.fill(
            pixmap,
            mask,
            (x, y, width, height),
            radius,
            color(&background, opacity),
        );
        self.fill(
            pixmap,
            mask,
            (x, y + height / 2.0, width, height / 2.0),
            0.0,
            color(&background, opacity),
        );
    }

    fn icon(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let config = config(props);
        let name = config["name"].as_str().unwrap_or("circle");
        let tone = props
            .color
            .as_deref()
            .or(config["color"].as_str())
            .unwrap_or("#ffffff");
        let size = props.size.unwrap_or(rect.2.min(rect.3)) * 0.9;
        let mut paint = Paint::default();
        paint.set_color(color(tone, opacity));
        paint.anti_alias = true;
        let (x, y, w, h) = self.px(rect);
        icons::draw(
            pixmap,
            mask,
            name,
            x + w / 2.0,
            y + h / 2.0,
            size * self.scale,
            &paint,
        );
    }

    fn photo(
        &mut self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let config = config(props);
        let (source, pages) = match config["photos"]
            .as_array()
            .filter(|photos| !photos.is_empty())
        {
            Some(photos) => (photos[0].as_str().unwrap_or("").to_string(), photos.len()),
            None => (
                config["src"]
                    .as_str()
                    .unwrap_or("asset://second-cup-card-full.jpeg")
                    .to_string(),
                config["pages"].as_u64().unwrap_or(5) as usize,
            ),
        };
        self.image(pixmap, mask, &source, rect, 0.0, opacity, false);
        // The pager's fade into the card and its page dots.
        let (x, y, width, height) = rect;
        let fade = (x, y + height - 45.0, width, 45.0);
        self.linear(
            pixmap,
            mask,
            fade,
            &[
                ("#0b0b0f00", 0.0),
                ("#0b0b0f59", 0.5),
                ("#0b0b0fb3", 0.8),
                ("#1f1f1f", 1.0),
            ],
            (0.0, 0.0),
            (0.0, 1.0),
            opacity,
        );
        if pages > 1 {
            let spacing = 11.0;
            let start = x + width / 2.0 - spacing * (pages as f32 - 1.0) / 2.0;
            for index in 0..pages {
                let radius = if index == 0 { 2.5 * 1.55 } else { 2.5 };
                let alpha = if index == 0 { 1.0 } else { 0.22 };
                let (cx, cy) = (start + index as f32 * spacing, y + height - 12.0);
                let mut dot = color("#ffffff", opacity);
                dot.apply_opacity(alpha);
                self.fill(
                    pixmap,
                    mask,
                    (cx - radius, cy - radius, radius * 2.0, radius * 2.0),
                    radius,
                    dot,
                );
            }
        }
    }

    fn gradient(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let config = config(props);
        let colors: Vec<String> = config["colors"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if colors.is_empty() {
            return;
        }
        let locations: Vec<f32> = config["locations"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|l| l.as_f64().map(|l| l as f32))
                    .collect()
            })
            .unwrap_or_default();
        let stops: Vec<(&str, f32)> = colors
            .iter()
            .enumerate()
            .map(|(index, c)| {
                (
                    c.as_str(),
                    locations
                        .get(index)
                        .copied()
                        .unwrap_or(index as f32 / (colors.len() - 1).max(1) as f32),
                )
            })
            .collect();
        let number = |key: &str, fallback: f64| config[key].as_f64().unwrap_or(fallback) as f32;
        let start = (number("startX", 0.5), number("startY", 0.0));
        let end = (number("endX", 0.5), number("endY", 1.0));
        self.linear(pixmap, mask, rect, &stops, start, end, opacity);
    }

    /// A linear gradient over `rect`; start and end are fractions of it.
    #[allow(clippy::too_many_arguments)]
    fn linear(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        rect: (f32, f32, f32, f32),
        stops: &[(&str, f32)],
        start: (f32, f32),
        end: (f32, f32),
        opacity: f32,
    ) {
        let (x, y, w, h) = self.px(rect);
        let stops: Vec<GradientStop> = stops
            .iter()
            .map(|(c, at)| GradientStop::new(*at, color(c, opacity)))
            .collect();
        let shader = LinearGradient::new(
            Point::from_xy(x + start.0 * w, y + start.1 * h),
            Point::from_xy(x + end.0 * w, y + end.1 * h),
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        );
        if let (Some(shader), Some(rect)) = (shader, Rect::from_xywh(x, y, w, h)) {
            let paint = Paint {
                shader,
                anti_alias: true,
                ..Paint::default()
            };
            pixmap.fill_rect(rect, &paint, Transform::identity(), mask);
        }
    }

    fn shape(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let config = config(props);
        let Some(path) = svg::parse(config["path"].as_str().unwrap_or("")) else {
            return;
        };
        let bounds = path.bounds();
        let (view_w, view_h) = match config["viewBox"].as_array() {
            Some(view) if view.len() == 2 => (
                view[0].as_f64().unwrap_or(1.0) as f32,
                view[1].as_f64().unwrap_or(1.0) as f32,
            ),
            _ => (bounds.right(), bounds.bottom()),
        };
        let (x, y, w, h) = self.px(rect);
        let transform =
            Transform::from_row(w / view_w.max(0.001), 0.0, 0.0, h / view_h.max(0.001), x, y);
        let Some(path) = path.transform(transform) else {
            return;
        };
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        if let Some(gradient) = config["gradient"].as_object() {
            let colors: Vec<&str> = gradient["colors"]
                .as_array()
                .map(|list| list.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let locations: Vec<f32> = gradient
                .get("locations")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|l| l.as_f64().map(|l| l as f32))
                        .collect()
                })
                .unwrap_or_default();
            let number = |key: &str, fallback: f32| {
                gradient
                    .get(key)
                    .and_then(Value::as_f64)
                    .map_or(fallback, |v| v as f32)
            };
            let stops: Vec<GradientStop> = colors
                .iter()
                .enumerate()
                .map(|(index, c)| {
                    GradientStop::new(
                        locations
                            .get(index)
                            .copied()
                            .unwrap_or(index as f32 / (colors.len() - 1).max(1) as f32),
                        color(c, opacity),
                    )
                })
                .collect();
            if let Some(shader) = LinearGradient::new(
                Point::from_xy(x + number("startX", 0.0) * w, y + number("startY", 0.0) * h),
                Point::from_xy(x + number("endX", 0.0) * w, y + number("endY", 1.0) * h),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                paint.shader = shader;
            }
        } else {
            paint.set_color(color(
                config["fill"].as_str().unwrap_or("#00000000"),
                opacity,
            ));
        }
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            mask,
        );
        if let Some(stroke) = config["stroke"].as_str() {
            let width = config["strokeWidth"].as_f64().unwrap_or(0.0) as f32;
            if width > 0.0 {
                let mut line = Paint {
                    anti_alias: true,
                    ..Paint::default()
                };
                line.set_color(color(stroke, opacity));
                pixmap.stroke_path(
                    &path,
                    &line,
                    &Stroke {
                        width: width * self.scale,
                        ..Stroke::default()
                    },
                    Transform::identity(),
                    mask,
                );
            }
        }
    }

    /// The map: its background, a street grid to read as a map, and the user.
    fn map(
        &self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        props: &Props,
        rect: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let satellite = props
            .native_props
            .as_deref()
            .is_some_and(|raw| raw.contains("hybrid"));
        let background = if satellite {
            "#1d2b22"
        } else {
            props.bg.as_deref().unwrap_or("#101826")
        };
        self.fill(pixmap, mask, rect, 0.0, color(background, opacity));
        let (x, y, width, height) = rect;
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        paint.set_color(color("#ffffff0f", opacity));
        let mut streets = PathBuilder::new();
        let step = 46.0;
        let mut offset = 0.0;
        while offset < width.max(height) * 1.5 {
            streets.move_to((x + offset) * self.scale, y * self.scale);
            streets.line_to(
                (x + offset - height * 0.18) * self.scale,
                (y + height) * self.scale,
            );
            streets.move_to(x * self.scale, (y + offset * 1.1) * self.scale);
            streets.line_to(
                (x + width) * self.scale,
                (y + offset * 1.1 - width * 0.12) * self.scale,
            );
            offset += step;
        }
        if let Some(path) = streets.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.5 * self.scale,
                    ..Stroke::default()
                },
                Transform::identity(),
                mask,
            );
        }
    }

    /// Where a pin's box goes: centered on its coordinate in the map's
    /// region, which spans the map.
    fn pin_point(
        &self,
        map: &Props,
        (x, y, width, height): (f32, f32, f32, f32),
        pin: &UiNode,
    ) -> Option<(f32, f32)> {
        let (lat, lng) = (pin.props.map_latitude?, pin.props.map_longitude?);
        let (center_lat, center_lng) = (map.map_latitude?, map.map_longitude?);
        let (lat_delta, lng_delta) = (map.map_latitude_delta?, map.map_longitude_delta?);
        let frame = self.frames.get(&pin.id)?;
        let px = x + ((lng - center_lng) / lng_delta + 0.5) * width;
        let py = y + (0.5 - (lat - center_lat) / lat_delta) * height;
        Some((px - frame.width / 2.0, py - frame.height / 2.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        pixmap: &mut Pixmap,
        mask: Option<&Mask>,
        source: &str,
        rect: (f32, f32, f32, f32),
        radius: f32,
        opacity: f32,
        contain: bool,
    ) {
        let (x, y, w, h) = self.px(rect);
        let corner = radius * self.scale;
        let Some(image) = self.load(source) else {
            // Not on this machine (a web photo): a plain placeholder.
            self.fill(pixmap, mask, rect, radius, color("#2a2a2e", opacity));
            return;
        };
        let (iw, ih) = (image.width() as f32, image.height() as f32);
        let scale = if contain {
            (w / iw).min(h / ih)
        } else {
            (w / iw).max(h / ih)
        };
        let (dw, dh) = (iw * scale, ih * scale);
        let transform = Transform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            x + (w - dw) / 2.0,
            y + (h - dh) / 2.0,
        );
        let paint = Paint {
            shader: Pattern::new(
                image.as_ref(),
                SpreadMode::Pad,
                tiny_skia::FilterQuality::Bicubic,
                opacity,
                transform,
            ),
            anti_alias: true,
            ..Paint::default()
        };
        if let Some(path) = rounded((x, y, w, h), corner) {
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
    }

    /// `asset://name` from the app's bundled assets, decoded once.
    fn load(&mut self, source: &str) -> Option<&Pixmap> {
        if !self.images.contains_key(source) {
            let loaded = source.strip_prefix("asset://").and_then(|name| {
                let decoded = image::open(self.assets.join(name)).ok()?.to_rgba8();
                let (width, height) = decoded.dimensions();
                let mut pixmap = Pixmap::new(width, height)?;
                for (pixel, source) in pixmap.pixels_mut().iter_mut().zip(decoded.pixels()) {
                    let [r, g, b, a] = source.0;
                    let premultiply = |channel: u8| (channel as u16 * a as u16 / 255) as u8;
                    *pixel = tiny_skia::PremultipliedColorU8::from_rgba(
                        premultiply(r),
                        premultiply(g),
                        premultiply(b),
                        a,
                    )?;
                }
                Some(pixmap)
            });
            self.images.insert(source.to_string(), loaded);
        }
        self.images.get(source)?.as_ref()
    }
}

/// Elements the host doesn't show: a sheet told to hide.
fn hidden(props: &Props) -> bool {
    props.native.as_deref() == Some("sheet") && config(props)["hidden"] == true
}

fn config(props: &Props) -> Value {
    props
        .native_props
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null)
}

fn round(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

/// "#rgb", "#rrggbb" or "#rrggbbaa", faded by `opacity`.
pub fn color(hex: &str, opacity: f32) -> Color {
    let digits: String = hex
        .trim_start_matches('#')
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    let byte = |index: usize| {
        u8::from_str_radix(digits.get(index..index + 2).unwrap_or("ff"), 16).unwrap_or(255)
    };
    let (r, g, b, a) = match digits.len() {
        3 => {
            let nibble =
                |index: usize| u8::from_str_radix(&digits[index..index + 1], 16).unwrap_or(15) * 17;
            (nibble(0), nibble(1), nibble(2), 255)
        }
        8 => (byte(0), byte(2), byte(4), byte(6)),
        _ => (byte(0), byte(2), byte(4), 255),
    };
    let mut color = Color::from_rgba8(r, g, b, a);
    color.apply_opacity(opacity);
    color
}

/// A rounded rectangle, in pixels.
fn rounded((x, y, w, h): (f32, f32, f32, f32), radius: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    if r < 0.5 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    // Circular corners; iOS's continuous curve is a little softer.
    let k = 0.552_284_8 * r;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}
