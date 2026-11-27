//! Stand-ins for SF Symbols, which can't be shipped: simple line drawings of
//! the ones apps use, so a screen reads right. Anything else is a ring.

use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

/// The icon module's names for SF Symbols (as the iOS host's table).
pub fn symbol(name: &str) -> &str {
    match name {
        "utensils" => "fork.knife",
        "arrow-up" => "arrow.up",
        "chevron-right" => "chevron.right",
        "chevron-up" => "chevron.up",
        "share" => "square.and.arrow.up",
        "coffee" => "cup.and.saucer.fill",
        "hamburger" => "takeoutbag.and.cup.and.straw.fill",
        "fish" => "fish.fill",
        "pizza-slice" => "triangle.fill",
        "car" => "car.fill",
        "google" => "g.circle.fill",
        "magnifying-glass" => "magnifyingglass",
        "circle-xmark" => "xmark.circle.fill",
        "star" => "star.fill",
        other => other,
    }
}

/// Draw `name` in a `size` square centered at (cx, cy), in pixels.
pub fn draw(
    pixmap: &mut Pixmap,
    mask: Option<&tiny_skia::Mask>,
    name: &str,
    cx: f32,
    cy: f32,
    size: f32,
    paint: &Paint,
) {
    let symbol = symbol(name);
    let base = symbol.trim_end_matches(".fill");
    let filled = symbol.ends_with(".fill");
    let s = size / 2.0;
    // Points in a -1..1 square.
    let p = |x: f32, y: f32| (cx + x * s, cy + y * s);
    let mut lines = PathBuilder::new();
    let mut fill = PathBuilder::new();
    let line = |b: &mut PathBuilder, points: &[(f32, f32)]| {
        if let Some((first, rest)) = points.split_first() {
            let (x, y) = p(first.0, first.1);
            b.move_to(x, y);
            for &(px, py) in rest {
                let (x, y) = p(px, py);
                b.line_to(x, y);
            }
        }
    };
    let circle = |b: &mut PathBuilder, x: f32, y: f32, r: f32| {
        let (x, y) = p(x, y);
        b.push_circle(x, y, r * s);
    };
    match base {
        "line.3.horizontal" => {
            for y in [-0.55, 0.0, 0.55] {
                line(&mut lines, &[(-0.8, y), (0.8, y)]);
            }
        }
        "chevron.up.chevron.down" => {
            line(&mut lines, &[(-0.45, -0.25), (0.0, -0.7), (0.45, -0.25)]);
            line(&mut lines, &[(-0.45, 0.25), (0.0, 0.7), (0.45, 0.25)]);
        }
        "ruler" => {
            line(&mut fill, &[(-0.85, -0.35), (0.85, -0.35), (0.85, 0.35), (-0.85, 0.35), (-0.85, -0.35)]);
        }
        "line.3.horizontal.decrease" => {
            for (y, w) in [(-0.55, 0.8), (0.0, 0.5), (0.55, 0.2)] {
                line(&mut lines, &[(-w, y), (w, y)]);
            }
        }
        "xmark" => {
            line(&mut lines, &[(-0.6, -0.6), (0.6, 0.6)]);
            line(&mut lines, &[(0.6, -0.6), (-0.6, 0.6)]);
        }
        "xmark.circle" => {
            circle(&mut fill, 0.0, 0.0, 0.9);
        }
        "chevron.up" => line(&mut lines, &[(-0.7, 0.35), (0.0, -0.35), (0.7, 0.35)]),
        "chevron.down" => line(&mut lines, &[(-0.7, -0.35), (0.0, 0.35), (0.7, -0.35)]),
        "chevron.right" => line(&mut lines, &[(-0.35, -0.7), (0.35, 0.0), (-0.35, 0.7)]),
        "chevron.left" => line(&mut lines, &[(0.35, -0.7), (-0.35, 0.0), (0.35, 0.7)]),
        "arrow.up" => {
            line(&mut lines, &[(0.0, 0.75), (0.0, -0.75)]);
            line(&mut lines, &[(-0.6, -0.15), (0.0, -0.75), (0.6, -0.15)]);
        }
        "plus" => {
            line(&mut lines, &[(0.0, -0.75), (0.0, 0.75)]);
            line(&mut lines, &[(-0.75, 0.0), (0.75, 0.0)]);
        }
        "magnifyingglass" => {
            circle(&mut lines, -0.15, -0.15, 0.55);
            line(&mut lines, &[(0.25, 0.25), (0.75, 0.75)]);
        }
        "location" | "location.slash" | "location.north" => {
            let arrow = [
                (0.75, -0.75),
                (-0.75, -0.05),
                (-0.05, 0.05),
                (0.05, 0.75),
                (0.75, -0.75),
            ];
            line(if filled { &mut fill } else { &mut lines }, &arrow);
            if base == "location.slash" {
                line(&mut lines, &[(-0.85, -0.85), (0.85, 0.85)]);
            }
        }
        "bookmark" => {
            let shape = [
                (-0.55, -0.8),
                (0.55, -0.8),
                (0.55, 0.85),
                (0.0, 0.4),
                (-0.55, 0.85),
                (-0.55, -0.8),
            ];
            line(if filled { &mut fill } else { &mut lines }, &shape);
        }
        "square.and.arrow.up" => {
            line(
                &mut lines,
                &[
                    (-0.3, -0.15),
                    (-0.65, -0.15),
                    (-0.65, 0.85),
                    (0.65, 0.85),
                    (0.65, -0.15),
                    (0.3, -0.15),
                ],
            );
            line(&mut lines, &[(0.0, 0.3), (0.0, -0.85)]);
            line(&mut lines, &[(-0.35, -0.5), (0.0, -0.85), (0.35, -0.5)]);
        }
        "arrow.triangle.turn.up.right.diamond" => {
            line(
                if filled { &mut fill } else { &mut lines },
                &[
                    (0.0, -0.9),
                    (0.9, 0.0),
                    (0.0, 0.9),
                    (-0.9, 0.0),
                    (0.0, -0.9),
                ],
            );
            if !filled {
                line(&mut lines, &[(-0.3, 0.35), (-0.3, -0.05), (0.3, -0.05)]);
                line(&mut lines, &[(0.1, -0.25), (0.3, -0.05), (0.1, 0.15)]);
            }
        }
        "globe.americas" | "globe" | "map" => {
            circle(&mut lines, 0.0, 0.0, 0.85);
            line(&mut lines, &[(-0.85, 0.0), (0.85, 0.0)]);
            line(&mut lines, &[(0.0, -0.85), (0.0, 0.85)]);
        }
        "fork.knife" => {
            line(&mut lines, &[(-0.4, -0.8), (-0.4, 0.8)]);
            line(
                &mut lines,
                &[(-0.65, -0.8), (-0.65, -0.25), (-0.15, -0.25), (-0.15, -0.8)],
            );
            line(
                &mut lines,
                &[(0.45, 0.8), (0.45, -0.8), (0.2, -0.2), (0.45, -0.1)],
            );
        }
        "star" => {
            let mut star = Vec::new();
            for index in 0..11 {
                let radius = if index % 2 == 0 { 0.9 } else { 0.4 };
                let angle =
                    std::f32::consts::PI * (index as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
                star.push((radius * angle.cos(), radius * angle.sin()));
            }
            line(&mut fill, &star);
        }
        "car" => {
            line(
                &mut fill,
                &[
                    (-0.85, 0.5),
                    (-0.85, -0.05),
                    (-0.55, -0.6),
                    (0.55, -0.6),
                    (0.85, -0.05),
                    (0.85, 0.5),
                    (-0.85, 0.5),
                ],
            );
        }
        "waveform" => {
            for (x, h) in [
                (-0.6, 0.3),
                (-0.3, 0.7),
                (0.0, 0.45),
                (0.3, 0.8),
                (0.6, 0.35),
            ] {
                line(&mut lines, &[(x, -h), (x, h)]);
            }
        }
        "gearshape" => {
            circle(&mut lines, 0.0, 0.0, 0.7);
            circle(&mut lines, 0.0, 0.0, 0.25);
        }
        "mappin.and.ellipse" | "mappin" => {
            circle(&mut lines, 0.0, -0.3, 0.4);
            line(&mut lines, &[(0.0, 0.1), (0.0, 0.8)]);
        }
        "info.circle" => {
            circle(&mut lines, 0.0, 0.0, 0.85);
            circle(&mut fill, 0.0, -0.4, 0.1);
            line(&mut lines, &[(0.0, -0.1), (0.0, 0.45)]);
        }
        "g.circle" => circle(&mut fill, 0.0, 0.0, 0.9),
        _ => circle(if filled { &mut fill } else { &mut lines }, 0.0, 0.0, 0.7),
    }
    if let Some(path) = fill.finish() {
        fill_path(pixmap, mask, &path, paint);
    }
    if let Some(path) = lines.finish() {
        let stroke = Stroke {
            width: (size * 0.11).max(1.0),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, paint, &stroke, Transform::identity(), mask);
    }
}

fn fill_path(pixmap: &mut Pixmap, mask: Option<&tiny_skia::Mask>, path: &Path, paint: &Paint) {
    pixmap.fill_path(path, paint, FillRule::Winding, Transform::identity(), mask);
}
