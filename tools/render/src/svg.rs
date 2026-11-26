//! SVG path data to a tiny-skia path, for the `shape` module: M L H V C S Q
//! T Z, absolute and relative, as the iOS host reads them.

use tiny_skia::{Path, PathBuilder};

enum Token {
    Command(char),
    Number(f32),
}

fn tokens(data: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, out: &mut Vec<Token>| {
        if let Ok(value) = number.parse::<f32>() {
            out.push(Token::Number(value));
        }
        number.clear();
    };
    for character in data.chars() {
        if character.is_ascii_alphabetic() && character != 'e' && character != 'E' {
            flush(&mut number, &mut out);
            out.push(Token::Command(character));
        } else if character == '-' || character == '+' {
            if number.ends_with(['e', 'E']) {
                number.push(character);
            } else {
                flush(&mut number, &mut out);
                number.push(character);
            }
        } else if character == '.' && number.contains('.') && !number.ends_with(['e', 'E']) {
            flush(&mut number, &mut out);
            number.push(character);
        } else if character.is_ascii_digit()
            || character == '.'
            || character == 'e'
            || character == 'E'
        {
            number.push(character);
        } else {
            flush(&mut number, &mut out);
        }
    }
    flush(&mut number, &mut out);
    out
}

/// The path, in its own units (its viewBox).
pub fn parse(data: &str) -> Option<Path> {
    let tokens = tokens(data);
    let mut builder = PathBuilder::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    // The last control point, for S and T.
    let mut last_control: Option<(f32, f32)> = None;
    let mut index = 0;
    let mut command = 'M';
    let number = |index: &mut usize| -> Option<f32> {
        match tokens.get(*index) {
            Some(Token::Number(value)) => {
                *index += 1;
                Some(*value)
            }
            _ => None,
        }
    };
    while index < tokens.len() {
        if let Token::Command(next) = tokens[index] {
            command = next;
            index += 1;
            if command == 'Z' || command == 'z' {
                builder.close();
                x = start_x;
                y = start_y;
                last_control = None;
                continue;
            }
        }
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        match command.to_ascii_uppercase() {
            'M' => {
                let (nx, ny) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.move_to(nx, ny);
                (x, y, start_x, start_y) = (nx, ny, nx, ny);
                last_control = None;
                // Further pairs are lines.
                command = if relative { 'l' } else { 'L' };
            }
            'L' => {
                (x, y) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.line_to(x, y);
                last_control = None;
            }
            'H' => {
                x = number(&mut index)? + ox;
                builder.line_to(x, y);
                last_control = None;
            }
            'V' => {
                y = number(&mut index)? + oy;
                builder.line_to(x, y);
                last_control = None;
            }
            'C' => {
                let c1 = (number(&mut index)? + ox, number(&mut index)? + oy);
                let c2 = (number(&mut index)? + ox, number(&mut index)? + oy);
                (x, y) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, x, y);
                last_control = Some(c2);
            }
            'S' => {
                let c1 = last_control.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let c2 = (number(&mut index)? + ox, number(&mut index)? + oy);
                (x, y) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, x, y);
                last_control = Some(c2);
            }
            'Q' => {
                let c = (number(&mut index)? + ox, number(&mut index)? + oy);
                (x, y) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.quad_to(c.0, c.1, x, y);
                last_control = Some(c);
            }
            'T' => {
                let c = last_control.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                (x, y) = (number(&mut index)? + ox, number(&mut index)? + oy);
                builder.quad_to(c.0, c.1, x, y);
                last_control = Some(c);
            }
            // Anything else (arcs) ends the path, as on iOS.
            _ => break,
        }
    }
    builder.finish()
}
