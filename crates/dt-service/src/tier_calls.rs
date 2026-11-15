//! The tiers a map wants (`tiersWant`): what the page asks for the pieces of a map or a picture shelf, one tier of one
//! tile at a time.  The types, the key a want is announced and fetched by, the checking of a want into something to
//! build ([`prepare`]) and the bytes a finished piece makes ([`tier_bytes`]) are written once here, for both
//! transports.  What differs is where the builds run: the program queues each on its tier lane while the page's request
//! returns, and says on the stream when each is done; the browser editor's service builds them in the call, one after
//! another, and says the same on the same stream.  The book (what is wanted, in flight and kept) is the owner's
//! ([`crate::tiers`]).

use std::collections::BTreeMap;

use dt_eval::{Built, Options};
use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::plates_calls::PlatesHost;
use crate::tile_host::TileRef;

/// The triangles a full tier of a piece keeps for a map's view (a map of full grass tiles is millions the page does not
/// need to draw).
pub const MAP_FULL_TRIANGLES: usize = 600_000;

/// One tier of one piece's tile that the page wants.
#[derive(Deserialize, Clone, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TierWant {
    pub tile: String,
    /// `super-low`, `low` or `full`.
    pub tier: dt_eval::Tier,
    pub seed: Option<u64>,
    pub system: Option<dt_lang::System>,
    pub printer: Option<String>,
    /// A source text to build in place of the file's own (a step of the document's history): the file is not
    /// edited, and the picture is kept by the text, so a text built before is not built again.
    pub text: Option<String>,
    /// The file `text` is of: the tile's own document, or a file its last build read.
    pub file: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TiersWantReq {
    /// Whose wants these are (a map, the picture shelf): a post replaces the last of the same scope.
    pub scope: String,
    /// The document the work is for (a map's tile id): its pieces loading is one job of it, said as the document's
    /// progress and in the messages like a build.  None for the picture shelf.
    #[serde(default)]
    pub doc: Option<String>,
    pub wants: Vec<TierWant>,
}

/// The key of each want, in the order asked: what its result is announced and fetched by.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct TiersWantRes {
    pub keys: Vec<String>,
}

/// `tiersWant`: replace the scope's wants.
pub const TIERS_WANT: Verb<TiersWantReq, TiersWantRes> = Verb::new("tiersWant");

/// `tierResult`'s request: the kept result for a key, if it is of this revision of the document.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TierResultReq {
    pub key: String,
    pub doc_rev: u64,
}

/// The key of a want: what its result is announced and fetched by.
#[must_use]
pub fn key_of(w: &TierWant) -> String {
    let dash = || "-".to_owned();
    format!(
        "{}|{}|{}|{}|{}|{}",
        w.tile,
        w.tier,
        w.seed.map_or_else(dash, |s| s.to_string()),
        w.system.map_or_else(dash, |s| s.to_string()),
        w.printer.clone().unwrap_or_else(dash),
        w.text.as_deref().map_or_else(dash, |t| format!(
            "{}:{:016x}",
            w.file.as_deref().unwrap_or("-"),
            crate::tile_calls::fnv(t)
        )),
    )
}

/// A want, checked and ready to build.
pub struct Ready {
    pub key: String,
    pub tile: String,
    pub path: String,
    pub opts: Options,
    /// A text to build in place of a file's: the file and the text.
    pub overlay: Option<BTreeMap<String, String>>,
}

/// Check a want: the tile it is of (`facts` gives a listed tile's path and the files its last build read), the text it
/// would build in place of a file's, and the options it builds with.
///
/// # Errors
/// [`Refusal`]: no such tile, a file that is not the tile's, a tier that is not one, a printer that does not read.
pub fn prepare(
    host: &impl PlatesHost,
    facts: impl Fn(&str) -> Option<TileRef>,
    w: &TierWant,
) -> Result<Ready, Refusal> {
    let Some(t) = facts(&w.tile) else {
        return Err(Refusal::refused(Status::NotFound, "no such tile"));
    };
    // the same rule as a draft's: the tile's own document, or a file its last build read
    let overlay = match &w.text {
        None => None,
        Some(text) => Some(BTreeMap::from([(
            t.resolve_file(w.file.as_deref())?,
            text.clone(),
        )])),
    };
    let mut opts = host.options();
    if w.tier == dt_eval::Tier::Draft {
        return Err(Refusal::refused(
            Status::BadRequest,
            "a tier is low, super-low or full",
        ));
    }
    opts.tier = w.tier;
    if let Some(s) = w.system {
        opts.system = s;
    }
    opts.seed = w.seed.or(opts.seed);
    if let Some(p) = w.printer.as_deref()
        && p != opts.printer.name
        && let Err(e) = opts.use_printer_in(p, &*host.profiles())
    {
        return Err(Refusal::refused(
            Status::BadRequest,
            e.first().map_or_else(String::new, |d| d.message.clone()),
        ));
    }
    Ok(Ready {
        key: key_of(w),
        tile: w.tile.clone(),
        path: t.path,
        opts,
        overlay,
    })
}

/// What a want builds from: the file's own text, or the text laid over it.
pub struct Sources<'a> {
    overlay: Option<&'a BTreeMap<String, String>>,
    base: &'a dyn dt_eval::Source,
}

impl<'a> Sources<'a> {
    /// The sources of `r`, over `base` (the files, the library).
    #[must_use]
    pub fn of(r: &'a Ready, base: &'a dyn dt_eval::Source) -> Self {
        Self {
            overlay: r.overlay.as_ref(),
            base,
        }
    }
}

impl dt_eval::Source for Sources<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.overlay
            .and_then(|o| o.get(path).cloned())
            .map_or_else(|| self.base.read(path), Ok)
    }
}

/// The bytes the page reads for a piece built for `r` at revision `rev` of its document: a full build thinned for the
/// view, then the map piece's payload.
///
/// # Errors
/// The build's per-triangle lists do not match its mesh (a bug), said in words.
pub fn tier_bytes(
    b: Built,
    r: &Ready,
    rev: u64,
    palette: &dt_kit::Palette,
) -> Result<Vec<u8>, String> {
    let b = if r.opts.tier == dt_eval::Tier::Full {
        dt_eval::tiers::thin_built(b, MAP_FULL_TRIANGLES)
    } else {
        b
    };
    crate::payload::for_map(&b, r.opts.tier.name(), rev, &r.opts, palette)
        .map_err(|e| format!("cannot prepare the {} view: {e}", r.opts.tier.name()))
}
