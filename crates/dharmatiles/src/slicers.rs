//! Printer profiles from installed slicers: `dharmatiles printers import`.
//!
//! A slicer's machine profiles say the bed, the printable height, the nozzle,
//! the zones nothing may print on and how many extruders (or MMU filaments)
//! there are.  This reads them (Bambu Studio and OrcaSlicer: JSON with
//! `inherits`; PrusaSlicer: the vendor `.ini` bundles; Cura: `.def.json`),
//! follows each profile's `inherits` chain, and writes only what dharmatiles
//! uses into the user's own `printers/` folder.  Nothing is read from a
//! slicer's user settings except machine profiles, nothing is written to a
//! slicer's folders, and nothing is added to the library: a slicer's profile
//! files carry the slicer's licence (see `docs/printers.html`).
//!
//! Nothing is swallowed: a profile that cannot be read is in the scan's
//! `problems`, said to the user beside what was found.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A slicer this reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slicer {
    Bambu,
    Orca,
    Prusa,
    Cura,
}

impl Slicer {
    pub(crate) const ALL: [Self; 4] = [Self::Bambu, Self::Orca, Self::Prusa, Self::Cura];

    /// As the command line and the page write it.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Bambu => "bambu",
            Self::Orca => "orca",
            Self::Prusa => "prusa",
            Self::Cura => "cura",
        }
    }

    /// The slicer's name.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Bambu => "Bambu Studio",
            Self::Orca => "OrcaSlicer",
            Self::Prusa => "PrusaSlicer",
            Self::Cura => "UltiMaker Cura",
        }
    }

    pub(crate) fn from_word(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == w)
    }
}

/// A slicer found on this computer.
#[derive(Clone, Debug)]
pub(crate) struct Install {
    pub(crate) slicer: Slicer,
    /// `2.9.1`, or empty when it cannot be told.
    pub(crate) version: String,
    /// Where its machine profiles are, most trusted first (the app bundle's
    /// and the folder the app updates).
    pub(crate) roots: Vec<PathBuf>,
}

impl Install {
    /// `Bambu Studio 2.9.1`.
    pub(crate) fn label(&self) -> String {
        if self.version.is_empty() {
            self.slicer.title().to_owned()
        } else {
            format!("{} {}", self.slicer.title(), self.version)
        }
    }
}

/// One printer model a slicer knows, as dharmatiles reads it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Candidate {
    /// Which slicer profile it is (the slicer's own name for it).
    pub(crate) profile: String,
    pub(crate) brand: String,
    pub(crate) model: String,
    /// Width by depth, mm.
    pub(crate) bed: [f64; 2],
    pub(crate) height: Option<f64>,
    pub(crate) nozzle: Option<f64>,
    /// Extruders, or MMU filaments: the filaments loaded at once.
    pub(crate) slots: u32,
    /// From the bed's front-left corner, mm.
    pub(crate) excluded: Vec<[f64; 4]>,
}

/// What reading a slicer found.
#[derive(Clone, Debug)]
pub(crate) struct Scan {
    pub(crate) install: Install,
    pub(crate) candidates: Vec<Candidate>,
    /// Profiles that could not be read or made sense of, and why.
    pub(crate) problems: Vec<String>,
}

// ---- finding the slicers ------------------------------------------------------------

fn plist_version(app: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(app.join("Contents/Info.plist")) else {
        // an app with no readable Info.plist is still there; only its version is unknown
        return String::new();
    };
    text.split("<key>CFBundleShortVersionString</key>")
        .nth(1)
        .and_then(|r| r.split("<string>").nth(1))
        .and_then(|r| r.split("</string>").next())
        .map(|v| {
            // 02.09.01.70 -> 2.9.1
            if v.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit())) {
                v.split('.')
                    .take(3)
                    .map(|p| p.trim_start_matches('0'))
                    .map(|p| if p.is_empty() { "0" } else { p })
                    .collect::<Vec<_>>()
                    .join(".")
            } else {
                v.to_owned()
            }
        })
        .unwrap_or_default()
}

/// The slicers installed for the user whose home is `home` (and whose
/// applications are in `apps`).
pub(crate) fn installs(home: &Path, apps: &Path) -> Vec<Install> {
    let support = home.join("Library/Application Support");
    let config = home.join(".config");
    let mut out = Vec::new();
    let mut add = |slicer, app: &str, bundle_rel: &str, extra: Vec<PathBuf>| {
        let appdir = apps.join(app);
        let mut roots: Vec<PathBuf> = extra.into_iter().filter(|p| p.is_dir()).collect();
        let bundle = appdir.join(bundle_rel);
        if bundle.is_dir() {
            roots.push(bundle);
        }
        if !roots.is_empty() {
            out.push(Install {
                slicer,
                version: plist_version(&appdir),
                roots,
            });
        }
    };
    add(
        Slicer::Bambu,
        "BambuStudio.app",
        "Contents/Resources/profiles",
        vec![
            support.join("BambuStudio/system"),
            config.join("BambuStudio/system"),
        ],
    );
    add(
        Slicer::Orca,
        "OrcaSlicer.app",
        "Contents/Resources/profiles",
        vec![
            support.join("OrcaSlicer/system"),
            config.join("OrcaSlicer/system"),
        ],
    );
    add(
        Slicer::Prusa,
        "PrusaSlicer.app",
        "Contents/Resources/profiles",
        vec![
            support.join("PrusaSlicer/vendor"),
            config.join("PrusaSlicer/vendor"),
        ],
    );
    add(
        Slicer::Cura,
        "UltiMaker Cura.app",
        "Contents/Resources/share/cura/resources/definitions",
        vec![PathBuf::from("/usr/share/cura/resources/definitions")],
    );
    // PrusaSlicer has no version in a plist when only its settings folder is here: its
    // settings file says which release wrote it
    for i in &mut out {
        if i.slicer == Slicer::Prusa && i.version.is_empty() {
            i.version = prusa_version(&support.join("PrusaSlicer/PrusaSlicer.ini"));
        }
    }
    out
}

fn prusa_version(ini: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(ini) else {
        // no settings file: the version is simply not known
        return String::new();
    };
    text.lines()
        .next()
        .and_then(|l| l.split("generated by PrusaSlicer ").nth(1))
        .and_then(|r| r.split([' ', '+']).next())
        .unwrap_or_default()
        .to_owned()
}

/// An install made from a folder the user names.
pub(crate) fn install_at(slicer: Slicer, path: &Path) -> Install {
    Install {
        slicer,
        version: String::new(),
        roots: vec![path.to_path_buf()],
    }
}

// ---- reading ---------------------------------------------------------------------------

/// Read every machine profile of an install.
pub(crate) fn scan(install: &Install) -> Scan {
    let mut problems = Vec::new();
    let mut found = match install.slicer {
        Slicer::Bambu | Slicer::Orca => json_slicer(install, &mut problems),
        Slicer::Prusa => prusa(install, &mut problems),
        Slicer::Cura => cura(install, &mut problems),
    };
    found.sort_by(|a, b| {
        (a.brand.as_str(), a.model.as_str()).cmp(&(b.brand.as_str(), b.model.as_str()))
    });
    Scan {
        install: install.clone(),
        candidates: found,
        problems,
    }
}

fn num(s: &str) -> Option<f64> {
    s.trim().trim_end_matches('%').parse::<f64>().ok()
}

fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(a) => a.first().and_then(as_str),
        _ => None,
    }
}

/// `"256x0"` or `"256,0"` points of a bed.
fn point(s: &str) -> Option<[f64; 2]> {
    let (x, y) = s.split_once(['x', ','])?;
    Some([num(x)?, num(y)?])
}

fn bbox(points: &[[f64; 2]]) -> Option<[f64; 4]> {
    if points.is_empty() {
        return None;
    }
    Some(
        points
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| {
                [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ]
            }),
    )
}

/// The bed from its outline: width by depth, and where its corner is.  A bed
/// that is not a rectangle (a delta's circle) has no place here.
fn bed_of(points: &[[f64; 2]]) -> Result<([f64; 2], [f64; 2]), String> {
    let b = bbox(points).ok_or("no bed shape")?;
    let (w, d) = (b[2] - b[0], b[3] - b[1]);
    if w <= 0.0 || d <= 0.0 {
        return Err("the bed has no area".into());
    }
    // the outline's area (shoelace) against its box's
    let mut a = 0.0;
    for i in 0..points.len() {
        let (p, q) = (points[i], points[(i + 1) % points.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    if (a.abs() / 2.0 - w * d).abs() > 0.01 * w * d {
        return Err("the bed is not a rectangle (a round or cut-corner bed)".into());
    }
    Ok(([w, d], [b[0], b[1]]))
}

/// Excluded zones from a flat list of corners: each four corners are a
/// rectangle when the list is a multiple of four, else one box round all.
fn zones_of(points: &[[f64; 2]], corner: [f64; 2], bed: [f64; 2]) -> Vec<[f64; 4]> {
    let boxes: Vec<[f64; 4]> = if !points.is_empty() && points.len().is_multiple_of(4) {
        points.chunks(4).filter_map(bbox).collect()
    } else {
        bbox(points).into_iter().collect()
    };
    boxes
        .into_iter()
        .map(|b| {
            [
                (b[0] - corner[0]).clamp(0.0, bed[0]),
                (b[1] - corner[1]).clamp(0.0, bed[1]),
                (b[2] - corner[0]).clamp(0.0, bed[0]),
                (b[3] - corner[1]).clamp(0.0, bed[1]),
            ]
        })
        .filter(|z| z[2] - z[0] > 0.0 && z[3] - z[1] > 0.0)
        .collect()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A model's name without the maker's: `Bambu Lab X1 Carbon` -> `X1 Carbon`.
fn without_brand(model: &str, brand: &str) -> String {
    let m = model.trim();
    if m.len() > brand.len()
        && m[..brand.len()].eq_ignore_ascii_case(brand)
        && m[brand.len()..].starts_with(' ')
    {
        m[brand.len()..].trim().to_owned()
    } else {
        m.to_owned()
    }
}

fn nozzle_free(name: &str) -> String {
    let n = name.trim();
    let n = n.strip_suffix(" nozzle").unwrap_or(n);
    // a trailing `0.4`
    match n.rsplit_once(' ') {
        Some((head, tail)) if tail.parse::<f64>().is_ok() && tail.contains('.') => {
            head.trim().to_owned()
        }
        _ => n.to_owned(),
    }
}

/// One candidate per model: the 0.4 mm nozzle's profile, else the first.
fn one_per_model(mut all: Vec<(String, Candidate)>) -> Vec<Candidate> {
    all.sort_by(|a, b| {
        let pref = |c: &Candidate| {
            (
                c.nozzle.is_none_or(|n| (n - 0.4).abs() > 1e-6),
                c.profile.clone(),
            )
        };
        a.0.cmp(&b.0).then(pref(&a.1).cmp(&pref(&b.1)))
    });
    all.dedup_by(|b, a| a.0 == b.0);
    all.into_iter().map(|(_, c)| c).collect()
}

// -- Bambu Studio and OrcaSlicer

type Chain<'a> = Vec<&'a Value>;

fn chain_of<'a>(
    name: &str,
    files: &'a HashMap<String, Value>,
    problems: &mut Vec<String>,
) -> Chain<'a> {
    let mut out = Vec::new();
    let mut at = name.to_owned();
    for _ in 0..10 {
        let Some(v) = files.get(&at) else {
            problems.push(format!(
                "{name}: inherits `{at}`, which the slicer does not have"
            ));
            return out;
        };
        out.push(v);
        match v
            .get("inherits")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(p) => at = p.to_owned(),
            None => return out,
        }
    }
    problems.push(format!(
        "{name}: its `inherits` chain is longer than 10 or loops"
    ));
    out
}

fn first<'a>(chain: &Chain<'a>, key: &str) -> Option<&'a Value> {
    chain.iter().find_map(|v| v.get(key))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn vendor_brand(vendor: &str) -> String {
    match vendor {
        "BBL" => "Bambu Lab".to_owned(),
        other => other.to_owned(),
    }
}

fn json_slicer(install: &Install, problems: &mut Vec<String>) -> Vec<Candidate> {
    // a vendor's folder, from the first root that has it
    let mut vendors: BTreeMap<String, PathBuf> = BTreeMap::new();
    for root in &install.roots {
        let rd = match std::fs::read_dir(root) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: {e}", root.display()));
                continue;
            }
        };
        for e in rd.flatten() {
            let p = e.path().join("machine");
            if p.is_dir()
                && let Some(n) = e.file_name().to_str()
            {
                vendors.entry(n.to_owned()).or_insert(p);
            }
        }
    }
    let mut all = Vec::new();
    for (vendor, dir) in vendors {
        if vendor == "Custom" {
            // the slicer's own placeholders, not a maker's machines
            continue;
        }
        let mut files: HashMap<String, Value> = HashMap::new();
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
            {
                Ok(v) => {
                    if let Some(n) = v.get("name").and_then(Value::as_str) {
                        files.insert(n.to_owned(), v);
                    }
                }
                Err(e) => problems.push(format!("{}: {e}", p.display())),
            }
        }
        let brand = vendor_brand(&vendor);
        let mut names: Vec<&String> = files.keys().collect();
        names.sort();
        for name in names {
            let v = &files[name];
            if v.get("type").and_then(Value::as_str) != Some("machine")
                || v.get("instantiation").and_then(Value::as_str) != Some("true")
            {
                continue;
            }
            let mut local = Vec::new();
            let chain = chain_of(name, &files, &mut local);
            problems.extend(local);
            match json_candidate(name, &brand, &chain) {
                Ok(Some((key, c))) => all.push((format!("{vendor}/{key}"), c)),
                Ok(None) => {}
                Err(why) => problems.push(format!("{name}: {why}")),
            }
        }
    }
    one_per_model(all)
}

fn json_candidate(
    name: &str,
    brand: &str,
    chain: &Chain,
) -> Result<Option<(String, Candidate)>, String> {
    let area: Vec<[f64; 2]> = strings(first(chain, "printable_area"))
        .iter()
        .filter_map(|s| point(s))
        .collect();
    if area.is_empty() {
        return Ok(None); // an abstract base with no bed
    }
    let (bed, corner) = bed_of(&area)?;
    let excl: Vec<[f64; 2]> = strings(first(chain, "bed_exclude_area"))
        .iter()
        .filter_map(|s| point(s))
        .collect();
    let nozzles = match first(chain, "nozzle_diameter") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(as_str)
            .filter_map(|s| num(&s))
            .collect(),
        Some(v) => as_str(v).and_then(|s| num(&s)).into_iter().collect(),
        None => Vec::new(),
    };
    let model_raw = first(chain, "printer_model")
        .and_then(as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| nozzle_free(name));
    Ok(Some((
        model_raw.clone(),
        Candidate {
            profile: name.to_owned(),
            brand: brand.to_owned(),
            model: without_brand(&model_raw, brand),
            bed: [round2(bed[0]), round2(bed[1])],
            height: first(chain, "printable_height")
                .and_then(as_str)
                .and_then(|s| num(&s))
                .map(round2),
            nozzle: nozzles.first().copied(),
            slots: u32::try_from(nozzles.len().max(1)).unwrap_or(1),
            excluded: zones_of(&excl, corner, bed)
                .into_iter()
                .map(|z| z.map(round2))
                .collect(),
        },
    )))
}

// -- PrusaSlicer

type Section = BTreeMap<String, String>;

fn parse_ini(text: &str) -> BTreeMap<String, Section> {
    let mut out: BTreeMap<String, Section> = BTreeMap::new();
    let mut at: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if let Some(h) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            at = Some(h.to_owned());
            out.entry(h.to_owned()).or_default();
        } else if let (Some(s), Some((k, v))) = (&at, l.split_once('=')) {
            out.entry(s.clone())
                .or_default()
                .insert(k.trim().to_owned(), v.trim().to_owned());
        }
    }
    out
}

/// A section with its `inherits` merged in: parents in the order written,
/// later ones over earlier, the section over all.
fn merged(
    name: &str,
    sections: &BTreeMap<String, Section>,
    depth: u8,
    problems: &mut Vec<String>,
) -> Section {
    let key = format!("printer:{name}");
    let Some(s) = sections.get(&key) else {
        problems.push(format!("{name}: no such printer section"));
        return Section::new();
    };
    let mut out = Section::new();
    if depth < 8
        && let Some(p) = s.get("inherits")
    {
        for parent in p.split(';').map(str::trim).filter(|x| !x.is_empty()) {
            out.extend(merged(parent, sections, depth + 1, problems));
        }
    } else if depth >= 8 {
        problems.push(format!("{name}: its `inherits` chain loops"));
    }
    out.extend(s.iter().map(|(k, v)| (k.clone(), v.clone())));
    out
}

fn prusa(install: &Install, problems: &mut Vec<String>) -> Vec<Candidate> {
    let mut all = Vec::new();
    for root in &install.roots {
        let rd = match std::fs::read_dir(root) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: {e}", root.display()));
                continue;
            }
        };
        let mut inis: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ini"))
            .collect();
        inis.sort();
        for p in inis {
            let text = match std::fs::read_to_string(&p) {
                Ok(t) => t,
                Err(e) => {
                    problems.push(format!("{}: {e}", p.display()));
                    continue;
                }
            };
            let sections = parse_ini(&text);
            let vendor = sections
                .get("vendor")
                .and_then(|s| s.get("name"))
                .cloned()
                .unwrap_or_else(|| {
                    p.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_owned()
                });
            if vendor == "Templates" {
                continue;
            }
            for key in sections.keys() {
                let Some(name) = key.strip_prefix("printer:") else {
                    continue;
                };
                if name.starts_with('*') {
                    continue;
                }
                let m = merged(name, &sections, 0, problems);
                if m.get("printer_technology").is_some_and(|t| t == "SLA") {
                    continue;
                }
                let Some(shape) = m.get("bed_shape") else {
                    problems.push(format!("{name}: no bed_shape"));
                    continue;
                };
                let pts: Vec<[f64; 2]> = shape.split(',').filter_map(point).collect();
                let (bed, _) = match bed_of(&pts) {
                    Ok(b) => b,
                    Err(e) => {
                        problems.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                let nozzles: Vec<f64> = m
                    .get("nozzle_diameter")
                    .map(|s| s.split(',').filter_map(num).collect())
                    .unwrap_or_default();
                let model_raw = nozzle_free(name);
                let model = without_brand(&model_raw, "Original Prusa");
                let model = without_brand(&model, &vendor);
                all.push((
                    format!("{vendor}/{model_raw}"),
                    Candidate {
                        profile: name.to_owned(),
                        brand: vendor.clone(),
                        model,
                        bed: [round2(bed[0]), round2(bed[1])],
                        height: m.get("max_print_height").and_then(|s| num(s)).map(round2),
                        nozzle: nozzles.first().copied(),
                        slots: u32::try_from(nozzles.len().max(1)).unwrap_or(1),
                        excluded: Vec::new(),
                    },
                ));
            }
        }
    }
    one_per_model(all)
}

// -- Cura

fn cura(install: &Install, problems: &mut Vec<String>) -> Vec<Candidate> {
    let mut defs: HashMap<String, Value> = HashMap::new();
    for root in &install.roots {
        let rd = match std::fs::read_dir(root) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: {e}", root.display()));
                continue;
            }
        };
        for e in rd.flatten() {
            let p = e.path();
            let Some(id) = p
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".def.json"))
            else {
                continue;
            };
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
            {
                Ok(v) => {
                    defs.entry(id.to_owned()).or_insert(v);
                }
                Err(e) => problems.push(format!("{}: {e}", p.display())),
            }
        }
    }
    let value = |chain: &[&Value], key: &str| -> Option<Value> {
        chain.iter().find_map(|d| {
            d.pointer(&format!("/overrides/{key}/default_value"))
                .or_else(|| d.pointer(&format!("/overrides/{key}/value")))
                .or_else(|| {
                    d.pointer(&format!(
                        "/settings/machine_settings/children/{key}/default_value"
                    ))
                })
                .cloned()
        })
    };
    let mut ids: Vec<&String> = defs.keys().collect();
    ids.sort();
    let mut all = Vec::new();
    for id in ids {
        let d = &defs[id];
        if d.pointer("/metadata/visible") != Some(&Value::Bool(true)) {
            continue;
        }
        let mut chain: Vec<&Value> = vec![d];
        let mut at = d;
        for _ in 0..10 {
            let Some(p) = at.get("inherits").and_then(Value::as_str) else {
                break;
            };
            match defs.get(p) {
                Some(v) => {
                    chain.push(v);
                    at = v;
                }
                None => {
                    problems.push(format!("{id}: inherits `{p}`, which Cura does not have"));
                    break;
                }
            }
        }
        let n = |k: &str| value(&chain, k).and_then(|v| v.as_f64());
        let (Some(w), Some(dp)) = (n("machine_width"), n("machine_depth")) else {
            continue;
        };
        let name = d
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_owned();
        let brand = d
            .pointer("/metadata/manufacturer")
            .and_then(Value::as_str)
            .filter(|b| !b.is_empty() && *b != "Unknown")
            .map_or_else(
                || name.split(' ').next().unwrap_or_default().to_owned(),
                str::to_owned,
            );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // clamped to 1..=32 just above
        let extruders =
            n("machine_extruder_count").map_or(1, |x| x.clamp(1.0, 32.0).round() as u32);
        all.push((
            format!("{brand}/{name}"),
            Candidate {
                profile: name.clone(),
                model: without_brand(&name, &brand),
                brand,
                bed: [round2(w), round2(dp)],
                height: n("machine_height").map(round2),
                nozzle: n("machine_nozzle_size"),
                slots: extruders,
                excluded: Vec::new(),
            },
        ));
    }
    one_per_model(all)
}

// ---- writing ---------------------------------------------------------------------------

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The profile name an import gives a candidate: `brand/model`.
pub(crate) fn name_of(c: &Candidate) -> String {
    format!("{}/{}", slug(&c.brand), slug(&c.model))
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn mm(v: f64) -> String {
    format!("{}mm", round2(v))
}

/// The `.dtile` of a candidate, with its `from-slicer` mark.
pub(crate) fn render(c: &Candidate, install: &Install, name: &str) -> String {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let source = format!("{}: {}", install.label(), c.profile);
    let mut t = String::from("dharmatiles version=\"1.0.0\"\n\n");
    let _ = writeln!(
        t,
        "// Imported by dharmatiles from {source}.\n// Only what dharmatiles uses was kept. Check it against your printer; edit freely.\n"
    );
    let _ = writeln!(t, "printer {stem} {{");
    let _ = writeln!(t, "    brand   {}", q(&c.brand));
    let _ = writeln!(t, "    model   {}", q(&c.model));
    let _ = writeln!(
        t,
        "    bed     {}",
        q(&format!(
            "{}x{}",
            mm(c.bed[0]).trim_end_matches("mm"),
            mm(c.bed[1])
        ))
    );
    if let Some(h) = c.height {
        let _ = writeln!(t, "    height  {}", q(&mm(h)));
    }
    if let Some(n) = c.nozzle {
        let _ = writeln!(t, "    nozzle  {}", q(&mm(n)));
    }
    let _ = writeln!(t, "    slots   {}", c.slots.min(32));
    if !c.excluded.is_empty() {
        let _ = writeln!(t, "    excluded {{");
        for z in &c.excluded {
            let _ = writeln!(
                t,
                "        zone {}",
                q(&format!(
                    "({}, {}) ({}, {})",
                    mm(z[0]),
                    mm(z[1]),
                    mm(z[2]),
                    mm(z[3])
                ))
            );
        }
        let _ = writeln!(t, "    }}");
    }
    let _ = writeln!(t, "    from-slicer {}", q(&source));
    t.push_str("}\n");
    t
}

/// The `-ams` profile that goes with a Bambu printer: the slicers' machine
/// profiles do not say whether an AMS is attached, so it is a separate
/// profile with four filaments a unit, as the library has.
fn render_ams(c: &Candidate, install: &Install, base: &str, name: &str) -> String {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let source = format!("{}: {}", install.label(), c.profile);
    let mut t = String::from("dharmatiles version=\"1.0.0\"\n\n");
    let _ = writeln!(
        t,
        "// {} + AMS: the same machine as `{base}` with an AMS: 4 filament slots a unit, one unit a nozzle.\n// The slicer's machine profile does not say whether an AMS is attached, so this is a profile of its own.\n",
        c.model
    );
    let _ = writeln!(t, "printer {stem} {{");
    let _ = writeln!(t, "    from    {}", q(base));
    let _ = writeln!(t, "    model   {}", q(&format!("{} + AMS", c.model)));
    let _ = writeln!(t, "    slots   {}", (c.slots * 4).min(32));
    let _ = writeln!(t, "    from-slicer {}", q(&source));
    t.push_str("}\n");
    t
}

/// What happened to one profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Written,
    /// The profile was already in your folder: left as it was.
    Kept,
}

#[derive(Clone, Debug)]
pub(crate) struct Written {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) outcome: Outcome,
}

/// Write the profiles into `<home>/printers/`.  `taken` says which names the
/// library already uses (a profile of yours of the same name would replace
/// the library's, so an import takes a name of its own: `-imported`).
///
/// # Errors
/// A folder or a file that cannot be written.
pub(crate) fn write(
    chosen: &[&Candidate],
    install: &Install,
    home: &Path,
    library_has: &dyn Fn(&str) -> bool,
    force: bool,
) -> Result<Vec<Written>, String> {
    let mut out = Vec::new();
    for c in chosen {
        let mut name = name_of(c);
        if library_has(&name) {
            name.push_str("-imported");
        }
        let mut files = vec![(name.clone(), render(c, install, &name))];
        if c.brand.eq_ignore_ascii_case("bambu lab") {
            let ams = format!("{name}-ams");
            files.push((ams.clone(), render_ams(c, install, &name, &ams)));
        }
        for (n, text) in files {
            let path = home.join("printers").join(format!("{n}.dtile"));
            if path.exists() && !force {
                out.push(Written {
                    name: n,
                    path,
                    outcome: Outcome::Kept,
                });
                continue;
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            out.push(Written {
                name: n,
                path,
                outcome: Outcome::Written,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
