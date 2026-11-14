//! The build record: where a build came from (document hash, seed, engine
//! version), written into each STL's header by `dt-print`.

/// Where a build came from. The front end writes it beside the STL in
/// full; the STL header carries a one-line summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRecord {
    /// Engine version, as the build script formats it (`v1.0.0-12-gabc1234`,
    /// `gabc1234-dirty`, …).
    pub engine: String,
    /// The document's path, such as `lib:tests/ground/1x1-flat.dtile`.
    pub document: String,
    /// 64-bit FNV-1a of the document's text.
    pub document_hash: u64,
    /// The tile's root seed.
    pub seed: u64,
    /// The connector system, `db` or `ol`.
    pub system: String,
    /// The printer profile's name (`default`, or another).
    pub printer: String,
}

/// `db`, or `db-fine` for the printer `fine` (and `db-bambu-x1c` for
/// `bambu/x1c`, the `/` made a `-` so a target is one folder name): the
/// printer is left out when it is `default`.
#[must_use]
pub fn target_name(system: &str, printer: &str) -> String {
    if printer == "default" {
        system.to_owned()
    } else {
        format!("{system}-{}", printer.replace('/', "-"))
    }
}

impl BuildRecord {
    /// The build target: the system, and the printer unless it is the
    /// default one (`db`, `ol`, `db-fine`).
    #[must_use]
    pub fn target(&self) -> String {
        target_name(&self.system, &self.printer)
    }

    /// The 80-byte STL header: never starts with `solid` (that marks ASCII
    /// STL), padded with spaces.
    #[must_use]
    pub fn header(&self) -> [u8; 80] {
        let line = format!(
            "dharmatiles {} {} seed={} doc={:016x} {}",
            self.engine,
            self.target(),
            self.seed,
            self.document_hash,
            self.document
        );
        let mut h = [b' '; 80];
        let n = line.len().min(80);
        h[..n].copy_from_slice(&line.as_bytes()[..n]);
        h
    }
}
