//! With the `embedded-library` feature, embeds `library/` (the `lib:` tiles
//! and parts) into the crate, for wasm and mobile sandboxes that cannot read
//! files; otherwise the table is empty and `lib:` is read from disk.
//! Writes `$OUT_DIR/library.rs`: a sorted table of (path, text).

use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else if p.extension().is_some_and(|x| x == "dtile") {
            let rel = p.strip_prefix(base).unwrap_or(&p);
            let key = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((key, p.clone()));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let lib = manifest.join("../../library");
    println!("cargo::rerun-if-changed={}", lib.display());
    let mut files = Vec::new();
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_LIBRARY").is_some() {
        walk(&lib, &lib, &mut files);
    }
    files.sort();
    let mut src = String::from(
        "/// The embedded library: (path below `library/`, text).\npub static FILES: &[(&str, &str)] = &[\n",
    );
    for (key, path) in files {
        src.push_str(&format!(
            "    ({key:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("library.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=cannot write {}: {e}", out.display());
    }
}
