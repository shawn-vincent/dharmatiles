//! `cargo xtask editor <out-dir>`: the editor page as static files, with no program behind it.
//!
//! The page (`web/preview/`), the visual system it shares with the site (`docs/assets/theme.css`, the logo, the fonts) and the
//! engine bundle (`cargo xtask wasm`, written to `<out-dir>/engine`) are laid out as the page asks for them, so any static
//! server over `<out-dir>` serves the editor, and `?host=worker` makes the page talk to the service in a web worker
//! (`js/editor-worker.js`) instead of the program's HTTP.  The program's own page needs none of this.

use std::fs;
use std::path::{Path, PathBuf};

use crate::wasm;

/// Copy `from` to `to` (a folder, recursively, or a file), leaving out what a served page never reads.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let meta = fs::metadata(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    if meta.is_dir() {
        fs::create_dir_all(to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
        for entry in
            fs::read_dir(from).map_err(|e| format!("cannot list {}: {e}", from.display()))?
        {
            let entry = entry.map_err(|e| format!("cannot list {}: {e}", from.display()))?;
            let name = entry.file_name();
            // a type declaration is for `tsc`, never for the browser
            if name.to_string_lossy().ends_with(".d.ts") || name == "node_modules" {
                continue;
            }
            copy(&entry.path(), &to.join(&name))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        }
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
    }
}

/// The page and what it reads, laid out in `out`: `index.html`, its style, the visual system it shares with the site, its
/// scripts and the vendored libraries.  Every reference in `index.html` is relative, so the same files work at the root of a
/// static server and in a folder of the website.
fn lay_page(root: &Path, out: &Path) -> Result<(), String> {
    let page = root.join("web/preview");
    let assets = root.join("docs/assets");
    for (from, to) in [
        (page.join("index.html"), "index.html"),
        (page.join("style.css"), "style.css"),
        (page.join("favicon.svg"), "favicon.svg"),
        (page.join("js"), "js"),
        (page.join("vendor"), "vendor"),
        (assets.join("theme.css"), "theme.css"),
        (assets.join("logo.svg"), "logo.svg"),
        (assets.join("fonts"), "fonts"),
    ] {
        copy(&from, &out.join(to))?;
    }
    Ok(())
}

/// `cargo xtask editor [--site | --page] <out-dir>`.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let usage = "usage: cargo xtask editor [--site | --page] <out-dir>";
    let (mode, out) = match args {
        [out] => ("", out),
        [mode, out] if mode == "--site" || mode == "--page" => (mode.as_str(), out),
        _ => return Err(usage.to_owned()),
    };
    let out = PathBuf::from(out);
    let engine =
        |dir: &Path| wasm::run_wasm(root, &[dir.join("engine").to_string_lossy().into_owned()]);
    match mode {
        // only the page's files: what the website's build adds to the pages it already has, beside the engine its own job builds
        "--page" => lay_page(root, &out),
        // the website as the program ships it (docs/, the editor at `editor/`, the engine at `engine/`), for a static server
        // over it: the pages' "Open in the editor" links work
        "--site" => {
            copy(&root.join("docs"), &out)?;
            lay_page(root, &out.join("editor"))?;
            engine(&out)?;
            println!(
                "the site is in {0}: serve it with any static server, e.g.\n  python3 -m http.server --directory {0} --bind 127.0.0.1 7931\nand open http://127.0.0.1:7931/elements/rock.html",
                out.display()
            );
            Ok(())
        }
        _ => {
            lay_page(root, &out)?;
            // the engine: one build, the same bundle the site's cards run
            engine(&out)?;
            println!(
                "the editor is in {0}: serve it with any static server, e.g.\n  python3 -m http.server --directory {0} --bind 127.0.0.1 7931\nand open http://127.0.0.1:7931/index.html?host=worker",
                out.display()
            );
            Ok(())
        }
    }
}
