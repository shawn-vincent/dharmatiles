//! The command line: arguments and subcommands.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// What `--version` prints after the name: the application version from
/// git and the tile formats this program reads.
pub(crate) fn version_line() -> &'static str {
    static LINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LINE.get_or_init(|| {
        format!(
            "{} (tile formats {})",
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
    },
    /// Read and plan tiles without building geometry.
    Check {
        /// Tile documents.
        #[arg(required = true)]
        paths: Vec<String>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// List the library defaults applied, each pin's share of its place,
        /// and each override.
        #[arg(long)]
        explain: bool,
    },
    /// Carry documents to the format this program writes, in place: the older
    /// language changes first (`ground`, `level=`, `bumps=`), then format 0 to
    /// format 1 (the header line, `$` names, units, points from the
    /// north-west corner).  Each tile is built before and after, and a mesh
    /// that moves stops it.  A document already at the format is left alone,
    /// whatever its minor or build part.  Until 1.0.0 ships format 1 this
    /// needs DT_FORMAT_PREVIEW=1.
    Migrate {
        /// Documents, or folders of them.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Say what would change and write nothing; exits 1 when something would.
        #[arg(long)]
        check: bool,
        /// Skip the hash guard: build no tile before and after.
        #[arg(long)]
        no_guard: bool,
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
