//! Every word of the project's languages, said once.
//!
//! A word is a row of [`Word`]: the word, its one meaning and its kind.  The tile language's words (its keywords,
//! controls, choices, functions, units and elements) and the program's (the service's verbs, the command line's
//! commands and flags, the file kinds) are the rows of the tables below, written by one macro.  A table that
//! lives elsewhere (a function's argument count, an element's controls, the clap derive) is checked against these
//! rows by a test, and the guide's cheat sheet is written from them.
//!
//! The model is [`crate::codes`]: that a word exists is a fact about the program, so it lives here.  The decoder
//! matches a keyword by its constant (`words::TILE`), never by a string literal, and a test fails on a keyword
//! written as a literal in the decoder.  The vocabulary tests, in the crates that own each surface, fail on a word
//! used and not registered, a word registered twice in a kind, a word registered and not used, a control name
//! used for two measures, and a word used as two kinds of word, unless `ledger.kdl` lists the collision.
//!
//! To add a word: one row in the table of its kind, with a constant name when code reads it by word.

/// What a word is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordKind {
    /// A node or property name the decoder matches (`tile`, `define`, `at`).
    Keyword,
    /// A control: a name written as `name=value` or as a child node, on an element, a node or a printer.
    Control,
    /// A word a control takes as its value (`bond=running`).
    Choice,
    /// A function, a weighting or a coordinate of the expression language.
    Function,
    /// A unit word (`mm`, `sq`).
    Unit,
    /// An element of a stack (`rock`, `wall`).
    Element,
    /// A call of the editor's service.
    Verb,
    /// A command of the command line.
    Command,
    /// A long flag of the command line, without its dashes.
    Flag,
    /// A kind of file, by its extension.
    FileKind,
}

impl WordKind {
    /// Whether the kind is a word of the tile language: one namespace, in which a word does one job.
    #[must_use]
    pub fn is_tile_language(self) -> bool {
        matches!(
            self,
            Self::Keyword
                | Self::Control
                | Self::Choice
                | Self::Function
                | Self::Unit
                | Self::Element
        )
    }
}

/// One word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Word {
    /// The word as written.
    pub word: &'static str,
    /// What it means, in a short sentence.
    pub meaning: &'static str,
    /// What it is.
    pub kind: WordKind,
}

/// Write each word once: its row in its table, a constant for each row that has a name (code reads such a word
/// by it), and [`NAMES`] from the same lines.
macro_rules! words {
    ($(
        $(#[$doc:meta])*
        $table:ident $kind:ident { $( $($name:ident)? $word:literal $meaning:literal, )* }
    )*) => {
        $( $( $( pub const $name: &str = $word; )? )* )*
        $(
            $(#[$doc])*
            pub const $table: &[Word] = &[
                $( Word { word: $word, meaning: $meaning, kind: WordKind::$kind }, )*
            ];
        )*
        /// Every table, so a word can be looked for in all of them.
        pub const TABLES: &[&[Word]] = &[ $( $table, )* ];
        /// The constants: their names and the words they hold, for the tests that look for a constant's use.
        pub const NAMES: &[(&str, &str, WordKind)] = &[ $( $( $( (stringify!($name), $word, WordKind::$kind), )? )* )* ];
    };
}

/// Every word of every table.
pub fn all() -> impl Iterator<Item = &'static Word> {
    TABLES.iter().flat_map(|t| t.iter())
}

/// The row of a word of a kind.
#[must_use]
pub fn find(kind: WordKind, word: &str) -> Option<&'static Word> {
    all().find(|w| w.kind == kind && w.word == word)
}

words! {
    /// The nodes and properties the decoder matches by word: it reads them by constant, and no literal of them is in the decoder.
    KEYWORDS Keyword {
        TILE "tile" "the one tile of a file: its size, seed and stack",
        DEFINE "define" "a named part: a thing or a layer other nodes can call",
        USE "use" "bring definitions in from another file",
        LET "let" "name a value",
        OVERRIDE "override" "replace a library name for this file",
        PARAM "param" "a value a definition takes from its caller",
        POINT "point" "a named point of a tile",
        LINE "line" "a line drawn through points",
        REGION "region" "an area of the tile",
        STACK "stack" "what the tile is made of, in the order it is built",
        DEFAULTS "defaults" "values every element of a kind starts from",
        PRINTER "printer" "a printer profile",
        SEAMS "seams" "the edge contract: the seam words along a square edge",
        WITHIN "within" "a clause of a region: all that lies within a distance of a place",
        EXCEPT "except" "a clause of a region: regions taken out of it",
        KIND "kind" "what kind of value a param takes",
        AT "at" "where an entry goes: a region, a line, a point or a place word",
        ID "id" "a name an entry is edited by",
        WEIGHT "weight" "how often a child of a scatter is drawn",
        DHARMATILES "dharmatiles" "the header line that says a file is a dharmatiles document",
    }

    /// The controls: every name an element, a node or a printer takes.
    CONTROLS Control {
        ABOUT "about" "a sentence on what the tile is",
        "balance" "how evenly a fork shares its attractors, 0 to 1",
        "bare" "the least share left bare",
        "bed" "the size of a printer bed",
        "blocks" "weight of chunky blocks",
        "boards" "boards across one square",
        "bond" "random courses, or brick-style offsets",
        BOUNDED_BY "boundedBy" "the lines that divide the tile into this region and the rest",
        "branch" "the branching knobs of a tree",
        "branching" "few long limbs, or many forks",
        "brand" "the maker of a printer",
        "bumpSize" "how wide the hills are in its band",
        "buried" "how much of each rock is under the ground",
        "centre" "radius of the centre dome",
        "clearingWidth" "how wide a clearing is, drawn for each",
        "clearings" "patches of bare earth",
        "clods" "lumps of earth on the surface",
        CLOSED "closed" "a line that ends where it starts",
        "clumpSize" "size of each clump",
        "clumps" "how many clumps",
        "columns" "weight of tall upright stones",
        "coping" "what tops a drystone wall",
        CORNER "corner" "a through of one point",
        "count" "how many, in total",
        "cover" "the share kept: ground grassed over, or places that keep a thing",
        "crown" "the crown of a tree",
        CURVE "curve" "how a line curves through its points",
        "depth" "how deep the gaps between stones look",
        DISTANCE "distance" "how far from the place it reaches, in mm at the 35 mm square",
        "dome" "height of the centre dome",
        "eagerness" "1 splits early, 0 only near the tips",
        "edges" "the corners of a wall's stones",
        ENDS_CONTROL "ends" "how a line leaves a tile side",
        "excluded" "zones of a bed that cannot be printed on",
        "exponent" "pipe-model exponent",
        FILL "fill" "a point inside the region, '' with units",
        "flow" "how tidily the blades lean together",
        "from" "the profile a printer starts from",
        "from-slicer" "the slicer profile a printer starts from",
        "gap" "least space between neighbours",
        HEADING "heading" "a through of one point",
        "height" "how tall",
        "joints" "the gaps between stones",
        "layer" "a printer profile's layer height",
        "leaf" "one leaf of a tree",
        "leaves" "separate leaves, just clumps, or a winter tree",
        "ledges" "steps down the free faces of a cliff",
        "length" "how long",
        "maxSlope" "the steepest surface under it that dirt stays on",
        "minRadius" "branch radius at a tip",
        "model" "the model of a printer",
        "monoliths" "weight of ground-to-top stones",
        NAME "name" "what the tile calls a line or a region",
        "near" "the things a scatter answers to",
        "nozzle" "the width of a printer nozzle",
        "odd" "weight of irregular lumps",
        OF "of" "the place or thing a clause reaches round or answers to",
        "overhang" "the angle past which a face counts as overhang",
        "patches" "broken patches along the run",
        "pattern" "how the places fall",
        "perSquare" "how many per square of the area, or of length along a line",
        "petal" "height of each petal dome",
        "petals" "petals round the centre",
        PULL "pull" "a through of one point",
        "reach" "how far out a scatter or its answer reaches",
        "rock" "what a cliff is stacked from",
        "rounding" "how worn the stone corners are",
        "ruin" "how much has fallen",
        "seams" "whether lines may cross an edge only at a seam",
        SEED "seed" "the layout: another seed, another arrangement of the same tile",
        "shape" "the shape of a rock or a crown",
        "shattered" "broken patches of a cliff",
        SIZE "size" "how big: squares across and up for a tile, a width for a rock or flower",
        "slabs" "slabs across one square",
        "slots" "how many filaments a printer holds",
        "split" "half-angle of the cone a branch keeps",
        "spread" "how far a clump reaches from its middle",
        "steepest" "the angle past which an overhang is steep",
        "stem" "height of the column under each dome",
        "stems" "stems in a clump",
        "stoneSize" "small broken stones, or big masses",
        "stones" "the stones of a wall",
        "style" "how a thing is made and laid",
        "target" "aim point in the attractor cloud, 0 low to 1 high",
        "template" "a slicer project a printer prints through",
        "texture" "the surface finish",
        "thickness" "wall thickness",
        "thin" "the share of things within reach that are left out",
        THROUGH "through" "the points it runs through",
        "top" "the band a body fills, or how high a thing stands, as a height",
        "trunk" "the bare trunk of a tree",
        "variants" "draw each thing as one of this many variants, each built once and copied turned and placed",
        "verify" "whether the bed is checked against the slicer",
        "wear" "how sharp or worn a rock's edges are",
        WIDTH "width" "how wide",
        WOBBLE "wobble" "how far the line may wander off its points",
        LINE_SETTING "line" "a printer profile's line width",
    }

    /// The words a control takes as its value.
    CHOICES Choice {
        "angular" "broken rock",
        "bare" "a winter tree",
        "battlements" "merlons and crenels",
        "blocks" "blocky granite",
        "brick" "irregular brick",
        "broken" "between the two",
        "chipped" "chipped cut stone",
        "clumped" "things gathered in clumps, kept apart within each",
        "clumps-only" "just foliage clumps",
        "columns" "basalt columns",
        "combed" "blades lean together, tidily",
        "cone" "pointed at the top, like a fir",
        "cover" "a dense cover of things",
        "crisp" "sharp edges",
        "cut-stone" "dressed ashlar",
        "dense" "many forks",
        "dressed" "near-smooth ashlar, crisp edges",
        "even" "boards half a square long, each course set half a board on",
        "few" "a scatter of clods",
        "fieldstone" "drystone fieldstone",
        "flat" "flat: slabs, or a flat wall top",
        "free" "lines cross an edge anywhere, or take their direction from their neighbours",
        "full" "up to the wall-top level",
        "hewn" "rough-hewn all over",
        "irregular" "random courses and bays",
        "loose" "loose stones perched on top",
        "low" "low: half the wall top",
        "many" "crumbly, cloddy earth",
        "massive" "big masses: 1.35x rubble size",
        "mixed" "a mix: the run shares",
        "none" "none of it",
        "normal" "between the two",
        "on" "separate leaves over the clumps",
        "on-edge" "stones set on edge",
        "open" "few long limbs",
        "random" "boards of mixed lengths, joints staggered",
        "regular-brick" "uniform brick in running bond",
        "rough" "sawn boards, coarse grain",
        "round" "a round crown",
        "rounded" "river stones",
        "rubble" "small broken stones",
        "running" "uniform courses, half-unit offsets",
        "slabs" "layered sandstone",
        "smooth" "one smooth curve, or planed boards",
        "some" "clods here and there",
        "spaced" "things kept apart, the room each takes between them",
        "square" "square to the side",
        "standing" "rocks, walls, trunks and cliffs",
        "straight" "straight runs from point to point",
        "tall" "a narrow column",
        "wavy" "blades meander together, in phase",
        "wide" "a broad, flat-bottomed spread",
        "wild" "blades every which way",
        "worn" "chipped, rounded or weathered",
    }

    /// The words a param's `kind=` takes.
    KIND_WORDS Choice {
        "length" "a length",
        "angle" "an angle",
        "plain" "a plain number",
        "region" "a region",
        "point" "a point",
        "thing" "a thing a scatter may hold",
        "layer" "a layer of the stack",
    }

    /// The functions, weightings and coordinates of the expression language.
    CALLS Function {
        "sin" "the sine of an angle",
        "cos" "the cosine of an angle",
        "abs" "the size of a number without its sign",
        "min" "the lesser of two numbers",
        "max" "the greater of two numbers",
        "clamp" "a number held between two bounds",
        "smooth" "smoothstep: an easing from 0 to 1",
        "x" "across: where a point is, or the across of a region in a unit",
        "y" "down: where a point is, or the down of a region in a unit",
        "d" "how far from the middle",
        "a" "the angle round the middle",
        "point" "a point in a frame, from two numbers",
        "polar" "a point in a frame, from a distance and an angle",
        "parent" "the region that holds a region",
        "flat" "a weighting: every value equally likely",
        "low" "a weighting: low values likelier",
        "high" "a weighting: high values likelier",
        "bell" "a weighting: values near the middle likelier",
        "peak" "a weighting: values near a chosen peak likelier",
        "ends" "a weighting: values near the ends likelier",
        "steps" "a weighting: one weight for each step",
        "mix" "a weighting: parts drawn by weight",
    }

    /// The unit words.
    UNIT_WORDS Unit {
        "sq" "one square of the tile grid",
        "mm" "a millimetre",
        "in" "an inch",
        "dmm" "a tenth of a millimetre",
        "layer" "one printed layer",
        "deg" "a degree",
        "rad" "a radian",
        "pi" "half a turn in radians",
    }

    /// The elements a stack holds.
    ELEMENT_WORDS Element {
        "boulder" "a rock with boulder sizes, about 4.5–8dmm",
        "cliff" "a cliff of stacked rock",
        "dirt" "a textured earth surface",
        "flagstones" "a floor of stone slabs, a wall laid on its side",
        "flower" "a wildflower: a thin dome on a column, ringed by petal domes on cones",
        "grass" "separate sculpted blades in tufts",
        "pebble" "a rock with pebble sizes, about 1–2.2dmm",
        "planks" "a floor of wooden boards",
        "reeds" "clumps of tapered reeds at the waterline, leaning toward the water",
        "rock" "one rock",
        "scatter" "more than one thing",
        "stone" "a rock with stone sizes, about 1–6dmm",
        "tree" "a space-colonisation tree with foliage clumps and leaves",
        "wall" "a masonry wall",
        "water" "still water with swells round its shores and rocks",
    }

    /// The calls of the editor's service.
    VERBS Verb {
        "about" "the program and its version",
        "build" "queue a build of a tile",
        "buildThing" "draw one thing of a tile alone",
        "check" "read and plan a tile, or a list, without building",
        "checkValue" "read one written value without building a tile",
        "deleteTile" "delete a tile for good",
        "draft" "the quick draft of an edit",
        "duplicateTile" "copy a tile beside itself",
        "edit" "apply an edit to a tile and say what it made",
        "elements" "the elements and their controls",
        "exportTile" "the files of a tile, to save",
        "inventoryGone" "record a print as lost or broken",
        "inventoryHave" "record a print that came off the printer",
        "inventoryRecord" "record a line of the inventory",
        "inventoryRelink" "make a moved tile's prints follow it",
        "inventoryReprint" "put a printed tile back on a list to print again",
        "language" "the language's word tables",
        "listsAdd" "add a tile to a print list",
        "listsCount" "change how many of a piece a list prints",
        "listsNew" "start a print list",
        "listsRemove" "take a piece off a print list",
        "listsRestore" "put back a piece taken off a list",
        "mapsAdd" "put a tile on a map",
        "mapsBatch" "several edits of a map as one",
        "mapsDup" "copy a piece of a map",
        "mapsPin" "pin a piece of a map in place",
        "mapsRemove" "take a piece off a map",
        "mapsSet" "change a piece of a map",
        "mapsUnpin" "unpin a piece of a map",
        "mapsUnpinAll" "unpin every piece of a map",
        "messages" "the text of the messages the page shows",
        "messagesReport" "the issue a defect makes",
        "mine" "my tiles and the places",
        "moveTile" "move a tile to another folder",
        "names" "the names a position of a tile can take",
        "newFolder" "make a folder",
        "newTile" "make a tile",
        "offers" "what can be written at a position",
        "platesExport" "the files of a plate set, to save",
        "platesSet" "change a plan of plates",
        "platesShow" "plan a list or map on plates",
        "printed" "whether a tile has been printed",
        "printedSet" "whether a set of tiles has been printed",
        "rename" "rename a tile",
        "renameFolder" "rename a folder",
        "save" "write a tile's source",
        "setTarget" "choose the system and printer to build for",
        "step" "one edit and the draft of what it made",
        "text" "the text the page reads at startup",
        "tiersWant" "ask for builds at a finer or coarser tier",
        "tokens" "a text cut into words with their kinds",
        "trashFolder" "put a folder in the Trash",
        "trashTile" "put a tile in the Trash",
        "undoTrash" "take something out of the Trash",
    }

    /// The commands of the command line.
    COMMANDS Command {
        "audit" "Run the printability gate on a binary STL file",
        "build" "Build tiles to STL, through the printability gate",
        "cache" "The cache of builds",
        "check" "Read and plan tiles without building geometry",
        "clear" "Remove everything the cache holds",
        "elements" "List the elements a stack can hold, or one element's controls",
        "explain" "Say what an error code means, from the local copy of its error page",
        "fix" "Mend what the program can mend in your tiles",
        "gone" "Mark one print of each tile named lost or broken",
        "have" "Add a print of each tile named",
        "import" "Make profiles from a slicer's machine profiles",
        "inventory" "What you have printed",
        "list" "What you have, by tile, system and colour mode, with the seeds held",
        "mcp" "Serve the Model Context Protocol on stdio",
        "plates" "Lay a print list",
        "poster" "What the docs site needs of its examples",
        "preview" "Open a live, painted 3D view of tiles in the browser",
        "previews" "Build a preview (the super-low tier",
        "printers" "Printer profiles",
        "relink" "A tile has another path now",
        "watch" "Build tiles, then rebuild each whenever it or a file it uses changes",
    }

    /// The long flags of the command line, without their dashes.
    FLAGS Flag {
        "3mf" "Also write a 3MF, each triangle painted with its material's filament",
        "all" "Build every tile of the bundled library",
        "bambu-template" "A project saved in Bambu Studio whose printer, process and filament settings the",
        "check" "Read and plan only",
        "colour" "Only single or multi colour prints",
        "count" "How many copies came off the printer",
        "explain" "List the library defaults applied, each pin's share of its place, and each overr",
        "failed" "Only pieces that have a failed print",
        "force" "Replace a profile of yours that is already there",
        "format" "bambu (one project, every plate",
        "from" "Where it came from, kit/plate-1",
        "gap" "Between two pieces, mm",
        "group" "filament (pieces sharing filaments together, then large to small) or size (l",
        "height" "The tile's height in mm",
        "http" "Serve Streamable HTTP on this address",
        "into" "Write into this folder instead of the cache",
        "json" "One JSON report on stdout",
        "list" "Only list what the slicer has",
        "margin" "Between a piece and the bed's edge, mm",
        "max-builds" "Stop after this many builds",
        "mode" "single (one filament, no paint) or multicolour, in place of the list's (defa",
        "no-cache" "Build every tile from nothing",
        "note" "A note",
        "on" "The date, 2026-11-09",
        "open" "Open the browser",
        "out" "Output folder (default",
        "palette" "A palette document",
        "path" "Read this slicer's profiles folder instead of the installed one",
        "png" "Also write the four preview views as PNG, overhangs red",
        "port" "Port on 127.0.0.1",
        "printer" "Printer profile: a name from library/printers/ or from printers/ beside your",
        "seed" "Replace each tile's seed",
        "system" "Base system: db (DungeonBlocks) or ol (OpenLOCK)",
        "tier" "How finely: full",
        "tile" "Only tiles whose path has this text in it",
        "turn" "allowed or off: may a piece turn a quarter to fit",
        "width" "The tile's width in mm, for see-through holes",
        "write" "Write the fixed files",
    }

    /// The kinds of file a person writes, by extension.
    FILE_KINDS FileKind {
        DTILE "dtile" "a tile, a part, a printer or a palette",
        DMAP "dmap" "a print list or a map",
        DINV "dinv" "an inventory of what has been printed",
    }
}
