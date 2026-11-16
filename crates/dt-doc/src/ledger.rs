//! The ledger: `ledger.kdl` at the repository root lists every known violation of the language principles
//! (`~/.claude/skills/language-design/principles.md`). Tests read their known exceptions from it by id; a change
//! can only shrink it, except an audit commit that records what it found.
//!
//! ```kdl
//! violation "audit:layer-unit" principle="one-job" found="94927dcd" note="the unit word `layer` reserves a kind word"
//! ```
//!
//! The file is read here, on [`Ledger`], and nowhere else. [`Ledger::expect`] is the shrink rule in one function:
//! a listed case that now works is a failure (remove it), and a broken case that is not listed is a failure (it is
//! new).

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use kdl::{KdlDocument, KdlNode};
use serde::Serialize;

/// The ledger's file name, at the repository root.
pub const FILE: &str = "ledger.kdl";

/// The one node kind of a ledger.
pub const NODE: &str = "violation";

/// The principles a violation may name: the closed list.
pub const PRINCIPLES: [&str; 13] = [
    "abstraction",
    "correspondence",
    "completeness",
    "orthogonality",
    "closure",
    "first-class",
    "compositionality",
    "kernel",
    "one-way",
    "one-job",
    "zero-one-infinity",
    "syntax-shows-type",
    "check-equals-build",
];

/// One known violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// The key a test looks up: `hole:<kind>:<form>`, `collision:<word>`, `disagree:<case>`, `audit:<slug>`.
    pub id: String,
    /// One of [`PRINCIPLES`].
    pub principle: String,
    /// The commit the finding was recorded against.
    pub found: String,
    /// What is wrong, in a line.
    pub note: String,
}

/// Every known violation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Ledger {
    pub violations: Vec<Violation>,
}

/// A ledger that cannot be read, or a case the ledger disagrees with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    /// The file cannot be read.
    Unreadable { path: String, why: String },
    /// The text is not KDL v2.
    NotKdl(String),
    /// A node that is not `violation`, or a violation that is not `violation "<id>" principle= found= note=`.
    BadNode { line: usize, why: String },
    /// Two violations with one id.
    DuplicateId(String),
    /// A principle outside [`PRINCIPLES`].
    UnknownPrinciple { id: String, principle: String },
    /// A case and the ledger disagree ([`Ledger::expect`]).
    Disagrees(String),
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, why } => write!(f, "{path}: {why}"),
            Self::NotKdl(why) => write!(f, "{FILE} is not KDL v2: {why}"),
            Self::BadNode { line, why } => write!(f, "{FILE}, line {line}: {why}"),
            Self::DuplicateId(id) => write!(f, "{FILE} lists `{id}` twice"),
            Self::UnknownPrinciple { id, principle } => write!(
                f,
                "{FILE}: `{id}` names the principle `{principle}`, which is none of: {}",
                PRINCIPLES.join(", ")
            ),
            Self::Disagrees(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for LedgerError {}

impl Ledger {
    /// The ledger at `repo_root/ledger.kdl`.
    ///
    /// # Errors
    /// The file cannot be read, or is not a ledger.
    pub fn load(repo_root: &Path) -> Result<Self, LedgerError> {
        let path = repo_root.join(FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| LedgerError::Unreadable {
            path: path.display().to_string(),
            why: e.to_string(),
        })?;
        Self::parse(&text)
    }

    /// A ledger from its text.
    ///
    /// # Errors
    /// Not KDL v2, a node that is not a well-made `violation`, a repeated id or an unknown principle.
    pub fn parse(text: &str) -> Result<Self, LedgerError> {
        let doc = KdlDocument::parse(text).map_err(|e| LedgerError::NotKdl(e.to_string()))?;
        let mut violations: Vec<Violation> = Vec::new();
        for n in doc.nodes() {
            let v = violation(text, n)?;
            if violations.iter().any(|o| o.id == v.id) {
                return Err(LedgerError::DuplicateId(v.id));
            }
            violations.push(v);
        }
        Ok(Self { violations })
    }

    /// Whether `id` is listed.
    #[must_use]
    pub fn known(&self, id: &str) -> bool {
        self.violations.iter().any(|v| v.id == id)
    }

    /// Every listed id.
    #[must_use]
    pub fn ids(&self) -> BTreeSet<&str> {
        self.violations.iter().map(|v| v.id.as_str()).collect()
    }

    /// The shrink rule: a case `id` whose `still_broken` agrees with the ledger passes; a listed case that now
    /// works must leave the ledger, and a broken case that is not listed is new.
    ///
    /// # Errors
    /// The ledger and the case disagree, in the words that say what to do.
    pub fn expect(&self, id: &str, still_broken: bool) -> Result<(), LedgerError> {
        match (self.known(id), still_broken) {
            (true, false) => Err(LedgerError::Disagrees(format!(
                "fixed: remove `{id}` from {FILE}"
            ))),
            (false, true) => Err(LedgerError::Disagrees(format!(
                "new violation `{id}`: fix it, or an audit commit records it in {FILE}"
            ))),
            _ => Ok(()),
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

fn violation(text: &str, n: &KdlNode) -> Result<Violation, LedgerError> {
    let line = line_of(text, n.span().offset());
    let bad = |why: String| LedgerError::BadNode { line, why };
    if n.name().value() != NODE {
        return Err(bad(format!(
            "a ledger holds only `{NODE}` nodes, not `{}`",
            n.name().value()
        )));
    }
    if n.children().is_some() {
        return Err(bad("a violation has no children".into()));
    }
    let mut id = None;
    let mut props: [Option<String>; 3] = [None, None, None];
    for e in n.entries() {
        let value = e.value().as_string();
        match e.name() {
            None => {
                let s = value.ok_or_else(|| bad("the id is a quoted string".into()))?;
                if id.replace(s.to_owned()).is_some() {
                    return Err(bad("a violation has one id".into()));
                }
            }
            Some(k) => {
                let slot = match k.value() {
                    "principle" => 0,
                    "found" => 1,
                    "note" => 2,
                    other => {
                        return Err(bad(format!(
                            "unknown property `{other}` (principle, found, note)"
                        )));
                    }
                };
                let s = value.ok_or_else(|| bad(format!("`{}` is a quoted string", k.value())))?;
                if props[slot].replace(s.to_owned()).is_some() {
                    return Err(bad(format!("`{}` is written twice", k.value())));
                }
            }
        }
    }
    let id = id
        .filter(|s| !s.is_empty() && !s.contains(char::is_whitespace))
        .ok_or_else(|| bad("a violation starts with its id, a word with no spaces".into()))?;
    let need = |p: &Option<String>, name: &str| {
        p.clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad(format!("`{id}` has no {name}=")))
    };
    let principle = need(&props[0], "principle")?;
    let found = need(&props[1], "found")?;
    let note = need(&props[2], "note")?;
    if !PRINCIPLES.contains(&principle.as_str()) {
        return Err(LedgerError::UnknownPrinciple { id, principle });
    }
    Ok(Violation {
        id,
        principle,
        found,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "violation \"audit:a\" principle=\"one-job\" found=\"abc\" note=\"n\"\n";

    #[test]
    fn a_ledger_is_read_and_asked_by_id() {
        let l = Ledger::parse(OK).unwrap();
        assert!(l.known("audit:a") && !l.known("audit:b"));
        assert_eq!(Ledger::parse("").unwrap(), Ledger::default());
    }

    #[test]
    fn the_shrink_rule() {
        let l = Ledger::parse(OK).unwrap();
        assert!(l.expect("audit:a", true).is_ok());
        assert!(l.expect("audit:b", false).is_ok());
        let fixed = l.expect("audit:a", false).unwrap_err().to_string();
        assert_eq!(fixed, "fixed: remove `audit:a` from ledger.kdl");
        let new = l.expect("audit:b", true).unwrap_err().to_string();
        assert!(new.starts_with("new violation `audit:b`"), "{new}");
    }

    #[test]
    fn mistakes_are_typed() {
        assert!(matches!(
            Ledger::parse("note \"x\"\n"),
            Err(LedgerError::BadNode { line: 1, .. })
        ));
        assert!(matches!(
            Ledger::parse(&format!("{OK}{OK}")),
            Err(LedgerError::DuplicateId(_))
        ));
        assert!(matches!(
            Ledger::parse(&OK.replace("one-job", "tidy")),
            Err(LedgerError::UnknownPrinciple { .. })
        ));
        assert!(matches!(
            Ledger::parse("violation \"a\" principle=\"one-job\" found=\"x\"\n"),
            Err(LedgerError::BadNode { .. })
        ));
        assert!(matches!(
            Ledger::parse("violation {"),
            Err(LedgerError::NotKdl(_))
        ));
    }

    #[test]
    fn the_repository_ledger_reads() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let l = Ledger::load(&root).unwrap();
        assert!(l.known("audit:three-colour-types"));
    }
}
