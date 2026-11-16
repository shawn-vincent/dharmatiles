//! Check equals build, and the closure of kinds over forms.
//!
//! Two invariants, both about the one reading of a document:
//!
//! 1. `check` and `build` never disagree about a document: `check` finds the mistakes `build` would refuse, the
//!    same ones, at the same spans.  A tile `check` accepts with nothing not-yet-built is one `build` accepts.
//! 2. Every kind a document can write (`Kind::WORDS`, the one table) can be put through every form a document has
//!    today (`Form`), or the cell is a refusal the language's own rules call for, or it is a hole in the ledger
//!    (`hole:<kind>:<form>`, in `ledger.kdl`, which only shrinks).
//!
//! The table is derived: its rows are `Kind::WORDS`, its columns `Form::ALL`, and each cell is a whole document
//! made from the form's template and the kind's sample.  The warm test only reads (`check`, and the build of a
//! document that check refuses, which stops at the read).  The build of a document that check accepts is the
//! `thorough_` test's, which `cargo xtask check --full` runs.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use std::collections::BTreeMap;

use dt_doc::ledger::Ledger;
use dt_doc::units::{Kind, misfit};
use dt_eval::{Diag, Memory, Options, check};

use crate::{common, golden, memo};

/// The ways a document can say a value of some kind: the columns of the table.  The guide's forms today:
/// naming (`let`, `region`), a param of the kind, a place word (`at=`), a scatter's child, an `override`
/// and `use` from a part.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    /// Name it: a `let` for a number or a point (a value), a thing or a layer (a body), a `region` for a region.
    Name,
    /// Pass it as a `param amount kind=…` of a definition.
    Param,
    /// Read a param's name in a formula (`amount + 1`) where a number is wanted.
    ParamFormula,
    /// Place it: written as an entry with `at=`.
    Place,
    /// Scatter it: written as a scatter's child.
    ScatterChild,
    /// Override it: `let $n …` and `override $n …`, read where a value goes.
    Override,
    /// Import it: `use "./p" n`, from a part that names it.
    Import,
}

impl Form {
    const ALL: [Form; 7] = [
        Form::Name,
        Form::Param,
        Form::ParamFormula,
        Form::Place,
        Form::ScatterChild,
        Form::Override,
        Form::Import,
    ];

    fn word(self) -> &'static str {
        match self {
            Form::Name => "name",
            Form::Param => "param",
            Form::ParamFormula => "param-formula",
            Form::Place => "place",
            Form::ScatterChild => "scatter-child",
            Form::Override => "override",
            Form::Import => "import",
        }
    }
}

/// What a kind is, written: a value of it, and a stack entry that reads one (`{}` is the value or its name).
struct Sample {
    value: &'static str,
    entry: &'static str,
    /// The entry lays its own ground, so the tile lays none before it.
    ground: bool,
}

fn sample(kind: Kind) -> Sample {
    let (value, entry) = match kind {
        Kind::Length => ("\"3dmm\"", "rock size={}"),
        Kind::Angle => ("\"30deg\"", "dirt top=\"$dirtEdgeHeight\" maxSlope={}"),
        Kind::Plain => ("3", "scatter count={} { rock }"),
        Kind::Region => ("bank", "rock at={}"),
        Kind::Point => ("\"(0.3sq, 0.5sq)\"", "rock at={}"),
        Kind::Thing => ("pebble", "{} at=\"(0.3sq, 0.5sq)\""),
        Kind::Layer => ("grass", "{}"),
        Kind::Zero | Kind::Any => unreachable!("not a kind a document writes: it has no word"),
    };
    Sample {
        value,
        entry,
        ground: kind == Kind::Angle,
    }
}

const HEAD: &str = "dharmatiles version=\"1.0.0\"\n";
const SCENE: &str = "    line shore { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }
    region bank { boundedBy shore; fill \"(0.5sq, 0.75sq)\" }
    region pool { boundedBy shore; fill \"(0.5sq, 0.25sq)\" }
";

fn tile(top: &str, stack: &str, ground: bool) -> String {
    let dirt = if ground {
        ""
    } else {
        "        dirt top=\"$dirtEdgeHeight\"\n"
    };
    format!(
        "tile size=\"1x1\" seed=4 {{\n{SCENE}{top}    stack {{\n        base\n{dirt}        {stack}\n    }}\n}}\n"
    )
}

/// What a kind's name is bound to when a form names it: where the name goes (the tile's top, or a definition).
fn naming(kind: Kind, s: &Sample, name: &str) -> (String, String) {
    match kind {
        Kind::Length | Kind::Angle | Kind::Plain | Kind::Point => {
            (format!("    let {name} {}\n", s.value), String::new())
        }
        Kind::Region => (String::new(), String::new()),
        // a thing is named by a bare body of one thing; a stack is always a layer
        Kind::Thing => (String::new(), format!("let {name} {{ {} }}\n", s.value)),
        Kind::Layer => (
            String::new(),
            format!("let {name} {{ stack {{ {} }} }}\n", s.value),
        ),
        Kind::Zero | Kind::Any => unreachable!(),
    }
}

/// The documents of one cell, by path: `t.dtile` is the tile, and a part is `p.dtile`.
fn cell(kind: Kind, form: Form) -> Vec<(&'static str, String)> {
    let s = sample(kind);
    let entry = |v: &str| s.entry.replace("{}", v);
    match form {
        Form::Name => {
            let (top, define) = naming(kind, &s, "n");
            let name = if kind == Kind::Region { "bank" } else { "n" };
            vec![(
                "t.dtile",
                format!("{HEAD}{define}{}", tile(&top, &entry(name), s.ground)),
            )]
        }
        Form::Param => {
            let call = format!("d amount={}", s.value);
            vec![(
                "t.dtile",
                format!(
                    "{HEAD}let d {{ param amount kind={}; stack {{ {} }} }}\n{}",
                    kind.word(),
                    entry("amount"),
                    tile("", &call, s.ground)
                ),
            )]
        }
        Form::ParamFormula => {
            // the formula reads the param by its name and adds one of its own kind
            let formula = match kind {
                Kind::Length => "\"amount + 1dmm\"",
                Kind::Angle => "\"amount + 1deg\"",
                _ => "\"amount + 1\"",
            };
            let entry = match kind {
                Kind::Length | Kind::Angle | Kind::Plain => entry(formula),
                // a thing, a layer, a region or a point is no number to add to
                _ => format!("scatter count={formula} {{ rock }}"),
            };
            let call = format!("d amount={}", s.value);
            vec![(
                "t.dtile",
                format!(
                    "{HEAD}let d {{ param amount kind={}; stack {{ {} }} }}\n{}",
                    kind.word(),
                    entry,
                    tile("", &call, s.ground)
                ),
            )]
        }
        Form::Place => vec![(
            "t.dtile",
            format!(
                "{HEAD}{}",
                tile("", &format!("{} at=bank", s.value), s.ground)
            ),
        )],
        Form::ScatterChild => vec![(
            "t.dtile",
            format!(
                "{HEAD}{}",
                tile("", &format!("scatter count=2 {{ {} }}", s.value), s.ground)
            ),
        )],
        Form::Override => vec![(
            "t.dtile",
            format!(
                "{HEAD}{}",
                tile(
                    &format!("    let $n 1\n    override $n {}\n", s.value),
                    &entry("\"$n\""),
                    s.ground
                )
            ),
        )],
        Form::Import => {
            let (top, define) = naming(kind, &s, "n");
            // a part holds definitions, so whatever a number's name is written as goes in the part as it is
            let part = format!("{HEAD}{define}{}", top.trim_start());
            vec![
                ("p.dtile", part),
                (
                    "t.dtile",
                    format!(
                        "{HEAD}use \"./p\" n\n{}",
                        tile(
                            "",
                            &entry(if kind == Kind::Region { "bank" } else { "n" }),
                            s.ground
                        )
                    ),
                ),
            ]
        }
    }
}

/// What the language says of a cell.
enum Rule {
    /// The cell is a document that checks and builds.
    Works,
    /// The language refuses it, with this code, and `why` says why.
    Refuses {
        codes: &'static [&'static str],
        why: &'static str,
    },
}

fn rule(kind: Kind, form: Form) -> Rule {
    use Form::*;
    use dt_lang::codes as c;
    let refuses = |codes, why| Rule::Refuses { codes, why };
    match (form, kind) {
        // a thing is placed by `at=`, so is a layer (a placed thing is a layer); nothing else is an entry
        (Place, Kind::Thing | Kind::Layer) => Rule::Works,
        (Place, _) => refuses(
            &[c::UNKNOWN_ELEMENT, c::SYNTAX],
            "only a thing or a layer is written as an entry, and a number, a region or a point is not one",
        ),
        // a scatter wants a thing; a layer never fits where a thing is wanted
        (ScatterChild, Kind::Thing) => Rule::Works,
        (ScatterChild, Kind::Layer) => refuses(
            &[c::SCATTER],
            "a scatter wants a thing and a layer never fits where a thing is wanted (units::fits)",
        ),
        (ScatterChild, _) => refuses(
            &[c::UNKNOWN_ELEMENT, c::SYNTAX],
            "a scatter's child is a thing, and a number, a region or a point is not one",
        ),
        // an `override` rewrites a `$` name, and a `$` name holds a formula: a number
        (Override, Kind::Length | Kind::Angle | Kind::Plain) => Rule::Works,
        (Override, _) => refuses(
            &[
                c::UNKNOWN_NAME,
                c::VALUE,
                c::POINT,
                c::TYPE,
                c::UNKNOWN_ELEMENT,
            ],
            "a `$` name holds a formula, a number; a region, a point, a thing or a layer is not one",
        ),
        // a thing or a layer is no number: a formula that reads it is refused where it is read
        (ParamFormula, Kind::Thing | Kind::Layer) => refuses(
            &[c::TYPE],
            "a param that holds a thing or a layer is no number, so a formula that adds to it is refused at the use",
        ),
        // a param holds a length, an angle, a plain number, a thing or a layer
        (Param | ParamFormula, Kind::Region | Kind::Point) => refuses(
            &[c::KIND],
            "a param holds a length, an angle, a plain number, a thing or a layer (Kind::holdable), so `kind=` names no region or point",
        ),
        _ => Rule::Works,
    }
}

struct Cell {
    kind: Kind,
    form: Form,
    files: Vec<(&'static str, String)>,
}

fn table() -> Vec<Cell> {
    Kind::WORDS
        .iter()
        .flat_map(|(_, kind)| {
            Form::ALL.iter().map(|form| Cell {
                kind: *kind,
                form: *form,
                files: cell(*kind, *form),
            })
        })
        .collect()
}

fn source(files: &[(&'static str, String)]) -> Memory {
    let pairs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    common::memory(&pairs)
}

/// The identity of a diagnostic that `check` and `build` must share: its code and its span.
fn mark(d: &Diag) -> (&'static str, Option<(usize, usize)>) {
    (d.code, d.at.as_ref().map(|(_, s)| (s.offset(), s.len())))
}

/// `check`'s diagnostics: its mistakes and what it sets aside as not built yet.
fn found(src: &Memory) -> Vec<Diag> {
    match check(src, "t.dtile", &Options::default()) {
        Err(e) => e,
        Ok(c) => c.pending().to_vec(),
    }
}

/// Whether a build's error is the geometry's, not the reading's: a failure of the program or the machine, or the
/// one mistake only the shape of the ground can show (`dt::thin`).  `check` plans and builds no geometry, so it
/// cannot see these, and the invariant is about what the reading refuses.
fn geometry(d: &Diag) -> bool {
    d.code == dt_lang::codes::THIN
        || dt_lang::codes::lookup(d.code).is_some_and(|c| c.kind == dt_lang::codes::Kind::Failure)
}

/// What the build did with a document `check` accepted.
enum Outcome {
    /// It was not built (the build is heavy, and this is the warm test).
    Skipped,
    /// It built.
    Built,
    /// The geometry refused it, which `check` cannot see.
    Geometry,
}

/// `check` and `build` of a document name the same mistakes.  A document `check` accepts is built only when
/// `build_ok` says to (the build is heavy); one it refuses is refused by the build at the read, which is cheap.
fn agree(label: &str, src: &Memory, build_ok: bool) -> (Vec<Diag>, Outcome) {
    let seen = found(src);
    if seen.is_empty() && !build_ok {
        return (seen, Outcome::Skipped);
    }
    match memo::build(src, "t.dtile", &Options::default()) {
        Ok(_) => {
            assert!(
                seen.is_empty(),
                "{label}: check refuses what build accepts: {seen:#?}"
            );
            (seen, Outcome::Built)
        }
        Err(errs) if seen.is_empty() && errs.iter().all(geometry) => (seen, Outcome::Geometry),
        Err(errs) => {
            let mut a: Vec<_> = seen.iter().map(mark).collect();
            let mut b: Vec<_> = errs.iter().map(mark).collect();
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "{label}: check and build name different mistakes");
            (seen, Outcome::Skipped)
        }
    }
}

#[test]
fn check_equals_build_on_the_torture_set() {
    // a torture tile builds (the memo panics on one that does not), so `check` must find nothing in it
    for path in golden::TILES {
        memo::built(path);
        let c = match check(&dt_eval::Library, path, &Options::default()) {
            Ok(c) => c,
            Err(e) => panic!("{path}: build accepts it and check refuses: {e:#?}"),
        };
        assert!(
            c.pending().is_empty(),
            "{path}: build accepts it and check sets something aside"
        );
    }
}

#[test]
fn a_param_is_a_name_in_a_formula_in_check_and_in_build() {
    // the part reads `n + 1`: `n` is a plain number (its default's kind), so check and build both accept it
    let part = "let bit { param n 3; stack { scatter count=\"n + 1\" { rock } } }\n";
    let t = format!("{HEAD}{part}{}", tile("", "bit", false));
    let src = common::memory(&[("t.dtile", &t)]);
    let (seen, built) = agree("n + 1", &src, true);
    assert!(seen.is_empty(), "{seen:#?}");
    assert!(matches!(built, Outcome::Built | Outcome::Geometry));
}

/// The `count` of each scatter in a tile, as the reader made it.
fn counts(text: &str) -> Vec<String> {
    let src = common::memory(&[("t.dtile", text)]);
    let checked = check(&src, "t.dtile", &Options::default()).unwrap();
    checked
        .tile
        .unwrap()
        .stack
        .iter()
        .filter(|e| e.what.value == "scatter")
        .flat_map(|e| {
            e.controls
                .iter()
                .filter(|c| c.name == "count")
                .map(|c| format!("{:?}", c.value))
        })
        .collect()
}

#[test]
fn a_param_in_a_formula_means_the_value_the_call_gave_it() {
    let part = "let bit { param n 3; stack { scatter count=\"n + 1\" { rock } } }\n";
    let stack = "        bit\n        bit n=5\n        bit n=\"k * 2\"";
    let text = format!("{HEAD}{part}{}", tile("    let k 2\n", stack, false));
    let by_hand = |n: &str| {
        let text = format!(
            "{HEAD}{}",
            tile("", &format!("scatter count=\"{n}\" {{ rock }}"), false)
        );
        counts(&text).pop().unwrap()
    };
    let got = counts(&text);
    assert_eq!(
        got,
        [by_hand("3 + 1"), by_hand("5 + 1"), by_hand("2 * 2 + 1")]
    );
}

#[test]
fn a_param_is_a_whole_value_and_a_place_and_a_formula() {
    // a whole value, and a name in the same line's formula; the caller's name is a `let`
    let part = "let bit { param n 3; stack { scatter count=n { rock size=\"n * 1dmm\" } } }\n";
    let t = format!("{HEAD}{part}{}", tile("", "bit n=2", false));
    let src = common::memory(&[("t.dtile", &t)]);
    assert!(found(&src).is_empty(), "{:#?}", found(&src));
}

/// A param's kind is its default's, or `kind=`, and never what it is used for: a document in which that would be
/// needed says to write `kind=`, at the param.
#[test]
fn a_param_with_no_kind_is_refused_at_the_param_and_not_read_from_its_use() {
    for param in ["param n", "param n angular", "param n #true"] {
        let t = format!(
            "{HEAD}let bit {{ {param}; stack {{ scatter count=n {{ rock }} }} }}\n{}",
            tile("", "bit n=2", false)
        );
        let src = common::memory(&[("t.dtile", &t)]);
        let seen = found(&src);
        let d = seen
            .iter()
            .find(|d| d.code == dt_lang::codes::KIND)
            .unwrap_or_else(|| panic!("{param}: {seen:#?}"));
        let (s, span) = d.at.as_ref().unwrap();
        assert_eq!(&s.inner()[span.offset()..span.offset() + span.len()], "n");
        assert!(d.help.as_deref().unwrap().contains("kind="), "{d:#?}");
    }
}

#[test]
fn a_param_used_where_its_kind_does_not_fit_is_refused_as_misfit_words_it() {
    // a length where a count is wanted, in a formula and as a whole value, and an argument of the wrong kind
    let cases = [
        // (param line, use, who, have, want)
        ("param w \"3dmm\"", "scatter count=\"w + 1\" { rock }", None),
        (
            "param w \"3dmm\"",
            "scatter count=w { rock }",
            Some(("w", Kind::Length, "`scatter count`", Kind::Plain)),
        ),
        (
            "param n 3",
            "rock size=n",
            Some(("n", Kind::Plain, "`rock size`", Kind::Length)),
        ),
    ];
    for (param, entry, want) in cases {
        let t = format!(
            "{HEAD}let bit {{ {param}; stack {{ {entry} }} }}\n{}",
            tile("", "bit", false)
        );
        let src = common::memory(&[("t.dtile", &t)]);
        let seen = found(&src);
        assert!(
            seen.iter()
                .any(|d| d.code == dt_lang::codes::TYPE || d.code == dt_lang::codes::UNIT),
            "{param} {entry}: {seen:#?}"
        );
        if let Some((name, have, who, wants)) = want {
            let m = misfit(name, have, who, wants);
            assert!(seen.iter().any(|d| d.message == m.message), "{seen:#?}");
        }
    }
    // an argument of the wrong kind, at the call
    let t = format!(
        "{HEAD}let bit {{ param n 3; stack {{ scatter count=n {{ rock }} }} }}\n{}",
        tile("", "bit n=\"4dmm\"", false)
    );
    let src = common::memory(&[("t.dtile", &t)]);
    let seen = found(&src);
    let d = seen
        .iter()
        .find(|d| d.code == dt_lang::codes::TYPE)
        .unwrap_or_else(|| panic!("{seen:#?}"));
    let (s, span) = d.at.as_ref().unwrap();
    assert_eq!(
        &s.inner()[span.offset()..span.offset() + span.len()],
        "\"4dmm\""
    );
}

/// One cell, read.  Returns whether it is broken: it should work and does not (check refuses it, or, when `build_ok`,
/// the build does).
fn judge(c: &Cell, build_ok: bool) -> bool {
    let label = format!("{}:{}", c.kind.word(), c.form.word());
    let src = source(&c.files);
    let (seen, built) = agree(&label, &src, build_ok);
    match rule(c.kind, c.form) {
        Rule::Works => {
            if matches!(built, Outcome::Geometry) {
                return true;
            }
            !seen.is_empty()
        }
        Rule::Refuses { codes, why } => {
            assert!(
                !seen.is_empty(),
                "{label}: should be refused ({why}), and check accepts it:\n{:#?}",
                c.files
            );
            assert!(
                seen.iter().any(|d| codes.contains(&d.code)),
                "{label}: refused, but not with {codes:?} ({why}): {:#?}",
                seen.iter()
                    .map(|d| (d.code, &d.message))
                    .collect::<Vec<_>>()
            );
            false
        }
    }
}

fn read_ledger() -> Ledger {
    Ledger::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .as_path(),
    )
    .unwrap()
}

fn closure(build_ok: bool) {
    let ledger = read_ledger();
    let mut report = BTreeMap::new();
    for c in table() {
        let broken = judge(&c, build_ok);
        let id = format!("hole:{}:{}", c.kind.word(), c.form.word());
        if let Err(e) = ledger.expect(&id, broken) {
            report.insert(id, e.to_string());
        }
    }
    assert!(
        report.is_empty(),
        "the closure table and the ledger disagree: {report:#?}"
    );
}

#[test]
fn every_kind_in_every_form_works_or_is_refused_or_is_a_known_hole() {
    closure(false);
}

#[test]
#[ignore = "thorough: builds every cell of the closure table that check accepts, which a warm test may not; cargo xtask check --full runs it"]
fn thorough_every_cell_that_checks_also_builds() {
    closure(true);
}

#[test]
fn a_refused_kind_reads_as_units_misfit_says() {
    // the scatter of a layer: the one wording of a kind mistake, from `units::misfit`, not a second one
    let c = cell(Kind::Layer, Form::ScatterChild);
    let seen = found(&source(&c));
    let want = misfit("grass", Kind::Layer, "a scatter", Kind::Thing);
    assert!(
        seen.iter().any(|d| d.message == want.message),
        "{:#?}",
        seen.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
