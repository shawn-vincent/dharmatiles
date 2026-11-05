//! Just enough raster work for the contact sheets and the docs' pictures:
//! 8-bit RGB images read from the renderer's PNGs, laid out, scaled down,
//! marked with dots and dashes, and written as PNG or JPEG.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel coordinates and channel values: small, non-negative, clamped"
)]

use std::path::Path;

/// The renderer's background, the site's `--plate`.
pub const PLATE: [u8; 3] = [217, 224, 235];

pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Rgb {
    pub fn new(w: usize, h: usize, fill: [u8; 3]) -> Self {
        Self {
            w,
            h,
            px: fill.repeat(w * h),
        }
    }

    /// Decode one of the renderer's 8-bit RGB PNGs.
    pub fn read_png(path: &Path) -> Result<Self, String> {
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let file = std::fs::File::open(path).map_err(|e| err(&e))?;
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .map_err(|e| err(&e))?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| err(&"image too large"))?;
        let mut px = vec![0; size];
        let info = reader.next_frame(&mut px).map_err(|e| err(&e))?;
        if info.bit_depth != png::BitDepth::Eight || info.color_type != png::ColorType::Rgb {
            return Err(err(&"not an 8-bit RGB PNG (dharmatiles renders are)"));
        }
        px.truncate(info.buffer_size());
        let dim = |v: u32| usize::try_from(v).map_err(|e| err(&e));
        Ok(Self {
            w: dim(info.width)?,
            h: dim(info.height)?,
            px,
        })
    }

    fn size_u32(&self, path: &Path) -> Result<(u32, u32), String> {
        let too_big = |_| format!("{}: image too large", path.display());
        Ok((
            u32::try_from(self.w).map_err(too_big)?,
            u32::try_from(self.h).map_err(too_big)?,
        ))
    }

    fn create(path: &Path) -> Result<std::fs::File, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn write_png(&self, path: &Path) -> Result<(), String> {
        let (w, h) = self.size_u32(path)?;
        let file = Self::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&self.px))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn write_jpeg(&self, path: &Path, quality: u8) -> Result<(), String> {
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let (w, h) = self.size_u32(path)?;
        let w = u16::try_from(w).map_err(|e| err(&e))?;
        let h = u16::try_from(h).map_err(|e| err(&e))?;
        let file = Self::create(path)?;
        let mut enc = jpeg_encoder::Encoder::new(std::io::BufWriter::new(file), quality);
        enc.set_progressive(true);
        enc.encode(&self.px, w, h, jpeg_encoder::ColorType::Rgb)
            .map_err(|e| err(&e))
    }

    fn get(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 3;
        [self.px[i], self.px[i + 1], self.px[i + 2]]
    }

    /// Scale down to `width` (kept if already narrower) by area averaging.
    pub fn fit_width(self, width: usize) -> Self {
        if self.w <= width {
            return self;
        }
        let s = self.w as f64 / width as f64;
        let height = ((self.h as f64 / s).round() as usize).max(1);
        // separable box filter with fractional edge coverage
        let pass = |len_in: usize, len_out: usize| -> Vec<Vec<(usize, f64)>> {
            let s = len_in as f64 / len_out as f64;
            (0..len_out)
                .map(|o| {
                    let (a, b) = (o as f64 * s, (o + 1) as f64 * s);
                    let mut taps = Vec::new();
                    let mut i = a.floor() as usize;
                    while (i as f64) < b && i < len_in {
                        let cover = (b.min(i as f64 + 1.0) - a.max(i as f64)) / s;
                        if cover > 0.0 {
                            taps.push((i, cover));
                        }
                        i += 1;
                    }
                    taps
                })
                .collect()
        };
        let (xs, ys) = (pass(self.w, width), pass(self.h, height));
        let mut out = Self::new(width, height, [0; 3]);
        let mut row = vec![[0.0f64; 3]; width];
        for (oy, ytaps) in ys.iter().enumerate() {
            row.iter_mut().for_each(|c| *c = [0.0; 3]);
            for &(y, wy) in ytaps {
                for (ox, xtaps) in xs.iter().enumerate() {
                    for &(x, wx) in xtaps {
                        let p = self.get(x, y);
                        for k in 0..3 {
                            row[ox][k] += f64::from(p[k]) * wx * wy;
                        }
                    }
                }
            }
            for (ox, c) in row.iter().enumerate() {
                let i = (oy * width + ox) * 3;
                for (dst, v) in out.px[i..i + 3].iter_mut().zip(c) {
                    *dst = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }

    /// The tile's box in a top view: everything that is not the plate.
    pub fn footprint(&self) -> Option<(f64, f64, f64, f64)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in (0..self.h).step_by(2) {
            for x in (0..self.w).step_by(2) {
                let p = self.get(x, y);
                let d: i32 = (0..3)
                    .map(|k| (i32::from(p[k]) - i32::from(PLATE[k])).abs())
                    .sum();
                if d > 24 {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        (x0 <= x1).then_some((x0 as f64, y0 as f64, x1 as f64, y1 as f64))
    }

    /// A filled disc.
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, c: [u8; 3]) {
        let (lo_y, hi_y) = ((cy - r).floor().max(0.0), (cy + r).ceil());
        let (lo_x, hi_x) = ((cx - r).floor().max(0.0), (cx + r).ceil());
        for y in lo_y as usize..=(hi_y as usize).min(self.h.saturating_sub(1)) {
            for x in lo_x as usize..=(hi_x as usize).min(self.w.saturating_sub(1)) {
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    let i = (y * self.w + x) * 3;
                    self.px[i..i + 3].copy_from_slice(&c);
                }
            }
        }
    }

    /// A round-capped stroke from `a` to `b`, `width` px wide.
    pub fn stroke(&mut self, a: (f64, f64), b: (f64, f64), width: f64, c: [u8; 3]) {
        let n = ((b.0 - a.0).hypot(b.1 - a.1) * 2.0).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            self.disc(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, width / 2.0, c);
        }
    }
}

/// Lay images out in rows and columns, each cell the size of the largest.
pub fn grid(rows: &[Vec<Rgb>], gap: usize, ground: [u8; 3]) -> Rgb {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let cw = rows.iter().flatten().map(|i| i.w).max().unwrap_or(0);
    let ch = rows.iter().flatten().map(|i| i.h).max().unwrap_or(0);
    let mut out = Rgb::new(
        gap + cols * (cw + gap),
        gap + rows.len() * (ch + gap),
        ground,
    );
    for (r, row) in rows.iter().enumerate() {
        for (c, im) in row.iter().enumerate() {
            let (x0, y0) = (gap + c * (cw + gap), gap + r * (ch + gap));
            for y in 0..im.h {
                let dst = ((y0 + y) * out.w + x0) * 3;
                out.px[dst..dst + im.w * 3]
                    .copy_from_slice(&im.px[y * im.w * 3..(y + 1) * im.w * 3]);
            }
        }
    }
    out
}
