//! The CPU preview renderer: a depth-buffered rasteriser with flat shading
//! and per-face paint (overhang faces red, the owner's policy for every
//! render). Deterministic and wasm-safe; it gives `build --png` and the
//! parity contact sheets. Camera and lights follow the CPU fallback in
//! `tests/parity/run_lab.py`, so Python and Rust views line up.

use dt_geom::TriMesh;
use libm::{cos, sin, tan};

use crate::Material;

/// One camera view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// File-name word: `painted`, `top`, `low`, `close`.
    pub name: &'static str,
    /// Degrees above the horizon.
    pub elev: f64,
    /// Degrees round from +x.
    pub azim: f64,
    /// Only the 20 mm square at the tile centre (z > 0.5 mm).
    pub close: bool,
}

/// The four views of the contact sheet (approved layout).
pub const VIEWS: [View; 4] = [
    View {
        name: "painted",
        elev: 40.0,
        azim: -120.0,
        close: false,
    },
    View {
        name: "top",
        elev: 89.0,
        azim: -90.0,
        close: false,
    },
    View {
        name: "low",
        elev: 14.0,
        azim: -150.0,
        close: false,
    },
    View {
        name: "close",
        elev: 38.0,
        azim: -125.0,
        close: true,
    },
];

/// The overhang paint.
pub const RED: [f32; 3] = [230.0 / 255.0, 30.0 / 255.0, 30.0 / 255.0];

/// An RGB image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGB bytes.
    pub rgb: Vec<u8>,
}

impl Image {
    /// Pixels painted exactly red (the render gate counts them).
    #[must_use]
    pub fn red_pixels(&self) -> usize {
        self.rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| p[0] > 150 && p[1] < 80 && p[2] < 80)
            .count()
    }

    /// PNG bytes.
    ///
    /// # Errors
    /// The encoder's message.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&self.rgb).map_err(|e| e.to_string())?;
        w.finish().map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// Paint per face: the material's colour, red where `overhang` is set.
#[must_use]
pub fn paint(materials: &[Material], overhang: &[bool]) -> Vec<[f32; 3]> {
    materials
        .iter()
        .zip(overhang)
        .map(|(m, &o)| {
            if o {
                RED
            } else {
                match m {
                    Material::Earth => [0.50, 0.38, 0.25],
                    Material::Connector => [0.62, 0.62, 0.64],
                }
            }
        })
        .collect()
}

fn dirv(elev: f64, azim: f64) -> [f64; 3] {
    let (e, a) = (elev.to_radians(), azim.to_radians());
    [cos(e) * cos(a), cos(e) * sin(a), sin(e)]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 { a.map(|x| x / l) } else { a }
}

/// **render**: `mesh` with per-face `colours` from `view`, `width` by
/// `height`, cropped to the drawn pixels plus a 20 px margin. `footprint`
/// (tile width and height, mm) places the close-up.
#[must_use]
pub fn render(
    mesh: &TriMesh,
    colours: &[[f32; 3]],
    view: &View,
    footprint: [f64; 2],
    size: (u32, u32),
) -> Image {
    let (w, h) = (size.0 as usize, size.1 as usize);
    let c0 = [footprint[0] / 2.0 - 10.0, footprint[1] / 2.0 - 10.0];
    let keep: Vec<bool> = mesh
        .triangles
        .iter()
        .map(|t| {
            if !view.close {
                return true;
            }
            let p = t.map(|i| mesh.positions[i as usize]);
            let c = [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0);
            c[0] > c0[0] && c[0] < c0[0] + 20.0 && c[1] > c0[1] && c[1] < c0[1] + 20.0 && c[2] > 0.5
        })
        .collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for (t, _) in mesh.triangles.iter().zip(&keep).filter(|(_, k)| **k) {
        for &i in t {
            let p = mesh.positions[i as usize];
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let bg = [0.85f32, 0.88, 0.92];
    let mut rgb: Vec<[f32; 3]> = vec![bg; w * h];
    let mut depth = vec![0.0f64; w * h]; // 1/z: larger is nearer
    if lo[0].is_finite() {
        let centre = [0, 1, 2].map(|k| 0.5 * (lo[k] + hi[k]));
        let diag = dot(
            [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
            [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
        )
        .sqrt();
        let fov = 40f64.to_radians();
        let th = tan(fov / 2.0);
        let d = dirv(view.elev, view.azim);
        let dist = diag / (2.0 * th) * 1.1;
        let cam = [0, 1, 2].map(|k| centre[k] + dist * d[k]);
        let fwd = unit([centre[0] - cam[0], centre[1] - cam[1], centre[2] - cam[2]]);
        let right = unit(cross(fwd, [0.0, 0.0, 1.0]));
        let up = cross(right, fwd);
        let (l1, l2) = (
            dirv(18.0, view.azim - 20.0),
            dirv(view.elev - 20.0, view.azim + 110.0),
        );
        let half = h as f64 / 2.0;
        let project = |p: [f64; 3]| {
            let v = [p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]];
            let z = dot(v, fwd);
            let x = dot(v, right) / z / th;
            let y = dot(v, up) / z / th;
            [
                x * half + w as f64 / 2.0 - 0.5,
                half - y * half - 0.5,
                1.0 / z,
            ]
        };
        for (f, t) in mesh.triangles.iter().enumerate() {
            if !keep[f] {
                continue;
            }
            let p = t.map(|i| mesh.positions[i as usize]);
            let mut n = unit(cross(
                [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]],
                [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]],
            ));
            let to_cam = [p[0][0] - cam[0], p[0][1] - cam[1], p[0][2] - cam[2]];
            if dot(n, to_cam) > 0.0 {
                n = n.map(|x| -x);
            }
            let shade = 0.3 + dot(n, l1).max(0.0) + 0.35 * dot(n, l2).max(0.0);
            #[allow(clippy::cast_possible_truncation)]
            let col = colours.get(f).copied().unwrap_or([0.7; 3]).map(|c| {
                let v = (f64::from(c) * shade * 0.95).clamp(0.0, 1.0);
                libm::pow(v, 1.0 / 1.25) as f32
            });
            let s = p.map(project);
            if s.iter().any(|q| q[2] <= 0.0) {
                continue;
            }
            fill_triangle(&s, w, h, |k, z| {
                if z > depth[k] {
                    depth[k] = z;
                    rgb[k] = col;
                }
            });
        }
    }
    crop(&rgb, &depth, w, h)
}

fn fill_triangle(s: &[[f64; 3]; 3], w: usize, h: usize, mut put: impl FnMut(usize, f64)) {
    let area =
        (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
    if area == 0.0 {
        return;
    }
    let minx = s
        .iter()
        .map(|q| q[0])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0);
    let maxx = s
        .iter()
        .map(|q| q[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(w as f64 - 1.0);
    let miny = s
        .iter()
        .map(|q| q[1])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0);
    let maxy = s
        .iter()
        .map(|q| q[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(h as f64 - 1.0);
    if minx > maxx || miny > maxy {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, x1, y0, y1) = (minx as usize, maxx as usize, miny as usize, maxy as usize);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f64, y as f64);
            let e = |a: [f64; 3], b: [f64; 3]| {
                (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
            };
            let (w0, w1, w2) = (
                e(s[1], s[2]) / area,
                e(s[2], s[0]) / area,
                e(s[0], s[1]) / area,
            );
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                put(y * w + x, w0 * s[0][2] + w1 * s[1][2] + w2 * s[2][2]);
            }
        }
    }
}

fn crop(rgb: &[[f32; 3]], depth: &[f64], w: usize, h: usize) -> Image {
    let (mut x0, mut x1, mut y0, mut y1) = (w, 0, h, 0);
    for y in 0..h {
        for x in 0..w {
            if depth[y * w + x] > 0.0 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 {
        (x0, x1, y0, y1) = (0, w - 1, 0, h - 1);
    } else {
        (x0, x1) = (x0.saturating_sub(20), (x1 + 20).min(w - 1));
        (y0, y1) = (y0.saturating_sub(20), (y1 + 20).min(h - 1));
    }
    let mut out = Vec::with_capacity((x1 - x0 + 1) * (y1 - y0 + 1) * 3);
    for y in y0..=y1 {
        for x in x0..=x1 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            out.extend(rgb[y * w + x].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8));
        }
    }
    Image {
        width: u32::try_from(x1 - x0 + 1).unwrap_or(0),
        height: u32::try_from(y1 - y0 + 1).unwrap_or(0),
        rgb: out,
    }
}
