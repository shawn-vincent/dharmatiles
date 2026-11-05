//! The command line: arguments and subcommands.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "dharmatiles",
    version = env!("DT_ENGINE_VERSION"),
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
        /// Connector system: `db`, `ol`, or `both`.
        #[arg(long, default_value = "db")]
        system: String,
        /// Replace each tile's seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Output folder (default: `out/` beside `dharmatiles.dtile`, else here).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the four preview views as PNG, overhangs red.
        #[arg(long)]
        png: bool,
        /// Also write a 3MF with each triangle's material (earth, rock,
        /// water, grass, …) assigned as a 3MF base material.
        #[arg(long = "3mf")]
        threemf: bool,
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
    },
    /// List the built-in looks, or one look's controls.
    Looks {
        /// A look's name: list its controls with units, ranges and defaults.
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
        /// Connector system: `db` or `ol`.
        #[arg(long, default_value = "db")]
        system: String,
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
        /// Connector system.
        #[arg(long, default_value = "db")]
        system: String,
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
