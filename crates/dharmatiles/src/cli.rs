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
    /// Build a preview (the draft: coarse cells, no union) of every library
    /// tile into the cache, so the library opens at once the first time.  A
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
    /// Carry documents from format 0 to format 1, the one this program reads,
    /// in place: the older language changes first (`ground`, `level=`,
    /// `bumps=`), then the header line, `$` names, units and points from the
    /// north-west corner.  Each migrated tile is checked as the program reads
    /// it now, and one that does not read stops it.  A document already at
    /// format 1 is left alone, whatever its minor or build part.
    Migrate {
        /// Documents, or folders of them.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Say what would change and write nothing; exits 1 when something would.
        #[arg(long)]
        check: bool,
    },
    /// List the elements a stack can hold, or one element's controls.
    Elements {
        /// An element's name: list its controls with units, ranges and defaults.
        name: Option<String>,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Serve the Model Context Protocol on stdio (check, build, preview and
    /// audit tiles for an agent).
    Mcp {
        /// Serve Streamable HTTP on this address (e.g. 127.0.0.1:8787) instead of stdio.
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
