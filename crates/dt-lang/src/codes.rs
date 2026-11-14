//! The diagnostic codes: every `dt::…` word the program prints, said once.
//!
//! A code is written nowhere else.  Every error, note and message takes its code from a constant here
//! (`codes::TYPE`), and a test fails when a `"dt::…"` literal turns up outside this file.  That a code exists is
//! a fact about the program, so it lives here; where it is explained is a fact about prose, so it lives on the
//! docs page, as a `data-explains="dt::type"` marker on the section that explains it.  `cargo xtask docs` joins
//! the two: it derives the `#dt-type` anchor on that section and the error page `e/type.html`, and `docs --check`
//! fails when a code has no section, a marker names no code, or a page or anchor is missing.
//!
//! To add a code: one entry in `registry!` below, then a marker on the section that explains it.

/// Whose fault an error is, and so how the page speaks of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A mistake in a document: the reader writes something else.
    Mistake,
    /// Something the checker saw in a document that is fine: nothing is refused.
    Note,
    /// The program, a file or the machine failed, not the document.
    Failure,
}

/// A broken document and the same document mended, each whole (header line included).  The test suite checks
/// the broken one gives this code and the mended one gives none, so an example cannot drift from the program.
#[derive(Clone, Copy, Debug)]
pub struct Example {
    /// A document that gives the code.
    pub broken: &'static str,
    /// The same document with the mistake mended.
    pub fixed: &'static str,
}

/// One diagnostic code.
#[derive(Clone, Copy, Debug)]
pub struct Code {
    /// The code as printed, `dt::type`.
    pub id: &'static str,
    /// Whose fault it is.
    pub kind: Kind,
    /// What happened, in one line.
    pub summary: &'static str,
    /// What to do about it, in general (the message itself says it for the case).
    pub help: &'static str,
    /// A broken document and its fix, when one document shows it.
    pub example: Option<Example>,
}

/// Write each code once: its constant (`TYPE` is `"dt::type"`) and its entry in [`ALL`] come from the same line.
macro_rules! registry {
    ($(
        $name:ident $slug:literal $kind:ident {
            summary: $summary:literal,
            help: $help:literal,
            example: $example:expr $(,)?
        }
    )*) => {
        $(
            #[doc = $summary]
            pub const $name: &str = concat!("dt::", $slug);
        )*
        /// Every code the program prints.
        pub const ALL: &[Code] = &[
            $(Code { id: $name, kind: Kind::$kind, summary: $summary, help: $help, example: $example },)*
        ];
    };
}

const NONE: Option<Example> = None;

/// A short broken/fixed pair: the whole of each document is written out in the entry.
macro_rules! ex {
    ($broken:literal, $fixed:literal) => {
        Some(Example {
            broken: $broken,
            fixed: $fixed,
        })
    };
}

registry! {
    // the header line and the format
    FORMAT_0 "format-0" Mistake {
        summary: "A document with no header line is format 0, which this program does not read",
        help: "start the document with `dharmatiles version=1`; release 1.0.0 carries a file to format 1 with `dharmatiles migrate`",
        example: ex!(
            "tile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    FORMAT_NEWER "format-newer" Mistake {
        summary: "The document is of a newer format than this program reads",
        help: "update dharmatiles, or write the document in the format the program reads",
        example: ex!(
            "dharmatiles version=2\ntile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    HEADER "header" Mistake {
        summary: "The header line takes `version=` and nothing else",
        help: "write just `dharmatiles version=1`, with no children and no other settings",
        example: ex!(
            "dharmatiles version=1 {\n    note\n}\ntile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    HEADER_VERSION "header-version" Mistake {
        summary: "The header line has no version, or one that cannot be read",
        help: "write `version=1` (a number), or a quoted version such as `version=\"1.0.0\"`",
        example: ex!(
            "dharmatiles\ntile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    HEADER_TWICE "header-twice" Mistake {
        summary: "A document has one header line",
        help: "remove the second header line",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ndharmatiles version=1\ntile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    HEADER_FIRST "header-first" Mistake {
        summary: "The header line must be the first node of the file",
        help: "move `dharmatiles version=…` above everything else",
        example: ex!(
            "tile size=\"1x1\" {\n    stack { dirt }\n}\ndharmatiles version=1\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    SYNTAX "syntax" Mistake {
        summary: "Documents are KDL v2: a bare `true`, `false` or `null` is KDL v1",
        help: "write the keyword with a hash: `#true`, `#false`, `#null`",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt clods=null }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt clods=none }\n}\n"
        ),
    }

    // the tile and what it holds
    TILE "tile" Mistake {
        summary: "A file holds one tile",
        help: "put each tile in its own file",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\ntile size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    TILE_NAME "tile-name" Mistake {
        summary: "A tile takes no name: its file name is its name",
        help: "remove the name from the `tile` line and name the file instead",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile pond size=\"1x1\" {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    ABOUT "about" Mistake {
        summary: "`about` takes one string",
        help: "write one quoted sentence after `about`",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    about 1 2\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    about \"A square of dirt.\"\n    stack { dirt }\n}\n"
        ),
    }
    SEED "seed" Mistake {
        summary: "A tile's seed is a whole number from 0",
        help: "write `seed=` with a whole number, 0 or more",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=-3 {\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=3 {\n    stack { dirt }\n}\n"
        ),
    }
    STACK "stack" Mistake {
        summary: "A tile or a definition has one stack",
        help: "merge the stacks into one, listing the elements in the order they are built",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n    stack { grass }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; grass }\n}\n"
        ),
    }
    PARAM "param" Mistake {
        summary: "A tile has no parameters: only a definition does",
        help: "move the `param` line into a `define`, or write a `let` in the tile",
        example: NONE,
    }
    DEFINE "define" Mistake {
        summary: "`define` takes only a name",
        help: "write `define name { … }` with nothing else on the line",
        example: NONE,
    }
    MISSING_NAME "missing-name" Mistake {
        summary: "This node needs a name",
        help: "write the name after the keyword",
        example: NONE,
    }
    LINE "line" Mistake {
        summary: "That name is a tile edge or corner, so a line cannot take it",
        help: "pick another name for the line; the edges and corners belong to the tile",
        example: NONE,
    }

    // controls
    UNKNOWN_CONTROL "unknown-control" Mistake {
        summary: "The element has no control by that name",
        help: "check the spelling against the element's page; the message names the nearest control",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt tpo=\"3dmm\" }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"3dmm\" }\n}\n"
        ),
    }
    UNKNOWN_ELEMENT "unknown-element" Mistake {
        summary: "No element or definition is called that",
        help: "check the spelling; the message names the nearest element, and `dharmatiles elements` lists them all",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirtt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt }\n}\n"
        ),
    }
    UNKNOWN_NODE "unknown-node" Mistake {
        summary: "That line is not one this kind of document holds",
        help: "check the spelling and where the line goes; the message says what is allowed here",
        example: NONE,
    }
    MISSING_CONTROL "missing-control" Mistake {
        summary: "A body needs a `top=` it has no default for",
        help: "give the body a `top=`, as the element's page shows",
        example: NONE,
    }
    POSITIONAL "positional" Mistake {
        summary: "The element takes name=value controls, not a bare value",
        help: "write each control as `name=value`",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt \"3dmm\" }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"3dmm\" }\n}\n"
        ),
    }
    CONTROL_NODE "control-node" Mistake {
        summary: "A control written as its own line has no block",
        help: "write the control as `name=value` on the element's line, or give the line a value",
        example: NONE,
    }
    GROUP "group" Mistake {
        summary: "That control is a group of controls, not a value",
        help: "write the group's own controls on its line: `leaf length=\"4dmm\" width=\"3dmm\"`",
        example: NONE,
    }
    EXPERT "expert" Mistake {
        summary: "That control is an expert control, not for a tile",
        help: "leave it out; the element's page says what to write instead",
        example: NONE,
    }
    CHOICE "choice" Mistake {
        summary: "The control takes one of a few words, and this is not one",
        help: "write one of the words the message lists",
        example: NONE,
    }
    DUPLICATE "duplicate" Mistake {
        summary: "A control is set twice on one line",
        help: "keep one of them",
        example: NONE,
    }
    ID "id" Mistake {
        summary: "Another entry of that element already has that `id=`",
        help: "give every entry of an element its own id",
        example: NONE,
    }
    SCATTER "scatter" Mistake {
        summary: "That element cannot be scattered",
        help: "scatter things (a rock, a tree, a flower), not terrain",
        example: NONE,
    }
    WEIGHT "weight" Mistake {
        summary: "`weight=` is a scatter child's share of the mix",
        help: "write `weight=` only on a thing inside a `scatter`",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; rock weight=2 }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; scatter count=3 { rock weight=2 } }\n}\n"
        ),
    }
    PERCENT "percent" Mistake {
        summary: "The control is a share written as a percent, such as `20%`",
        help: "write the share with a percent sign, from 0% to 100%",
        example: NONE,
    }
    EXPR "expr" Mistake {
        summary: "A formula in quotes could not be read",
        help: "check the formula against the language guide's expressions; the message points at the spot",
        example: NONE,
    }

    // names, units and kinds
    NAME "name" Mistake {
        summary: "A plain name is declared twice or is a word of the language",
        help: "pick another name; `override` replaces a `$` name the context already has",
        example: NONE,
    }
    UNIT "unit" Mistake {
        summary: "A length or angle has no unit, or a unit that is not one",
        help: "write the unit: `5dmm` or `5mm` for a length, `20deg` for an angle",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; rock size=5 }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; rock size=\"5dmm\" }\n}\n"
        ),
    }
    TYPE "type" Mistake {
        summary: "Kinds do not agree: a length mixed with a plain number, or a place of the wrong kind",
        help: "make both sides the same kind; the message says what to write",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"$dirtEdgeHeight + 20%\" }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"$dirtEdgeHeight + 2dmm\" }\n}\n"
        ),
    }
    VALUE "value" Mistake {
        summary: "A value cannot be read or is not allowed here",
        help: "write the value the control takes, as its page shows",
        example: NONE,
    }
    UNKNOWN_NAME "unknown-name" Mistake {
        summary: "Nothing declares or sets that name",
        help: "check the spelling; the message names the nearest name",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"$grundHeight\" }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"$groundHeight\" }\n}\n"
        ),
    }
    RANGE "range" Mistake {
        summary: "A number is outside the limits of its weighting",
        help: "bring the number inside the limits the message gives",
        example: NONE,
    }
    WEIGHTING "weighting" Mistake {
        summary: "A weighting is written wrongly, or the control takes none",
        help: "a weighting wraps a plain range, `low(1dmm .. 6dmm)`; the message says which part is wrong",
        example: NONE,
    }
    CLOUD "cloud" Mistake {
        summary: "A top with odds (a cloud) is used in arithmetic it does not have",
        help: "use a weighted range only where the guide's clouds section allows it",
        example: NONE,
    }

    // places, regions and lines
    POINT "point" Mistake {
        summary: "A point is outside the tile or the region it is written in",
        help: "move the point inside the box the message gives",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    point p \"(2sq, 0.5sq)\"\n    stack { dirt }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    point p \"(0.5sq, 0.5sq)\"\n    stack { dirt }\n}\n"
        ),
    }
    AT "at" Mistake {
        summary: "No point is called that",
        help: "check the spelling, or declare the point first",
        example: NONE,
    }
    REGION "region" Mistake {
        summary: "A region cannot be made: it names a place that is not there, or leads back to itself",
        help: "a region is made of lines, fill points, `within` and `except` of other regions, none of which may lead back to it",
        example: NONE,
    }
    UNKNOWN_REGION "unknown-region" Mistake {
        summary: "No region has that name",
        help: "check the spelling, or declare the region",
        example: NONE,
    }
    EMPTY_REGION "empty-region" Mistake {
        summary: "A region covers nothing",
        help: "check the lines that bound it and its fill point",
        example: NONE,
    }
    PLACE "place" Mistake {
        summary: "That place is not one the element can use",
        help: "write a place the element takes: a point, a line or a region, as its page says",
        example: NONE,
    }

    // heights and the stack
    BASE "base" Mistake {
        summary: "`base` takes no settings yet",
        help: "write just `base`: the connector is the build target's",
        example: NONE,
    }
    BASE_ORDER "base-order" Mistake {
        summary: "The base is the first line of the stack, and there is one",
        help: "move `base` to the top of the stack, and keep one",
        example: ex!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt; base }\n}\n",
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { base; dirt }\n}\n"
        ),
    }
    ORDER "order" Mistake {
        summary: "The stack lists something before what it stands on",
        help: "list the surface (`ground`, `dirt`, `floor`) before the things that stand on it; the plan never reorders",
        example: NONE,
    }
    PLAN "plan" Mistake {
        summary: "The stack asks for something the plan cannot give yet",
        help: "the message says what the entry needs; list what makes it earlier in the stack",
        example: NONE,
    }
    PIN "pin" Mistake {
        summary: "A pin needs `top=` and `at=`, and belongs on a body",
        help: "write both on the pin, and put it inside a body such as `dirt`",
        example: NONE,
    }
    EMPTY "empty" Mistake {
        summary: "A pin's place covers nothing",
        help: "check the place the pin is written at",
        example: NONE,
    }
    THIN "thin" Mistake {
        summary: "A layer can never be as thick as the printer's skin",
        help: "raise the layer's top, or lower what is under it, so it is at least three layers thick",
        example: NONE,
    }
    BAND "band" Failure {
        summary: "A piece misses its band: a bug in the element, not in the document",
        help: "report it, with the tile that shows it",
        example: NONE,
    }
    CALM "calm" Mistake {
        summary: "The water's calm level cannot be read",
        help: "check `$waterSurfaceHeight` and what it reads",
        example: NONE,
    }
    STEEP "steep" Note {
        summary: "Pins force the dirt past its slope",
        help: "nothing is refused; move the pins apart, or soften their heights, for a gentler hill",
        example: NONE,
    }
    SHADOWED "shadowed" Note {
        summary: "A pin wins nowhere because later pins cover all of its place",
        help: "remove the pin, or move the later ones",
        example: NONE,
    }
    EDGE "edge" Note {
        summary: "A body meets the tile's edge at a height of its own, so it no longer butts a standard tile",
        help: "keep the pins off the edge, or accept that this tile joins only the tiles that match it",
        example: NONE,
    }
    LOOK "look" Mistake {
        summary: "An element cannot build what a control or the place asks for",
        help: "the message says which control; change it, or the place, so the element has room",
        example: NONE,
    }
    ELEMENT "element" Mistake {
        summary: "An element refused its controls",
        help: "the message says which control and why",
        example: NONE,
    }
    CROWN "crown" Note {
        summary: "The crown of one piece reaches over a taller neighbour",
        help: "move one of them, or raise the crown's piece; nothing is refused",
        example: NONE,
    }

    // parts and definitions
    USE "use" Mistake {
        summary: "Files use each other round to themselves",
        help: "files may use each other, but never round to themselves",
        example: NONE,
    }
    LOOP "loop" Mistake {
        summary: "Definitions call each other too deeply",
        help: "break the loop: a definition may not call itself, directly or through others",
        example: NONE,
    }
    NESTING "nesting" Mistake {
        summary: "That entry cannot go inside this one",
        help: "move it to the stack, or inside an element that takes it",
        example: NONE,
    }
    TOO_MANY "too-many" Mistake {
        summary: "The stack expands to more entries than a tile may hold",
        help: "scatter fewer things, or call a definition fewer times",
        example: NONE,
    }
    NOT_YET "not-yet" Mistake {
        summary: "That is not available yet",
        help: "write it another way; the message says what is",
        example: NONE,
    }
    SYSTEM "system" Mistake {
        summary: "The build target is not one the program knows",
        help: "use `db` (DungeonBlocks) or `ol` (OpenLOCK)",
        example: NONE,
    }
    PRINTER "printer" Mistake {
        summary: "A printer profile is not found or does not read",
        help: "`dharmatiles printers` lists them; the message says what is wrong with a profile",
        example: NONE,
    }
    NO_TILE "no-tile" Mistake {
        summary: "The file has no tile to build",
        help: "a tile document holds one `tile size=\"1x1\" { stack { … } }`; parts, printer and level files are only checked",
        example: NONE,
    }
    PALETTE "palette" Mistake {
        summary: "The palette does not read, so the tile is shown in the default paint",
        help: "the message says which line; mend the palette file",
        example: NONE,
    }

    // plates, maps and the inventory
    MAP "map" Mistake {
        summary: "A map or print list names a tile that cannot be read",
        help: "the message says why; mend the tile, or the path to it",
        example: NONE,
    }
    PLATES "plates" Mistake {
        summary: "Plates are not made from a draft build",
        help: "wait for the full build of the piece, then export",
        example: NONE,
    }
    SLOTS "slots" Mistake {
        summary: "A piece needs more filaments than the printer has slots",
        help: "use fewer materials, or print it on a printer with more slots",
        example: NONE,
    }
    GATE "gate" Failure {
        summary: "A piece fails the printability gate",
        help: "the message says which check; fix the geometry where it is made, never the threshold",
        example: NONE,
    }
    GROUPING "grouping" Note {
        summary: "Grouping plates by filament set takes more plates than packing by size",
        help: "set group to size in the panel to pack by size only",
        example: NONE,
    }
    VERIFY "verify" Note {
        summary: "A machine's numbers are not yet checked against the maker",
        help: "print a test piece before a long job",
        example: NONE,
    }
    INVENTORY "inventory" Failure {
        summary: "The inventory file cannot be read or written",
        help: "mend inventory.dinv, then `dharmatiles inventory relink <old> <new>`",
        example: NONE,
    }
    VIEW "view" Failure {
        summary: "The view of a print list could not be prepared",
        help: "the message says why; mend the list or the tile it names",
        example: NONE,
    }

    // the program and its files
    SOURCE "source" Failure {
        summary: "A file could not be read",
        help: "check that the file exists and that you may read it",
        example: NONE,
    }
    WRITE "write" Failure {
        summary: "A file could not be written",
        help: "check that the folder exists and that you may write to it",
        example: NONE,
    }
    BUILD "build" Failure {
        summary: "The build failed in the program, not in the document",
        help: "report it, with the tile and the message",
        example: NONE,
    }
    PANIC "panic" Failure {
        summary: "The program stopped on a bug of its own",
        help: "report it, with the tile and the message",
        example: NONE,
    }
    PAYLOAD "payload" Failure {
        summary: "The page sent something the program could not read",
        help: "reload the page; if it comes back, report it",
        example: NONE,
    }
    INTERNAL "internal" Failure {
        summary: "The program reached a state it should not",
        help: "report it, with the tile and the message",
        example: NONE,
    }
    BUG "bug" Failure {
        summary: "An element reached a case it does not build: a bug in the program",
        help: "report it, with the tile that shows it",
        example: NONE,
    }
}

/// The code with this id (`dt::type`), if the program has one.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Code> {
    ALL.iter().find(|c| c.id == id)
}

/// The word after `dt::`: `type` for `dt::type`.
#[must_use]
pub fn slug(id: &str) -> &str {
    id.strip_prefix("dt::").unwrap_or(id)
}

/// A code's anchor on the page that explains it: `dt-type`.  A hyphen, since a colon needs escaping in CSS and
/// scripts.
#[must_use]
pub fn anchor(id: &str) -> String {
    format!("dt-{}", slug(id))
}

/// Where the site is served from, derived from the repository address in `Cargo.toml` (the one place it is
/// written): `https://github.com/<owner>/<repo>` is served at `https://<owner>.github.io/<repo>`.
#[must_use]
pub fn site_base() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    match repo
        .strip_prefix("https://github.com/")
        .and_then(|r| r.split_once('/'))
    {
        Some((owner, name)) => format!("https://{owner}.github.io/{name}"),
        None => repo.to_owned(),
    }
}

/// The short address of a code's error page: `<site>/e/type`.
#[must_use]
pub fn explain_url(id: &str) -> String {
    format!("{}/e/{}", site_base(), slug(id))
}

/// The two lines printed under every error: the address, and the command that opens the local copy.
#[must_use]
pub fn explain_lines(id: &str) -> String {
    format!(
        "explain: {}\noffline: dharmatiles explain {id}",
        explain_url(id)
    )
}

/// What a terminal prints as an error's help: the help itself (when there is one), then the explain lines.
#[must_use]
pub fn help_with_explain(help: Option<&str>, id: &str) -> String {
    match help {
        Some(h) => format!("{h}\n{}", explain_lines(id)),
        None => explain_lines(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_is_unique_and_spelt_alike() {
        let mut seen = std::collections::BTreeSet::new();
        for c in ALL {
            assert!(seen.insert(c.id), "{} is registered twice", c.id);
            let s = slug(c.id);
            assert!(c.id.starts_with("dt::") && !s.is_empty(), "{}", c.id);
            assert!(
                s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{} must be lower-case words and digits joined by hyphens",
                c.id
            );
            assert!(!c.summary.is_empty() && !c.help.is_empty(), "{}", c.id);
        }
    }

    #[test]
    fn the_address_is_derived_from_the_repository() {
        let url = explain_url(TYPE);
        assert!(url.ends_with("/e/type"), "{url}");
        assert!(url.starts_with("https://"), "{url}");
    }
}
