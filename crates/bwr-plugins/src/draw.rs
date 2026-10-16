//! Declarative drawing commands returned by plugins.
//!
//! Coordinates are fractions of the drawing area: (0,0) top-left,
//! (1,1) bottom-right. Sizes given as `size`/`r` are fractions of the
//! area's *height* so circles and symbols stay round.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const BLACK: Rgba = Rgba([0, 0, 0, 255]);

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub fn parse(s: &str) -> Option<Rgba> {
        let h = s.trim().strip_prefix('#')?;
        let hex = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
        match h.len() {
            3 => {
                let d = |i| hex(i, 1).map(|v| v * 17);
                Some(Rgba([d(0)?, d(1)?, d(2)?, 255]))
            }
            6 => Some(Rgba([hex(0, 2)?, hex(2, 2)?, hex(4, 2)?, 255])),
            8 => Some(Rgba([hex(0, 2)?, hex(2, 2)?, hex(4, 2)?, hex(6, 2)?])),
            _ => None,
        }
    }

    /// Relative luminance (WCAG) ignoring alpha, 0..1.
    pub fn luminance_opaque(self) -> f32 {
        Rgba([self.0[0], self.0[1], self.0[2], 255]).luminance()
    }

    /// Relative luminance (WCAG), scaled by alpha, 0..1.
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b, a] = self.0;
        (0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)) * (a as f32 / 255.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    /// Fill the whole area.
    Fill(Rgba),
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgba },
    Circle { x: f32, y: f32, r: f32, color: Rgba, stroke: Option<f32> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgba },
    Text { x: f32, y: f32, text: String, size: f32, color: Rgba, centered: bool },
    /// Stroked polylines from `draw.svg`: points are offsets from the centre
    /// (`cx`, `cy`, area fractions) measured in units of the area *height*.
    Paths { cx: f32, cy: f32, paths: Vec<Vec<(f32, f32)>>, width: f32, color: Rgba },
    /// Concentric rings (from `draw.rings`): a disc of `radius` (height
    /// units) centred on (`cx`, `cy`), cut into rings `width` thick,
    /// coloured by cycling through `colors` from the outside in. Fully
    /// transparent colours leave gaps, so stacked discs make moiré.
    Rings { cx: f32, cy: f32, radius: f32, width: f32, colors: Vec<Rgba> },
}

/// Max rings in one `draw.rings` call.
pub const MAX_RINGS: usize = 400;

impl DrawCmd {
    /// Fraction of the area this command covers when it is a solid fill.
    pub fn solid_area(&self) -> f32 {
        match self {
            DrawCmd::Fill(_) => 1.0,
            DrawCmd::Rect { w, h, .. } => (w.clamp(0.0, 1.0) * h.clamp(0.0, 1.0)).max(0.0),
            DrawCmd::Circle { r, stroke: None, .. } => std::f32::consts::PI * r * r,
            _ => 0.0,
        }
    }

    pub fn color_mut(&mut self) -> Option<&mut Rgba> {
        Some(match self {
            DrawCmd::Rings { .. } => return None,
            DrawCmd::Fill(c) => c,
            DrawCmd::Rect { color, .. }
            | DrawCmd::Circle { color, .. }
            | DrawCmd::Line { color, .. }
            | DrawCmd::Text { color, .. }
            | DrawCmd::Paths { color, .. } => color,
        })
    }

    /// Colour this command gives to point (x, y) in area fractions, as
    /// (luminance, coverage alpha), or None if it doesn't cover the point.
    /// Rings report their *average* colour so their motion doesn't read as
    /// flicker. `aspect` = width / height. Thin shapes are ignored.
    pub fn sample(&self, x: f32, y: f32, aspect: f32) -> Option<(f32, f32)> {
        let lum_a = |c: &Rgba| (c.luminance_opaque(), c.0[3] as f32 / 255.0);
        let inside_disc = |cx: f32, cy: f32, r: f32| {
            let dx = (x - cx) * aspect;
            let dy = y - cy;
            dx * dx + dy * dy <= r * r
        };
        match self {
            DrawCmd::Fill(c) => Some(lum_a(c)),
            DrawCmd::Rect { x: rx, y: ry, w, h, color } => {
                (x >= *rx && x <= rx + w && y >= *ry && y <= ry + h).then(|| lum_a(color))
            }
            DrawCmd::Circle { x: cx, y: cy, r, color, stroke: None } => inside_disc(*cx, *cy, *r).then(|| lum_a(color)),
            DrawCmd::Rings { cx, cy, radius, colors, .. } if !colors.is_empty() => {
                if !inside_disc(*cx, *cy, *radius) {
                    return None;
                }
                let n = colors.len() as f32;
                let a: f32 = colors.iter().map(|c| c.0[3] as f32 / 255.0).sum::<f32>() / n;
                if a <= 0.0 {
                    return None;
                }
                let l: f32 = colors.iter().map(|c| c.luminance_opaque() * c.0[3] as f32 / 255.0).sum::<f32>() / n / a;
                Some((l, a))
            }
            _ => None,
        }
    }
}

/// Parse a subset of SVG path data (`M L H V C S Q T Z`, absolute and
/// relative) into flattened polylines in the path's own coordinates.
/// Arcs (`A`) are not supported: use cubic curves or `draw.circle`.
pub fn parse_svg_path(d: &str) -> Result<Vec<Vec<(f32, f32)>>, String> {
    #[derive(Debug, Clone, Copy)]
    enum Tok {
        Cmd(char),
        Num(f32),
    }
    let mut toks = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            if !"MmLlHhVvCcSsQqTtZz".contains(c) {
                return Err(format!("unsupported path command '{c}' (supported: M L H V C S Q T Z)"));
            }
            toks.push(Tok::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            let start = i;
            let mut seen_dot = false;
            if c == '-' || c == '+' {
                i += 1;
            }
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && i + 1 < b.len() {
                    i += 1;
                    if b[i] == b'-' || b[i] == b'+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            let s = &d[start..i];
            toks.push(Tok::Num(s.parse().map_err(|_| format!("bad number \"{s}\" in path"))?));
        } else if c.is_whitespace() || c == ',' {
            i += 1;
        } else {
            return Err(format!("unexpected character '{c}' in path"));
        }
    }

    if !matches!(toks.first(), Some(Tok::Cmd('M' | 'm')) | None) {
        return Err("path must start with M".into());
    }
    let mut paths: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur = Vec::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut last_ctrl: Option<(char, f32, f32)> = None;
    let mut cmd = ' ';
    let mut k = 0;
    let nums = |k: &mut usize, n: usize| -> Result<Vec<f32>, String> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            match toks.get(*k) {
                Some(Tok::Num(v)) => out.push(*v),
                _ => return Err("path command is missing numbers".into()),
            }
            *k += 1;
        }
        Ok(out)
    };
    let toks_len = toks.len();
    let peek_is_num = |k: usize, toks: &Vec<Tok>| matches!(toks.get(k), Some(Tok::Num(_)));
    let toks_copy = toks.clone();
    const SEG: usize = 12;
    while k < toks_len {
        if let Tok::Cmd(c) = toks_copy[k] {
            cmd = c;
            k += 1;
        } else if cmd == ' ' {
            return Err("path must start with M".into());
        } else if cmd == 'M' {
            cmd = 'L';
        } else if cmd == 'm' {
            cmd = 'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let v = nums(&mut k, 2)?;
                if cur.len() > 1 {
                    paths.push(std::mem::take(&mut cur));
                }
                cur.clear();
                x = ox + v[0];
                y = oy + v[1];
                sx = x;
                sy = y;
                cur.push((x, y));
                last_ctrl = None;
            }
            'L' => {
                let v = nums(&mut k, 2)?;
                x = ox + v[0];
                y = oy + v[1];
                cur.push((x, y));
                last_ctrl = None;
            }
            'H' => {
                let v = nums(&mut k, 1)?;
                x = ox + v[0];
                cur.push((x, y));
                last_ctrl = None;
            }
            'V' => {
                let v = nums(&mut k, 1)?;
                y = oy + v[0];
                cur.push((x, y));
                last_ctrl = None;
            }
            'C' | 'S' => {
                let (c1, rest) = if cmd.to_ascii_uppercase() == 'C' {
                    let v = nums(&mut k, 6)?;
                    ((ox + v[0], oy + v[1]), [ox + v[2], oy + v[3], ox + v[4], oy + v[5]])
                } else {
                    let v = nums(&mut k, 4)?;
                    let c1 = match last_ctrl {
                        Some(('C', cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    };
                    (c1, [ox + v[0], oy + v[1], ox + v[2], oy + v[3]])
                };
                let (p0, p3) = ((x, y), (rest[2], rest[3]));
                let c2 = (rest[0], rest[1]);
                for s in 1..=SEG {
                    let t = s as f32 / SEG as f32;
                    let u = 1.0 - t;
                    let px = u * u * u * p0.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p3.0;
                    let py = u * u * u * p0.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p3.1;
                    cur.push((px, py));
                }
                x = p3.0;
                y = p3.1;
                last_ctrl = Some(('C', c2.0, c2.1));
            }
            'Q' | 'T' => {
                let (c, p) = if cmd.to_ascii_uppercase() == 'Q' {
                    let v = nums(&mut k, 4)?;
                    ((ox + v[0], oy + v[1]), (ox + v[2], oy + v[3]))
                } else {
                    let v = nums(&mut k, 2)?;
                    let c = match last_ctrl {
                        Some(('Q', cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    };
                    (c, (ox + v[0], oy + v[1]))
                };
                let p0 = (x, y);
                for s in 1..=SEG {
                    let t = s as f32 / SEG as f32;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p.0,
                        u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p.1,
                    ));
                }
                x = p.0;
                y = p.1;
                last_ctrl = Some(('Q', c.0, c.1));
            }
            'Z' => {
                cur.push((sx, sy));
                x = sx;
                y = sy;
                last_ctrl = None;
                if cur.len() > 1 {
                    paths.push(std::mem::take(&mut cur));
                }
                cur.push((x, y));
                if !peek_is_num(k, &toks_copy) {
                    continue;
                }
                return Err("numbers after Z".into());
            }
            _ => unreachable!(),
        }
    }
    if cur.len() > 1 {
        paths.push(cur);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba([255, 255, 255, 255])));
        assert_eq!(Rgba::parse("#10203040"), Some(Rgba([16, 32, 48, 64])));
        assert_eq!(Rgba::parse("red"), None);
        assert!(Rgba::parse("#ffffff").unwrap().luminance() > 0.99);
        assert!(Rgba::BLACK.luminance() < 0.01);
    }

    #[test]
    fn svg_paths() {
        let p = parse_svg_path("M10 10 L90 10 L90 90 Z m-40-40 h10 v10").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], vec![(10.0, 10.0), (90.0, 10.0), (90.0, 90.0), (10.0, 10.0)]);
        assert_eq!(p[1], vec![(-30.0, -30.0), (-20.0, -30.0), (-20.0, -20.0)]);
        let c = parse_svg_path("M0,0 C0,10 10,10 10,0").unwrap();
        assert_eq!(c[0].len(), 13);
        assert_eq!(*c[0].last().unwrap(), (10.0, 0.0));
        assert!(parse_svg_path("M0 0 A5 5 0 0 1 10 10").is_err());
        assert!(parse_svg_path("L0 0").is_err());
    }
}
