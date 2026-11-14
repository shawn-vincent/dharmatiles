//! The machine half of a printer profile: bed, height, excluded zones,
//! nozzle, filament slots, template and the maker's name.  Each is optional
//! in the file; lengths are written in millimetres with their unit
//! (`"180x180mm"`, `"0.4mm"`), and a mistake is reported at its text.

use kdl::{KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use super::{Decoder, arg_str, value_span};
use crate::model::{Machine, Spanned};

/// The words a machine adds to a printer.
pub(crate) const KEYS: [&str; 11] = [
    "brand",
    "model",
    "bed",
    "height",
    "excluded",
    "nozzle",
    "slots",
    "template",
    "verify",
    "from",
    "from-slicer",
];

/// A length in millimetres: `180mm`, `0.4 mm`; `Err` says what is wrong.
fn mm(text: &str) -> Result<f64> {
    let t = text.trim();
    let n = t.strip_suffix("mm").ok_or_else(|| {
        format!("`{t}` has no unit: a machine's lengths are written in millimetres, like `180mm`")
    })?;
    n.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("`{t}` is not a number of millimetres"))
}

/// `180x180mm` or `180mm x 180mm`: width by depth.
fn size(text: &str) -> Result<[f64; 2]> {
    let t = text.trim();
    let (a, b) = t
        .split_once(['x', '×'])
        .ok_or_else(|| format!("`{t}` is not a size: write width x depth, like `256x256mm`"))?;
    // the unit is written once at the end, or after each
    let a = if a.trim_end().ends_with("mm") {
        a.to_owned()
    } else {
        format!("{}mm", a.trim())
    };
    Ok([mm(&a)?, mm(b)?])
}

type Result<T> = std::result::Result<T, String>;

/// `(0mm, 0mm) (18mm, 6mm)`: two corners.
fn zone(text: &str) -> Result<[f64; 4]> {
    let cleaned = text.replace(['(', ')', ','], " ");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() != 4 {
        return Err(format!(
            "`{}` is not a zone: write two corners, like `(0mm, 0mm) (18mm, 6mm)`",
            text.trim()
        ));
    }
    let v: Vec<f64> = parts.iter().map(|p| mm(p)).collect::<Result<_>>()?;
    Ok([
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ])
}

impl Decoder<'_> {
    /// A string value read by `read`, its mistake reported at the text.
    fn machine_read<T>(
        &mut self,
        c: &KdlNode,
        read: impl Fn(&str) -> Result<T>,
    ) -> Option<Spanned<T>> {
        let key = c.name().value();
        let [e] = c.entries() else {
            self.err(
                dt_lang::codes::PRINTER,
                c.span(),
                format!("`{key}` takes one quoted value"),
                "here",
            )
            .help = Some(format!("write `{key} \"…\"`"));
            return None;
        };
        let Some(text) = arg_str(e) else {
            self.err(
                dt_lang::codes::PRINTER,
                value_span(e),
                format!("`{key}` takes a quoted value"),
                "not a string",
            )
            .help = Some(format!("write `{key} \"…\"`"));
            return None;
        };
        let span = value_span(e);
        match read(text) {
            Ok(value) => Some(Spanned { value, span }),
            Err(m) => {
                self.err(dt_lang::codes::PRINTER, span, m, "not that");
                None
            }
        }
    }

    /// One machine word of a printer, into `m`.
    pub(super) fn machine_key(&mut self, c: &KdlNode, m: &mut Machine) {
        match c.name().value() {
            "brand" => m.brand = self.machine_read(c, |s| Ok(s.to_owned())),
            "model" => m.model = self.machine_read(c, |s| Ok(s.to_owned())),
            "template" => m.template = self.machine_read(c, |s| Ok(s.to_owned())),
            "verify" => m.verify = self.machine_read(c, |s| Ok(s.to_owned())),
            "from-slicer" => m.from_slicer = self.machine_read(c, |s| Ok(s.to_owned())),
            "from" => m.from = self.machine_read(c, |s| Ok(s.to_owned())),
            "bed" => {
                if let Some(b) = self.machine_read(c, size)
                    && self.positive(b.span, "the bed", b.value)
                {
                    m.bed = Some(b);
                }
            }
            "height" => {
                if let Some(h) = self.machine_read(c, mm)
                    && self.positive(h.span, "the height", [h.value])
                {
                    m.height = Some(h);
                }
            }
            "nozzle" => {
                if let Some(h) = self.machine_read(c, mm)
                    && self.positive(h.span, "the nozzle", [h.value])
                {
                    m.nozzle = Some(h);
                }
            }
            "slots" => m.slots = self.slots(c),
            "excluded" => self.excluded(c, m),
            _ => {}
        }
    }

    fn positive(&mut self, span: SourceSpan, what: &str, v: impl IntoIterator<Item = f64>) -> bool {
        if v.into_iter().all(|x| x > 0.0 && x < 2000.0) {
            return true;
        }
        self.err(
            dt_lang::codes::PRINTER,
            span,
            format!("{what} is a positive size under 2000 mm"),
            "not that",
        );
        false
    }

    fn slots(&mut self, c: &KdlNode) -> Option<Spanned<u32>> {
        let [e] = c.entries() else {
            self.err(
                dt_lang::codes::PRINTER,
                c.span(),
                "`slots` takes one whole number",
                "here",
            );
            return None;
        };
        let span = value_span(e);
        match e.value() {
            KdlValue::Integer(i) if (1..=32).contains(i) => Some(Spanned {
                value: u32::try_from(*i).ok()?,
                span,
            }),
            _ => {
                self.err(
                    dt_lang::codes::PRINTER,
                    span,
                    "`slots` is the number of filaments loaded at once, from 1 to 32",
                    "not that",
                );
                None
            }
        }
    }

    fn excluded(&mut self, c: &KdlNode, m: &mut Machine) {
        if !c.entries().is_empty() {
            let span = c.entries().first().map_or(c.span(), KdlEntry::span);
            self.err(
                dt_lang::codes::PRINTER,
                span,
                "`excluded` is a block of `zone` lines",
                "put the zones inside",
            )
            .help = Some("excluded { zone \"(0mm, 0mm) (18mm, 6mm)\" }".to_owned());
        }
        for z in c.iter_children() {
            if z.name().value() != "zone" {
                self.err(
                    dt_lang::codes::UNKNOWN_CONTROL,
                    z.name().span(),
                    format!("`excluded` holds `zone` lines, not `{}`", z.name().value()),
                    "unknown",
                );
                continue;
            }
            if let Some(v) = self.machine_read(z, |s| {
                let b = zone(s)?;
                if b[2] - b[0] <= 0.0 || b[3] - b[1] <= 0.0 {
                    return Err("a zone has an area: its two corners are not the same line".into());
                }
                Ok(b)
            }) {
                m.excluded.push(v);
            }
        }
    }
}
