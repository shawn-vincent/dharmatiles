//! The print list: a `.dmap` document that lists tiles to print and how to
//! lay them on plates.
//!
//! ```kdl
//! dharmatiles version="1.0.0"
//! map {
//!     piece "lib:tiles/ground/1x1-grass" count=8
//!     piece "lib:tiles/water/1x1-water-corridor+grass" count=4 seed=12
//!     piece "home:meadows/glade" count=1 system="ol"
//! }
//! plates printer="bambu/x1c-ams" system="db" mode=multicolour group=filament gap="3mm" margin="5mm"
//! ```
//!
//! A print list is a map whose pieces have no place yet (`at` is the map's,
//! read by the map view; plates ignore it).  The `plates` line names the
//! printer profile, the base system, the print mode and the spacing; every
//! word of it is optional.  Decoding keeps spans and goes on past a mistake,
//! as a tile's does, so a file's mistakes come together.
//!
//! This module reads the document and nothing else: resolving a piece's tile
//! path, building and packing are the evaluator's.

use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::decode::{arg_str, syntax, value_span};
use crate::error::{DocError, Src};
use crate::model::Spanned;
use crate::schema::did_you_mean;

/// The file extension of a print list.
pub const EXT: &str = "dmap";

/// The printer a print list that names none is laid out for: the X1 Carbon
/// with an AMS (the page's question 15, answered by its recommendation).
pub const DEFAULT_PRINTER: &str = "bambu/x1c-ams";

/// A tile to print.
#[derive(Debug, Clone)]
pub struct Piece {
    /// Its name, if written (`piece p1 "…"`): what a pin or the map names it by.
    pub id: Option<Spanned<String>>,
    /// The tile: `lib:tiles/ground/1x1-grass`, `home:meadows/glade` or a path
    /// beside the list; the `.dtile` may be left off.
    pub tile: Spanned<String>,
    /// How many copies (1 where none is written).
    pub count: Spanned<u32>,
    /// A seed to build it with, in place of the tile's own.
    pub seed: Option<Spanned<u64>>,
    /// A base system for this piece (`db` or `ol`), in place of the list's.
    pub system: Option<Spanned<String>>,
    /// `print=skip`: leave it off the plates.
    pub skip: bool,
    /// Where the map places it, as written (`"(2sq, 1sq)"`); the map's, not plates'.
    pub at: Option<Spanned<String>>,
    /// Its turn on the map, as written.
    pub rot: Option<Spanned<i64>>,
    /// Its height on the map, as written.
    pub height: Option<Spanned<String>>,
    /// The whole node.
    pub span: SourceSpan,
}

/// Single colour or multicolour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// One filament, no paint.
    Single,
    /// Each material in its own filament.
    Multicolour,
}

impl Mode {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multicolour => "multicolour",
        }
    }
}

/// How multicolour pieces are put on plates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Pieces that share their filaments go together, then large to small.
    Filament,
    /// Large to small, whatever the filaments.
    Size,
}

impl Group {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Filament => "filament",
            Self::Size => "size",
        }
    }
}

/// The `plates` line: what the list is laid out for.
#[derive(Debug, Clone)]
pub struct Plates {
    /// The base system pieces are built for (`db`, `ol`).
    pub system: Option<Spanned<String>>,
    /// The printer profile (`bambu/x1c-ams`).
    pub printer: Option<Spanned<String>>,
    /// Single colour or multicolour.
    pub mode: Option<Spanned<Mode>>,
    /// How multicolour pieces are grouped.
    pub group: Option<Spanned<Group>>,
    /// Between two pieces, mm.
    pub gap: Option<Spanned<f64>>,
    /// Between a piece and the edge of the bed, mm.
    pub margin: Option<Spanned<f64>>,
    /// May a piece be turned a quarter to fit.
    pub turn: Option<Spanned<bool>>,
    /// The whole node.
    pub span: SourceSpan,
}

impl Default for Plates {
    fn default() -> Self {
        Self {
            system: None,
            printer: None,
            mode: None,
            group: None,
            gap: None,
            margin: None,
            turn: None,
            span: SourceSpan::new(0.into(), 0),
        }
    }
}

/// A print list as written.
#[derive(Debug, Clone)]
pub struct PrintList {
    /// What it is for, in words (`about "…"`).
    pub about: Option<String>,
    /// The tiles to print, in the order written.
    pub pieces: Vec<Piece>,
    /// What it is laid out for; every word has a default.
    pub plates: Plates,
    /// The text, for diagnostics.
    pub src: Src,
    /// Where the `map` is written.
    pub span: SourceSpan,
}

struct Reader {
    src: Src,
    errors: Vec<DocError>,
}

const MAP_KEYS: [&str; 2] = ["about", "piece"];
const PLATES_KEYS: [&str; 7] = [
    "system", "printer", "mode", "group", "gap", "margin", "turn",
];
const PIECE_KEYS: [&str; 8] = [
    "count", "seed", "system", "print", "at", "rot", "height", "id",
];

impl Reader {
    fn err(
        &mut self,
        code: &'static str,
        span: SourceSpan,
        msg: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut DocError {
        self.errors
            .push(DocError::new(&self.src, code, span, msg, label));
        let n = self.errors.len() - 1;
        &mut self.errors[n]
    }

    /// A property's string value, or an error at it.
    fn string(&mut self, key: &str, e: &KdlEntry) -> Option<Spanned<String>> {
        if let KdlValue::String(s) = e.value() {
            Some(Spanned {
                value: s.clone(),
                span: value_span(e),
            })
        } else {
            self.err(
                "dt::map",
                value_span(e),
                format!("`{key}` takes a quoted word"),
                "not a string",
            )
            .help = Some(format!("write `{key}=\"…\"`"));
            None
        }
    }

    /// A property that is a word out of `words`, bare or quoted.
    fn word<T: Copy>(
        &mut self,
        key: &str,
        e: &KdlEntry,
        words: &[(&str, T)],
    ) -> Option<Spanned<T>> {
        let KdlValue::String(s) = e.value() else {
            self.err(
                "dt::map",
                value_span(e),
                format!("`{key}` takes a word"),
                "not a word",
            );
            return None;
        };
        if let Some((_, v)) = words.iter().find(|(w, _)| w == s) {
            return Some(Spanned {
                value: *v,
                span: value_span(e),
            });
        }
        let names: Vec<&str> = words.iter().map(|(w, _)| *w).collect();
        let help = did_you_mean(s, names.iter().copied()).map_or_else(
            || format!("`{key}` is {}", names.join(" or ")),
            |d| format!("did you mean `{d}`?"),
        );
        self.err(
            "dt::map",
            value_span(e),
            format!("`{s}` is not a {key}"),
            "not that",
        )
        .help = Some(help);
        None
    }

    /// A length in millimetres, `"3mm"`.
    fn mm(&mut self, key: &str, e: &KdlEntry) -> Option<Spanned<f64>> {
        let span = value_span(e);
        let KdlValue::String(s) = e.value() else {
            self.err(
                "dt::unit",
                span,
                format!("`{key}` is written with its unit"),
                "no unit",
            )
            .help = Some(format!("write `{key}=\"3mm\"`"));
            return None;
        };
        let n = s
            .trim()
            .strip_suffix("mm")
            .and_then(|n| n.trim().parse::<f64>().ok());
        match n {
            Some(v) if v.is_finite() && (0.0..=100.0).contains(&v) => {
                Some(Spanned { value: v, span })
            }
            _ => {
                self.err(
                    "dt::unit",
                    span,
                    format!("`{key}` is a length from 0 to 100 mm, like `\"3mm\"`"),
                    "not that",
                );
                None
            }
        }
    }

    /// A whole number of at least `min`.
    fn whole(&mut self, key: &str, e: &KdlEntry, min: i128, max: i128) -> Option<Spanned<i128>> {
        let span = value_span(e);
        match e.value() {
            KdlValue::Integer(i) if (min..=max).contains(i) => Some(Spanned { value: *i, span }),
            _ => {
                self.err(
                    "dt::map",
                    span,
                    format!("`{key}` is a whole number from {min} to {max}"),
                    "not that",
                );
                None
            }
        }
    }

    fn piece(&mut self, n: &KdlNode) -> Option<Piece> {
        let mut pos: Vec<&KdlEntry> = Vec::new();
        let mut p = Piece {
            id: None,
            tile: Spanned {
                value: String::new(),
                span: n.name().span(),
            },
            count: Spanned {
                value: 1,
                span: n.name().span(),
            },
            seed: None,
            system: None,
            skip: false,
            at: None,
            rot: None,
            height: None,
            span: n.span(),
        };
        for e in n.entries() {
            let Some(k) = e.name() else {
                pos.push(e);
                continue;
            };
            match k.value() {
                "count" => {
                    if let Some(v) = self.whole("count", e, 1, 10_000) {
                        p.count = Spanned {
                            value: u32::try_from(v.value).unwrap_or(1),
                            span: v.span,
                        };
                    }
                }
                "seed" => {
                    if let Some(v) = self.whole("seed", e, 0, i128::from(i64::MAX)) {
                        p.seed = Some(Spanned {
                            value: u64::try_from(v.value).unwrap_or(0),
                            span: v.span,
                        });
                    }
                }
                "system" => {
                    if let Some(s) = self.word("system", e, &[("db", "db"), ("ol", "ol")]) {
                        p.system = Some(Spanned {
                            value: s.value.to_owned(),
                            span: s.span,
                        });
                    }
                }
                "print" => {
                    if let Some(s) = self.word(
                        "print",
                        e,
                        &[("missing", false), ("always", false), ("skip", true)],
                    ) {
                        p.skip = s.value;
                    }
                }
                "at" => p.at = self.string("at", e),
                "height" => p.height = self.string("height", e),
                "rot" => {
                    if let Some(v) = self.whole("rot", e, 0, 270) {
                        if v.value % 90 == 0 {
                            p.rot = Some(Spanned {
                                value: i64::try_from(v.value).unwrap_or(0),
                                span: v.span,
                            });
                        } else {
                            self.err("dt::map", v.span, "`rot` is 0, 90, 180 or 270", "not that");
                        }
                    }
                }
                other => {
                    self.err(
                        "dt::unknown-control",
                        k.span(),
                        format!("a piece has no `{other}`"),
                        "unknown",
                    )
                    .help = Some(did_you_mean(other, PIECE_KEYS).map_or_else(
                        || format!("a piece takes: {}", PIECE_KEYS.join(", ")),
                        |d| format!("did you mean `{d}`?"),
                    ));
                }
            }
        }
        match pos.as_slice() {
            [path] => p.tile = self.tile_path(path)?,
            [id, path] => {
                p.id = arg_str(id).map(|s| Spanned {
                    value: s.to_owned(),
                    span: value_span(id),
                });
                p.tile = self.tile_path(path)?;
            }
            _ => {
                self.err(
                    "dt::map",
                    n.span(),
                    "a piece names its tile",
                    "needs `piece \"lib:tiles/ground/1x1-grass\"`",
                )
                .help =
                    Some("write the tile's path in quotes after `piece`, then `count=`".to_owned());
                return None;
            }
        }
        if let Some(c) = n.children() {
            self.err(
                "dt::map",
                c.span(),
                "a piece has no children",
                "nothing goes inside it",
            );
        }
        Some(p)
    }

    fn tile_path(&mut self, e: &KdlEntry) -> Option<Spanned<String>> {
        let span = value_span(e);
        let Some(s) = arg_str(e) else {
            self.err(
                "dt::map",
                span,
                "a tile is named by its path in quotes",
                "not a path",
            );
            return None;
        };
        if s.trim().is_empty() {
            self.err("dt::map", span, "this tile path is empty", "no path");
            return None;
        }
        Some(Spanned {
            value: s.to_owned(),
            span,
        })
    }

    fn plates(&mut self, n: &KdlNode, into: &mut Plates) {
        into.span = n.span();
        for e in n.entries() {
            let Some(k) = e.name() else {
                self.err(
                    "dt::map",
                    e.span(),
                    "the plates line takes `word=value` settings",
                    "write the setting's name first",
                )
                .help = Some(format!("the settings are: {}", PLATES_KEYS.join(", ")));
                continue;
            };
            match k.value() {
                "system" => {
                    if let Some(s) = self.word("system", e, &[("db", "db"), ("ol", "ol")]) {
                        into.system = Some(Spanned {
                            value: s.value.to_owned(),
                            span: s.span,
                        });
                    }
                }
                "printer" => into.printer = self.string("printer", e),
                "mode" => {
                    into.mode = self.word(
                        "mode",
                        e,
                        &[("single", Mode::Single), ("multicolour", Mode::Multicolour)],
                    );
                }
                "group" => {
                    into.group = self.word(
                        "group",
                        e,
                        &[("filament", Group::Filament), ("size", Group::Size)],
                    );
                }
                "gap" => into.gap = self.mm("gap", e),
                "margin" => into.margin = self.mm("margin", e),
                "turn" => {
                    into.turn = self.word("turn", e, &[("allowed", true), ("off", false)]);
                }
                other => {
                    self.err(
                        "dt::unknown-control",
                        k.span(),
                        format!("the plates line has no `{other}`"),
                        "unknown",
                    )
                    .help = Some(did_you_mean(other, PLATES_KEYS).map_or_else(
                        || format!("the settings are: {}", PLATES_KEYS.join(", ")),
                        |d| format!("did you mean `{d}`?"),
                    ));
                }
            }
        }
        if let Some(c) = n.children() {
            let first = c.nodes().first().map_or(c.span(), |x| x.span());
            self.err(
                "dt::map",
                first,
                "plates takes settings only for now",
                "nothing goes inside it",
            )
            .help = Some(
                "pins (`pin p4 plate=1 …`) come with the map view; until then every piece is placed by the packer"
                    .to_owned(),
            );
        }
    }
}

/// Read a print list.
///
/// # Errors
/// Every problem found, each pointing at its text with a hint: not KDL v2,
/// a bad header, an unknown node or setting, a piece with no tile.
pub fn decode(name: &str, text: &str) -> Result<PrintList, Vec<DocError>> {
    let src: Src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (header, header_errors) = crate::header::read(&src, &kdl);
    if header_errors.iter().any(|e| e.code == "dt::format-newer") {
        return Err(header_errors);
    }
    if header
        .as_ref()
        .is_none_or(|h| h.format < crate::header::FORMAT)
    {
        let mut errors = header_errors;
        errors.push(crate::header::format_old(&src, &kdl, header.as_ref()));
        return Err(errors);
    }
    let mut r = Reader {
        src: Arc::clone(&src),
        errors: header_errors,
    };
    let mut list = PrintList {
        about: None,
        pieces: Vec::new(),
        plates: Plates::default(),
        src,
        span: SourceSpan::new(0.into(), 0),
    };
    let (mut maps, mut plates) = (0, 0);
    for n in kdl.nodes() {
        match n.name().value() {
            crate::header::NODE => {}
            "map" => {
                maps += 1;
                if maps > 1 {
                    r.err(
                        "dt::map",
                        n.name().span(),
                        "a file holds one map",
                        "a second one",
                    )
                    .help = Some("put each list in its own file".to_owned());
                    continue;
                }
                list.span = n.span();
                read_map(&mut r, n, &mut list);
            }
            "plates" => {
                plates += 1;
                if plates > 1 {
                    r.err(
                        "dt::map",
                        n.name().span(),
                        "a list has one plates line",
                        "a second one",
                    )
                    .help = Some("a pin means something only for one printer and one base; keep one `plates` line".to_owned());
                    continue;
                }
                r.plates(n, &mut list.plates);
            }
            other => {
                r.err(
                    "dt::unknown-node",
                    n.name().span(),
                    format!("unknown top-level node `{other}`"),
                    "not map or plates",
                )
                .help = Some(did_you_mean(other, ["map", "plates"]).map_or_else(
                    || {
                        "a print list is `map { piece … }` and an optional `plates …` line"
                            .to_owned()
                    },
                    |d| format!("did you mean `{d}`?"),
                ));
            }
        }
    }
    if maps == 0 && r.errors.is_empty() {
        let span = kdl
            .nodes()
            .first()
            .map_or_else(|| SourceSpan::new(0.into(), 0), KdlNode::span);
        r.err("dt::map", span, "this document has no `map`", "no map here")
            .help = Some(
            "a print list holds `map { piece \"lib:tiles/ground/1x1-grass\" count=4 }`".to_owned(),
        );
    }
    if r.errors.is_empty() {
        Ok(list)
    } else {
        Err(r.errors)
    }
}

fn read_map(r: &mut Reader, n: &KdlNode, list: &mut PrintList) {
    if let Some(e) = n.entries().first() {
        r.err(
            "dt::map",
            e.span(),
            "`map` takes no arguments",
            "put the pieces inside",
        )
        .help = Some("map { piece \"lib:tiles/ground/1x1-grass\" count=4 }".to_owned());
    }
    for c in n.iter_children() {
        match c.name().value() {
            "about" => {
                if let [e] = c.entries()
                    && let Some(s) = arg_str(e)
                {
                    list.about = Some(s.to_owned());
                } else {
                    r.err(
                        "dt::map",
                        c.span(),
                        "`about` takes one quoted sentence",
                        "here",
                    );
                }
            }
            "piece" => {
                if let Some(p) = r.piece(c) {
                    list.pieces.push(p);
                }
            }
            other => {
                let help = match other {
                    "grid" | "let" => Some("grids and named heights belong to the map view, which is not built yet; a print list holds `piece` lines".to_owned()),
                    _ => did_you_mean(other, MAP_KEYS).map(|d| format!("did you mean `{d}`?")),
                };
                r.err(
                    "dt::unknown-node",
                    c.name().span(),
                    format!("a map holds `piece` lines, not `{other}`"),
                    "unknown",
                )
                .help = help.or_else(|| Some(format!("a map takes: {}", MAP_KEYS.join(", "))));
            }
        }
    }
}
