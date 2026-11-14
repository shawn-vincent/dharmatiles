//! The palette: material to colour, the separate step after the build.
//! A 3MF is painted by its flat colours; the preview and the PNG renders by
//! its relief ramps.  The geometry never changes.

use crate::Material;

/// One ramp: tones along relief, each with its place from 0 to 1.
pub type Ramp = Vec<(f32, [f32; 3])>;

/// Material to colour: the separate step after the build (a 3MF is
/// painted by it; the geometry never changes). A palette document has two
/// kinds of line, `//` for comments:
///
/// - `rock "#8a8a90"`: the material's one flat colour (3MF base colour);
/// - `ramp rock "#3a3d46" "#5f636e" "#8c909a@0.7" "#e4e5e6"`: the acrylic
///   paint of the preview and the PNG renders, a dark basecoat to the
///   drybrushed highlights, blended smoothly by local relief. A tone's
///   place is spaced evenly unless it ends `@0.7`.
///
/// What a document does not name keeps its default
/// (`library/palettes/default.dtile`).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    flat: Vec<(Material, [f64; 3])>,
    ramps: std::collections::BTreeMap<String, Ramp>,
}

/// The ramps' names: the materials, plus the ground's `pebble` and
/// `brick` (masonry of a brick wall).
pub const RAMPS: [&str; 12] = [
    "earth",
    "pebble",
    "water",
    "grass",
    "leaf",
    "wood",
    "rock",
    "masonry",
    "brick",
    "flower",
    "connector",
    "side",
];

const DEFAULT: &str = include_str!("../../../library/palettes/default.dtile");

impl Default for Palette {
    fn default() -> Self {
        let mut p = Self {
            flat: Vec::new(),
            ramps: std::collections::BTreeMap::new(),
        };
        // The bundled palette is compiled in and checked by a test
        // (`the_bundled_palette_reads`), so this cannot fail in a program that
        // passed its tests; if it ever did, stopping is right: no colours
        // can be guessed.
        #[allow(clippy::expect_used)]
        p.read(DEFAULT)
            .expect("the bundled palette (library/palettes/default.dtile) reads");
        p
    }
}

fn hex(h: &str) -> Option<[f64; 3]> {
    let h = h.trim_matches('"').trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
    Some([v(0), v(2), v(4)])
}

impl Palette {
    /// The colour of a single-colour print: one filament for the whole tile,
    /// the neutral grey of the connector (which has no filament of its own in
    /// multicolour, where it prints in earth's).  The one place single colour
    /// gets its colour.
    #[must_use]
    pub fn single(&self) -> [f64; 3] {
        self.colour(Material::Connector)
    }

    /// The colour of `m`.
    #[must_use]
    pub fn colour(&self, m: Material) -> [f64; 3] {
        self.flat
            .iter()
            .find(|(k, _)| *k == m)
            .map_or_else(|| m.colour(), |(_, c)| *c)
    }

    /// The paint of ramp `name` at `t` (0 to 1) of relief.
    #[must_use]
    pub fn sample(&self, name: &str, t: f32) -> [f32; 3] {
        let Some(r) = self.ramps.get(name) else {
            return [0.7; 3];
        };
        let t = t.clamp(0.0, 1.0);
        let i = (0..r.len().saturating_sub(1))
            .rev()
            .find(|&i| t >= r[i].0)
            .unwrap_or(0);
        if r.len() < 2 {
            return r.first().map_or([0.7; 3], |x| x.1);
        }
        let f = ((t - r[i].0) / (r[i + 1].0 - r[i].0).max(1e-6)).clamp(0.0, 1.0);
        [0, 1, 2].map(|c| r[i].1[c] * (1.0 - f) + r[i + 1].1[c] * f)
    }

    /// Read a palette document.
    ///
    /// # Errors
    /// The line and what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut p = Self::default();
        p.read(text)?;
        Ok(p)
    }

    fn read(&mut self, text: &str) -> Result<(), String> {
        for (n, line) in text.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut w = line.split_whitespace();
            let first = w.next().unwrap_or("");
            // the header line (`dharmatiles version=…`): dt-doc checks it,
            // and callers run that check before reading a palette
            if first == "dharmatiles" {
                continue;
            }
            if first == "ramp" {
                let name = w.next().unwrap_or("");
                if !RAMPS.contains(&name) {
                    return Err(format!(
                        "line {}: `{name}` is not a ramp ({})",
                        n + 1,
                        RAMPS.join(", ")
                    ));
                }
                let toks: Vec<&str> = w.collect();
                if toks.len() < 2 {
                    return Err(format!("line {}: a ramp needs at least two tones", n + 1));
                }
                #[allow(clippy::cast_precision_loss)]
                let last = (toks.len() - 1) as f32;
                let mut ramp: Ramp = Vec::new();
                for (k, tok) in toks.iter().enumerate() {
                    let (h, at) = tok
                        .split_once('@')
                        .map_or((*tok, None), |(h, a)| (h, Some(a)));
                    let c = hex(h).ok_or_else(|| {
                        format!("line {}: `{tok}` is not a colour like \"#8a8a90\"", n + 1)
                    })?;
                    #[allow(clippy::cast_precision_loss)]
                    let pos = match at {
                        Some(a) => a
                            .trim_matches('"')
                            .parse::<f32>()
                            .map_err(|_| format!("line {}: `@{a}` is not a number", n + 1))?,
                        None => k as f32 / last,
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    ramp.push((pos, c.map(|v| v as f32)));
                }
                if ramp.windows(2).any(|w| w[1].0 <= w[0].0) {
                    return Err(format!("line {}: the tones' places must increase", n + 1));
                }
                self.ramps.insert(name.to_owned(), ramp);
                continue;
            }
            let m = Material::from_name(first).ok_or_else(|| {
                format!("line {}: `{first}` is not a material (earth, connector, rock, water, grass, masonry, wood, leaf) or `ramp`", n + 1)
            })?;
            let c = hex(w.next().unwrap_or("")).ok_or_else(|| {
                format!("line {}: `{first}` needs a colour like \"#8a8a90\"", n + 1)
            })?;
            self.flat.retain(|(k, _)| *k != m);
            self.flat.push((m, c));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_palette_reads() {
        // `Palette::default` expects this to hold
        let mut p = Palette {
            flat: Vec::new(),
            ramps: std::collections::BTreeMap::new(),
        };
        p.read(DEFAULT).unwrap();
        assert!(!p.flat.is_empty());
    }

    #[test]
    fn a_palette_names_materials_and_refuses_what_is_not_one() {
        let p = Palette::parse("// mine\nrock \"#ff0000\"\nwater #0000ff").unwrap();
        assert_eq!(p.colour(Material::Rock), [1.0, 0.0, 0.0]);
        assert_eq!(
            p.colour(Material::Earth),
            Palette::default().colour(Material::Earth)
        );
        assert!(Palette::parse("lava #ff0000").is_err());
        assert!(Palette::parse("rock red").is_err());
    }
}
