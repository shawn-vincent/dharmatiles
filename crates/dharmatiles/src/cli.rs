//! The command line: arguments and subcommands.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// What `--version` prints after the name: the application version from
/// git and the tile formats this program reads.
pub(crate) fn version_line() -> &'static str {
    static LINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LINE.get_or_init(|| {
        format!(
            "{} (tile format {})",
            env!("DT_ENGINE_VERSION"),
            dt_doc::header::formats()
        )
    })
}

#[derive(Parser)]
#[command(
    name = "dharmatiles",
    about = "Build printable terrain tiles from .dtile documents",
    after_help = "With no command, opens the preview in the browser: your tiles ($DT_HOME, else ~/dharmatiles) and the library."
)]
pub(crate) struct Cli {
    /// None: `preview --open` on your tiles and the library.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// What `dharmatiles cache` does.
#[derive(Subcommand)]
pub(crate) enum CacheAction {
    /// Every entry: what it is a build of, its size and when it was last used.
    List,
    /// Remove everything the cache holds.
    Clear,
    /// Build a preview (the super-low tier: about 12 000 triangles, not yet
    /// checked) of every library tile into the cache, so the library opens at once the first time.  A
    /// release or an install runs it; `--into` writes a folder instead, laid
    /// out as the cache is, for an install to copy into the cache folder.
    Previews {
        /// Base system: `db` or `ol` (run it for each).
        #[arg(long, default_value = "db")]
        system: String,
        /// Printer profile, as for `build`.
        #[arg(long, default_value = "default")]
        printer: String,
        /// Write into this folder instead of the cache.
        #[arg(long)]
        into: Option<PathBuf>,
    },
}

/// What `dharmatiles printers` does.
#[derive(Subcommand)]
pub(crate) enum PrintersAction {
    /// Make profiles from a slicer's machine profiles: the bed, height, nozzle,
    /// excluded zones and filament count, with the slicer's `inherits` followed.
    /// With no slicer, says which are installed.  Nothing is written to a
    /// slicer's folders or to the library.
    Import {
        /// `bambu`, `orca`, `prusa` or `cura`.
        slicer: Option<String>,
        /// Models to import: a part of the name is enough (`x1 carbon`).
        models: Vec<String>,
        /// Import every model.
        #[arg(long)]
        all: bool,
        /// Read this slicer's profiles folder instead of the installed one.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Only list what the slicer has.
        #[arg(long)]
        list: bool,
        /// Replace a profile of yours that is already there.
        #[arg(long)]
        force: bool,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
}

/// What `dharmatiles inventory` does.
#[derive(Subcommand)]
pub(crate) enum InventoryAction {
    /// What you have, by tile, system and colour mode, with the seeds held
    /// (the default).  Shows one system: DungeonBlocks unless you say
    /// `--system ol` or `--system all`.
    List {
        /// `db`, `ol` or `all` (default `db`).
        #[arg(long)]
        system: Option<String>,
        /// Only `single` or `multi` colour prints.
        #[arg(long)]
        colour: Option<String>,
        /// Only tiles whose path has this text in it.
        #[arg(long)]
        tile: Option<String>,
        /// Only pieces that have a failed print.
        #[arg(long)]
        failed: bool,
    },
    /// Add a print of each tile named: a good one, or with `--failed` one
    /// that failed (kept in the log, not counted).  The seed is the tile's
    /// own unless you give one; the colour mode is the last you used.
    Have {
        /// Tiles: `lib:tiles/ground/1x1-grass`, `home:meadows/glade` or a path.
        #[arg(required = true)]
        tiles: Vec<String>,
        /// The seed it was printed with (default: the tile's own).
        #[arg(long)]
        seed: Option<u64>,
        /// `db` or `ol` (default `db`).
        #[arg(long)]
        system: Option<String>,
        /// `single` or `multi` (default: the last colour mode you wrote).
        #[arg(long)]
        colour: Option<String>,
        /// It failed: keep the line, do not count it.
        #[arg(long)]
        failed: bool,
        /// How many copies came off the printer (default 1).
        #[arg(long, default_value_t = 1)]
        count: u32,
        /// The date, `2026-11-09` (default: today, UTC).
        #[arg(long)]
        on: Option<String>,
        /// A note.
        #[arg(long)]
        note: Option<String>,
        /// Where it came from, `kit/plate-1`.
        #[arg(long)]
        from: Option<String>,
    },
    /// Mark one print of each tile named lost or broken.  The log keeps the
    /// line it came from; a `gone` line takes one away from the count.
    Gone {
        /// Tiles, as for `have`.
        #[arg(required = true)]
        tiles: Vec<String>,
        /// The seed (default: the tile's own).
        #[arg(long)]
        seed: Option<u64>,
        /// `db` or `ol` (default `db`).
        #[arg(long)]
        system: Option<String>,
        /// `single` or `multi` (default: the last colour mode you wrote).
        #[arg(long)]
        colour: Option<String>,
        /// The date (default: today, UTC).
        #[arg(long)]
        on: Option<String>,
        /// A note, `dropped on the floor`.
        #[arg(long)]
        note: Option<String>,
    },
    /// A tile has another path now: its prints follow it (a `renamed` line).
    /// The editor writes one itself when it renames or moves a tile.
    Relink {
        /// The old path as the inventory names it (`home:meadow`).
        old: String,
        /// The tile now.
        new: String,
        /// The date (default: today, UTC).
        #[arg(long)]
        on: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Build tiles to STL, through the printability gate.
    Build {
        /// Tile documents: files, or `lib:tiles/…` in the bundled library.
        #[arg(required_unless_present = "all")]
        paths: Vec<String>,
        /// Build every tile of the bundled library (not the test tiles).
        #[arg(long)]
        all: bool,
        /// Base system: `db` (DungeonBlocks) or `ol` (OpenLOCK).
        #[arg(long, default_value = "db")]
        system: String,
        /// Printer profile: a name from `library/printers/` or from
        /// `printers/` beside your tiles (`$DT_HOME`, else `~/dharmatiles`).
        /// Output goes to `out/<system>[-<printer>]/`.
        #[arg(long, default_value = "default")]
        printer: String,
        /// Replace each tile's seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Output folder (default: `out/` beside `dharmatiles.dtile`, else here).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the four preview views as PNG, overhangs red.
        #[arg(long)]
        png: bool,
        /// Also write a 3MF, each triangle painted with its material's
        /// filament. With a Bambu Studio template it is a Bambu project
        /// with one named, coloured slot per material.
        #[arg(long = "3mf")]
        threemf: bool,
        /// A project saved in Bambu Studio whose printer, process and
        /// filament settings the 3MF takes (default: `bambu.3mf` in
        /// `$DT_HOME`, else `~/dharmatiles`, when it exists).
        #[arg(long)]
        bambu_template: Option<PathBuf>,
        /// A palette document (see library/palettes/default.dtile) to paint
        /// with: its flat colours for the 3MF, its acrylic `ramp`s for the
        /// PNG renders.
        #[arg(long)]
        palette: Option<PathBuf>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Build every tile from nothing: do not read the cache of builds
        /// (see `dharmatiles cache`), and do not keep this build in it.
        #[arg(long)]
        no_cache: bool,
        /// How finely: `full` (the default, through the union and the gate),
        /// `low` (the same places with less detail, for browsing and the
        /// map) or `super-low` (a few thousand triangles).  A low tier is
        /// not yet checked: it writes the `--png` views only, never an STL
        /// or a 3MF.
        #[arg(long, default_value = "full")]
        tier: String,
    },
    /// Lay a print list (`.dmap`) out on a printer's plates and write them:
    /// one Bambu Studio project holding every plate, or one STL (or plain
    /// 3MF) per plate.  Every distinct piece is built in full (through the
    /// cache), packed large to small by its real footprint, and written to
    /// `out/plates/<list>/`.
    Plates {
        /// The print list.
        list: PathBuf,
        /// Printer profile, in place of the list's (default `bambu/x1c-ams`):
        /// `bambu/x1c`, `sovol/sv06`, or one of yours.
        #[arg(long)]
        printer: Option<String>,
        /// Base system, in place of the list's: `db` or `ol`.
        #[arg(long)]
        system: Option<String>,
        /// `single` (one filament, no paint) or `multicolour`, in place of
        /// the list's (default: multicolour when the printer holds more than
        /// one filament).
        #[arg(long)]
        mode: Option<String>,
        /// `filament` (pieces sharing filaments together, then large to
        /// small) or `size` (large to small alone).
        #[arg(long)]
        group: Option<String>,
        /// Between two pieces, mm (default 3).
        #[arg(long)]
        gap: Option<f64>,
        /// Between a piece and the bed's edge, mm (default 5).
        #[arg(long)]
        margin: Option<f64>,
        /// `allowed` or `off`: may a piece turn a quarter to fit.
        #[arg(long)]
        turn: Option<String>,
        /// `bambu` (one project, every plate; the default for a Bambu
        /// printer), `stl` (one per plate; the default for any other) or
        /// `3mf` (one plain 3MF per plate).
        #[arg(long)]
        format: Option<String>,
        /// Output folder (default: `out/` beside `dharmatiles.dtile`, else here).
        #[arg(long)]
        out: Option<PathBuf>,
        /// A project saved in Bambu Studio for the printer, whose settings
        /// the Bambu project takes (default: the profile's `template`, else
        /// `bambu.3mf` in `$DT_HOME`, else `~/dharmatiles`).
        #[arg(long)]
        bambu_template: Option<PathBuf>,
        /// A palette document to paint with.
        #[arg(long)]
        palette: Option<PathBuf>,
        /// Read and plan only: the printer, the pieces and whether the
        /// printer has the filaments.  Builds nothing, writes nothing.
        #[arg(long)]
        check: bool,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Do not read or keep the cache of builds.
        #[arg(long)]
        no_cache: bool,
        /// Print every piece the list names, whatever your inventory holds
        /// (by default a piece prints only what the inventory lacks of it:
        /// the count, less what you have; see `dharmatiles inventory`).
        #[arg(long)]
        all: bool,
    },
    /// Printer profiles: `import` reads the machine profiles of the slicers
    /// installed here (Bambu Studio, OrcaSlicer, PrusaSlicer, Cura) and writes
    /// profiles of your own into `printers/` beside your tiles.
    Printers {
        #[command(subcommand)]
        action: PrintersAction,
    },
    /// What you have printed: `list` (the default), `have <tile>`, `gone
    /// <tile>`, `relink <old> <new>`.  One log, `inventory.dinv` in
    /// `$DT_HOME` (else `~/dharmatiles`), made at the first line written.
    Inventory {
        #[command(subcommand)]
        action: Option<InventoryAction>,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Read and plan tiles without building geometry.
    Check {
        /// Tile documents.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Base system, as for `build`: `db` (DungeonBlocks) or `ol`
        /// (OpenLOCK). Units and sizes depend on it, so a tile can pass
        /// for one and fail for the other.
        #[arg(long, default_value = "db")]
        system: String,
        /// Printer profile, as for `build`: a name from `library/printers/`
        /// or from `printers/` beside your tiles.
        #[arg(long, default_value = "default")]
        printer: String,
        /// Replace each tile's seed, as for `build`.
        #[arg(long)]
        seed: Option<u64>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// List the library defaults applied, each pin's share of its place,
        /// and each override.
        #[arg(long)]
        explain: bool,
    },
    /// Mend what the program can mend in your tiles: a point off a seam moved onto the nearest, a misspelt word
    /// written right, each mistake that carries a fix.  Shows the change; `--write` makes it.
    Fix {
        /// Tile documents, or folders of them.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Write the fixed files (without it, only show what would change).
        #[arg(long)]
        write: bool,
        /// Base system, as for `check`: sizes in millimetres depend on it.
        #[arg(long, default_value = "db")]
        system: String,
        /// Printer profile, as for `check`.
        #[arg(long, default_value = "default")]
        printer: String,
    },
    /// Say what an error code means, from the local copy of its error page: `explain dt::type`.
    Explain {
        /// The code an error printed, such as `dt::type` (or just `type`).
        code: String,
    },
    /// List the elements a stack can hold, or one element's controls.
    Elements {
        /// An element's name: list its controls with units, ranges and defaults.
        name: Option<String>,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// What the docs site needs of its examples: each file's poster key, and with `--out` the poster's picture
    /// (a chore of the repository, run by `cargo xtask docs` and `cargo xtask examples`).
    #[command(hide = true)]
    Poster {
        /// The example documents.
        files: Vec<PathBuf>,
        /// Write each tile's top view as `KEY.png` here (skipped when it is there).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Serve the Model Context Protocol on stdio (check, build, preview and
    /// audit tiles for an agent).
    Mcp {
        /// Serve Streamable HTTP on this address (e.g. 127.0.0.1:8787) instead of stdio.
        /// Loopback only: a request whose Host or Origin is not the loopback, that is not
        /// application/json, or whose body is over 4 MiB is refused.
        #[arg(long)]
        http: Option<String>,
    },
    /// Run the printability gate on a binary STL file.
    Audit {
        /// The STL file.
        path: String,
        /// The tile's width in mm, for see-through holes (default: the mesh's).
        #[arg(long)]
        width: Option<f64>,
        /// The tile's height in mm (default: the mesh's).
        #[arg(long)]
        height: Option<f64>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
    /// The cache of builds: where it is and how much it holds (no action),
    /// `list` its entries, `clear` it, or build the library's `previews` into
    /// it.  The folder is `$DT_CACHE`, else `~/Library/Caches/dharmatiles`
    /// (macOS) or `$XDG_CACHE_HOME/dharmatiles`; `$DT_CACHE_LIMIT` bounds its
    /// size (default 4GB); `DT_CACHE=off` switches it off.
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Open a live, painted 3D view of tiles in the browser (127.0.0.1
    /// only).  Overhangs (over 65 degrees) are red, the 55-65 degree band orange; a tile
    /// rebuilds when you save it or a file it uses.  Ctrl-C stops.
    Preview {
        /// Tile documents (`.dtile`, `.stl`, `lib:tiles/…`), built at once;
        /// folders, listed with every tile in them (`lib:tiles/water` is a
        /// library group), each tile built when you choose it.  Folders
        /// follow new and deleted files.  None: your home (`$DT_HOME`, else
        /// ~/dharmatiles) and the library, with a switch between them.
        paths: Vec<String>,
        /// Base system: `db` or `ol`.  Default: the one the title bar's chip
        /// last chose (remembered in `$DT_HOME`), else `db`.
        #[arg(long)]
        system: Option<String>,
        /// Printer profile; see `build`.  Default: the chip's, else `default`.
        #[arg(long)]
        printer: Option<String>,
        /// Port on 127.0.0.1 (default: the first free one from 7878).
        #[arg(long)]
        port: Option<u16>,
        /// Open the browser.
        #[arg(long)]
        open: bool,
        /// A palette document (`ramp rock "#…" "#…"`, see
        /// library/palettes/default.dtile) to paint with; saving it repaints.
        #[arg(long)]
        palette: Option<PathBuf>,
    },
    /// Build tiles, then rebuild each whenever it or a file it uses changes.
    /// Ctrl-C stops.
    Watch {
        /// Tile documents: files, or `lib:tiles/…` (built once; the library
        /// is inside the program).
        #[arg(required = true)]
        paths: Vec<String>,
        /// Base system: `db` or `ol`.
        #[arg(long, default_value = "db")]
        system: String,
        /// Printer profile; see `build`.
        #[arg(long, default_value = "default")]
        printer: String,
        /// Replace each tile's seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Output folder (default: `out/` beside `dharmatiles.dtile`, else here).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the four preview views as PNG, overhangs red.
        #[arg(long)]
        png: bool,
        /// Stop after this many builds (for tests).
        #[arg(long, hide = true)]
        max_builds: Option<usize>,
    },
}
