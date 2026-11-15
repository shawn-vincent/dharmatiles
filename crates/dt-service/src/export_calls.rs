//! Export: the files of a tile (`exportTile`) and of a print list's plates (`platesExport`).  The files' bytes are made
//! once (`dt_print::tile_files` for a tile; [`plate_files`] for plates, over `dt_print`'s [`Builds`]), for both
//! transports: the program writes them where `dharmatiles build` does (its `out/` folders), and the browser editor's
//! service answers them as [`Download`]s, which the page saves through the browser's own download.  A download is a
//! typed answer (a name, a media type, the bytes as base64 text, since the answer is JSON): the page makes a file of it
//! with one generic call, so nothing else in the page knows what an STL is.

use dt_eval::{Built, Diag, Options, Tier, TileRecord};
use dt_print::plates::builds::Builds;
use dt_print::plates::export::{File, Format};
use dt_print::plates::{Layout, Overrides, Plan};
use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::plates_calls::{PlatesHost, plan_from};
use crate::tile_host::TileRef;

/// A file the page saves: its name, its media type and its bytes as standard base64 (an answer is JSON).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub name: String,
    pub media_type: String,
    pub data: String,
}

impl Download {
    /// `bytes` as the file `name`, its media type from the name's extension.
    #[must_use]
    pub fn of(name: &str, bytes: &[u8]) -> Self {
        let media_type = match name.rsplit('.').next() {
            Some("stl") => "model/stl",
            Some("3mf") => "model/3mf",
            Some("json") => "application/json",
            _ => "application/octet-stream",
        };
        Self {
            name: name.to_owned(),
            media_type: media_type.to_owned(),
            data: base64(bytes),
        }
    }

    /// The file's bytes.
    ///
    /// # Errors
    /// The data is not base64 (a download made by [`Download::of`] always is).
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        unbase64(&self.data)
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64.
#[must_use]
pub fn base64(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Standard base64, read.
///
/// # Errors
/// A character that is not base64, or a length that is not a multiple of four.
pub fn unbase64(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(4) {
        return Err("base64 text is a multiple of four characters".to_owned());
    }
    let value = |b: u8| ALPHABET.iter().position(|a| *a == b);
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for quad in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        let mut real = 0;
        for &b in quad {
            n <<= 6;
            if b != b'=' {
                let v = value(b).ok_or_else(|| format!("{:?} is not base64", char::from(b)))?;
                n |= u32::try_from(v).map_err(|e| e.to_string())?;
                real += 1;
            }
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..real.max(1)]);
    }
    Ok(out)
}

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct ExportReq {
    /// Also write the painted 3MF.
    pub threemf: Option<bool>,
    /// Only say where the files would go.
    #[serde(rename = "where")]
    pub ask_where: Option<bool>,
}

/// An export: where it would go, or what it made.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(untagged)]
pub enum ExportRes {
    Where { out: String, shown: String },
    Built(ExportBuilt),
}

#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct ExportBuilt {
    pub path: String,
    /// The files written, by path: the program's (empty where the files are downloads).
    pub files: Vec<String>,
    /// The files to save: the browser editor's (empty where the program wrote them).
    pub downloads: Vec<Download>,
    /// The build's record: whether it passes the gate is `build.audit.failures`.
    pub build: Box<TileRecord>,
}

/// `exportTile`: write the tile's STL (and 3MF) where `dharmatiles build` does, or hand them over to save.
pub const EXPORT_TILE: Verb<ExportReq, ExportRes> = Verb::new("exportTile");

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct PlatesExportReq {
    pub list: String,
}

/// The plates made.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlatesExported {
    /// The files written, by path: the program's (empty where the files are downloads).
    pub files: Vec<String>,
    /// The files to save: the browser editor's (empty where the program wrote them).
    pub downloads: Vec<Download>,
    pub format: String,
    /// The pieces that fail the printability gate, by tile.
    pub failures: Vec<String>,
    pub plates: usize,
    pub total_s: f64,
    pub out: String,
}

/// `platesExport`: build every piece in full, lay the plates out by what they cover and write the files (or hand
/// them over to save).
pub const PLATES_EXPORT: Verb<PlatesExportReq, PlatesExported> = Verb::new("platesExport");

/// Where a tile's files are named under: a file of yours goes to `<target>/<name>-<target>.stl`, not under the folders
/// it sits in (a library tile mirrors the library, as `dharmatiles build` does).
pub fn place_out(b: &mut Built, tile: &str, target: &str) {
    if !tile.starts_with(dt_eval::LIB) {
        let stem = std::path::Path::new(tile)
            .file_stem()
            .map_or_else(|| "tile".to_owned(), |s| s.to_string_lossy().into_owned());
        b.out = std::path::PathBuf::from(target).join(format!("{stem}-{target}.stl"));
    }
}

/// `exportTile` where the files are downloads: the tile built in full as saved, through the gate, and its files'
/// bytes (the same function `dharmatiles build` writes them with).  The 3MF is the plain painted one: a Bambu Studio
/// template is a file of the program's folder, which a page has not.
///
/// # Errors
/// [`Refusal`]: the tile's mistakes, or a file that cannot be made.
pub fn export_tile(
    host: &impl PlatesHost,
    t: &TileRef,
    v: &ExportReq,
) -> Result<ExportRes, Refusal> {
    if v.ask_where == Some(true) {
        return Ok(ExportRes::Where {
            out: "downloads".to_owned(),
            shown: "your browser's downloads".to_owned(),
        });
    }
    let opts = Options {
        tier: Tier::Full,
        ..host.options()
    };
    let target = opts.target();
    let t0 = host.seconds();
    let mut b = host
        .build_piece(&t.path, &opts)
        .map_err(|e| Refusal::of_mistakes(&e))?;
    place_out(&mut b, &t.path, &target);
    let record = TileRecord::of(&b, &opts.printer, host.seconds() - t0);
    let verdict = b.verdict(&opts.printer);
    let files = dt_print::tile_files(
        &b,
        &verdict,
        &record,
        v.threemf.unwrap_or(false),
        None,
        &host.palette(),
    )
    .map_err(|e| Refusal::failure(Status::Internal, e))?;
    Ok(ExportRes::Built(ExportBuilt {
        path: t.path.clone(),
        files: Vec::new(),
        downloads: downloads(&files),
        build: Box::new(record),
    }))
}

fn downloads(files: &[File]) -> Vec<Download> {
    files.iter().map(|(n, b)| Download::of(n, b)).collect()
}

/// The plates of a plan, made.
pub struct PlateFiles {
    pub format: Format,
    pub layout: Layout,
    /// The files; none when a piece failed the gate (it is never written).
    pub files: Vec<File>,
    /// Each failing piece's reasons, `tile: reason`.
    pub failures: Vec<String>,
    /// The printer's profile the pieces were built for, as its label.
    pub machine: String,
}

/// A Bambu project's missing template, said by name.
fn no_template(why: &str) -> Diag {
    Diag::plain(dt_lang::codes::PLATES, why.to_owned())
}

/// Build every piece of `plan` in full (calling `step` before each with its number, the count and its name), lay the
/// plates out by what they cover and make the files: [`Builds`] over the host's own build of a piece.
///
/// # Errors
/// The plan's printer or a piece that does not build, a piece that does not fit, a Bambu project with no template.
pub fn plate_files(
    host: &impl PlatesHost,
    plan: &Plan,
    document: &str,
    step: &mut dyn FnMut(usize, usize, &str),
) -> Result<PlateFiles, Vec<Diag>> {
    let s = &plan.settings;
    let format = Format::for_printer(&s.machine);
    // before anything builds
    let template = if format == Format::Bambu {
        match host.bambu_template(&s.machine) {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                return Err(vec![no_template(
                    "a Bambu Studio project needs a template for the printer's settings",
                ).help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder")]);
            }
            Err(e) => return Err(vec![no_template(&e)]),
        }
    } else {
        None
    };
    let profiles = host.profiles();
    let builds = Builds::of(
        plan,
        &host.options(),
        &*profiles,
        step,
        &mut |path, opts| host.build_piece(path, opts),
    )?;
    let layout = builds.layout(plan)?;
    let failures = failing(plan, &builds);
    let files = builds
        .files(
            plan,
            &layout,
            format,
            &host.palette(),
            template.as_deref(),
            document,
        )
        .map_err(|e| vec![e])?
        .unwrap_or_default();
    Ok(PlateFiles {
        format,
        layout,
        files,
        failures,
        machine: s.machine.label(),
    })
}

/// The pieces that fail the gate, each reason as `tile: reason`.
fn failing(plan: &Plan, builds: &Builds) -> Vec<String> {
    builds
        .gate()
        .iter()
        .enumerate()
        .flat_map(|(k, f)| {
            let name = plan.kinds[k].name.clone();
            f.iter()
                .map(move |l| format!("{name}: {l}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `platesExport` where the files are downloads: the list's plates, made from its text as the host holds it.
///
/// # Errors
/// [`Refusal`]: there is no such list, its mistakes, or a piece that fails the gate (the plates wait for it).
pub fn export_plates(
    host: &impl PlatesHost,
    v: &PlatesExportReq,
) -> Result<PlatesExported, Refusal> {
    let path = host
        .path_of(&v.list)
        .filter(|p| crate::list_calls::is_plates(p))
        .ok_or_else(|| Refusal::refused(Status::NotFound, "no such print list"))?;
    let t0 = host.seconds();
    let (text, _) = host
        .docs()
        .read(&path)
        .map_err(|e| Refusal::failure(Status::Internal, format!("cannot read {path}: {e}")))?;
    let (_, plan) = plan_from(host, &path, &text, &Overrides::default())
        .map_err(|u| Refusal::of_mistakes(&u.1))?;
    let made =
        plate_files(host, &plan, &path, &mut |_, _, _| {}).map_err(|e| Refusal::of_mistakes(&e))?;
    Ok(PlatesExported {
        files: Vec::new(),
        downloads: downloads(&made.files),
        format: made.format.word().to_owned(),
        failures: made.failures,
        plates: made.layout.plates.len(),
        total_s: host.seconds() - t0,
        out: "downloads".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_examples_and_reads_back() {
        for (plain, coded) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain), coded);
            assert_eq!(unbase64(coded).expect("reads"), plain);
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(unbase64(&base64(&all)).expect("reads"), all);
        assert!(unbase64("a").is_err() && unbase64("!!!!").is_err());
    }

    #[test]
    fn a_download_knows_its_media_type() {
        assert_eq!(Download::of("a.stl", b"x").media_type, "model/stl");
        assert_eq!(Download::of("a.3mf", b"x").media_type, "model/3mf");
        assert_eq!(
            Download::of("a.build.json", b"x").media_type,
            "application/json"
        );
    }
}
