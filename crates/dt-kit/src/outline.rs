//! Outlines: closed 2D contours, read from an SVG path. Port of the path
//! parser in `src/dharmatiles/core/logo.py` (`_parse_svg_d`): M/L/H/V/C/Z,
//! absolute and relative, implicit repeats, cubic Béziers halved until flat.

use crate::KitError;

/// Closed contours in mm (x, y), filled even-odd.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    /// The contours, each a closed polygon (last point not repeated).
    pub contours: Vec<Vec<[f64; 2]>>,
}

impl Outline {
    /// Parse an SVG path `d` attribute, flattening curves to within
    /// `tol` path units. Coordinates stay in path units (y down).
    ///
    /// # Errors
    /// [`KitError::Shape`] on a token that is not a command or number.
    pub fn svg_path(d: &str, tol: f64) -> Result<Self, KitError> {
        let toks = tokens(d)?;
        let mut contours = Vec::new();
        let mut pts: Vec<[f64; 2]> = Vec::new();
        let (mut px, mut py, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
        let mut cmd: Option<char> = None;
        let mut i = 0;
        while i < toks.len() {
            if let Tok::Cmd(c) = toks[i] {
                i += 1;
                if c == 'Z' || c == 'z' {
                    if !pts.is_empty() {
                        contours.push(std::mem::take(&mut pts));
                    }
                    (px, py) = (sx, sy);
                    cmd = None;
                    continue;
                }
                cmd = Some(c);
            }
            let before = i;
            let take = |n: usize| -> Result<Vec<f64>, KitError> {
                let mut v = Vec::with_capacity(n);
                for k in 0..n {
                    match toks.get(before + k) {
                        Some(Tok::Num(x)) => v.push(*x),
                        _ => {
                            return Err(KitError::Shape(format!(
                                "SVG path: number expected at token {}",
                                before + k
                            )));
                        }
                    }
                }
                Ok(v)
            };
            let Some(c) = cmd else {
                return Err(KitError::Shape(format!(
                    "SVG path: command expected at token {i}"
                )));
            };
            let rel = c.is_ascii_lowercase();
            let (ox, oy) = if rel { (px, py) } else { (0.0, 0.0) };
            match c.to_ascii_uppercase() {
                'M' => {
                    let v = take(2)?;
                    i += 2;
                    if !pts.is_empty() {
                        contours.push(std::mem::take(&mut pts));
                    }
                    (px, py) = (ox + v[0], oy + v[1]);
                    (sx, sy) = (px, py);
                    pts.push([px, py]);
                    cmd = Some(if rel { 'l' } else { 'L' });
                }
                'L' => {
                    let v = take(2)?;
                    i += 2;
                    (px, py) = (ox + v[0], oy + v[1]);
                    pts.push([px, py]);
                }
                'H' => {
                    let v = take(1)?;
                    i += 1;
                    px = ox + v[0];
                    pts.push([px, py]);
                }
                'V' => {
                    let v = take(1)?;
                    i += 1;
                    py = oy + v[0];
                    pts.push([px, py]);
                }
                'C' => {
                    let v = take(6)?;
                    i += 6;
                    let p1 = [ox + v[0], oy + v[1]];
                    let p2 = [ox + v[2], oy + v[3]];
                    let p3 = [ox + v[4], oy + v[5]];
                    subdivide([px, py], p1, p2, p3, tol, &mut pts);
                    (px, py) = (p3[0], p3[1]);
                }
                other => {
                    return Err(KitError::Shape(format!(
                        "SVG path: unsupported command {other}"
                    )));
                }
            }
        }
        if !pts.is_empty() {
            contours.push(pts);
        }
        Ok(Self { contours })
    }

    /// The outline scaled from a `viewbox`-unit square (y down) to a
    /// `size` mm square centred on `(cx, cy)` (y up), as `logo.py` places it.
    #[must_use]
    pub fn placed(&self, viewbox: f64, cx: f64, cy: f64, size: f64) -> Self {
        let s = size / viewbox;
        let (x0, y0) = (cx - size / 2.0, cy + size / 2.0);
        Self {
            contours: self
                .contours
                .iter()
                .map(|c| c.iter().map(|p| [x0 + p[0] * s, y0 - p[1] * s]).collect())
                .collect(),
        }
    }
}

enum Tok {
    Cmd(char),
    Num(f64),
}

fn tokens(d: &str) -> Result<Vec<Tok>, KitError> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if "MLCZHVmlczhv".contains(c) {
            out.push(Tok::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            // [-+]?(\d*\.\d+|\d+\.?)([eE][-+]?\d+)?
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            let mut dot = false;
            while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !dot)) {
                dot |= b[i] == b'.';
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s = &d[start..i];
            let v = s
                .parse::<f64>()
                .map_err(|_| KitError::Shape(format!("SVG path: bad number {s:?}")))?;
            out.push(Tok::Num(v));
        } else {
            i += 1; // separators
        }
    }
    Ok(out)
}

/// Halve a cubic Bézier until its control hull is within `tol` of the
/// chord; push the points after `p0`.
fn subdivide(
    p0: [f64; 2],
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    tol: f64,
    out: &mut Vec<[f64; 2]>,
) {
    let ux = 3.0 * p1[0] - 2.0 * p0[0] - p3[0];
    let uy = 3.0 * p1[1] - 2.0 * p0[1] - p3[1];
    let vx = 3.0 * p2[0] - 2.0 * p3[0] - p0[0];
    let vy = 3.0 * p2[1] - 2.0 * p3[1] - p0[1];
    if (ux * ux + uy * uy).max(vx * vx + vy * vy) <= 16.0 * tol * tol {
        out.push(p3);
        return;
    }
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (m01, m12, m23) = (mid(p0, p1), mid(p1, p2), mid(p2, p3));
    let (m012, m123) = (mid(m01, m12), mid(m12, m23));
    let m = mid(m012, m123);
    subdivide(p0, m01, m012, m, tol, out);
    subdivide(m, m123, m23, p3, tol, out);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    #[test]
    fn parses_relative_and_implicit() {
        let o = Outline::svg_path("m 10,10 5,0 0,5 z M 0 0 H 4 V 4 L 0 4 Z", 1.0).unwrap();
        assert_eq!(o.contours.len(), 2);
        assert_eq!(
            o.contours[0],
            vec![[10.0, 10.0], [15.0, 10.0], [15.0, 15.0]]
        );
        assert_eq!(o.contours[1].len(), 4);
    }

    #[test]
    fn flattens_curves() {
        let o = Outline::svg_path("M0,0 C0,10 10,10 10,0 Z", 0.1).unwrap();
        assert!(o.contours[0].len() > 8);
        assert_eq!(*o.contours[0].last().unwrap(), [10.0, 0.0]);
    }
}
