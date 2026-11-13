//! What a control is where nothing is written for it.
//!
//! The editor's sheet shows every control of an element, written or not.  An
//! unwritten one shows the value a build would use and where it comes from,
//! so the page never works one out.  Every value has one source:
//!
//! 1. what the element works out for itself, read off the fields it is built
//!    with ([`dt_elements::Element::shown`]: a wall's stone size comes from
//!    its style);
//! 2. else the schema's `default`;
//! 3. else the schema's `absent` words, which say what leaving it out means
//!    ("no strip");
//! 4. a control the build does not take yet says so; a body's `top` is the
//!    band the solver used, which is where a library default is picked.
//!
//! A control that is none of these is a mistake in the schema's table and is
//! an error, never a blank (`tests/unwritten.rs` runs every element).

use dt_doc::schema::{self, Control};

use crate::doc::{Child, Origin, Setting};
use crate::solve::SolvedTop;

/// An unwritten control: its path, what it shows, and where that comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unwritten {
    /// The control's path (`stones/height`).
    pub control: String,
    /// The value as a document writes it, or the `absent` words.
    pub text: String,
    /// What it comes from, after "from" (`style brick`), or empty when
    /// `text` says what leaving it out means.
    pub from: String,
}

fn as_controls(settings: &[Setting]) -> Vec<dt_elements::Control> {
    settings
        .iter()
        .map(|c| dt_elements::Control {
            name: c.name.clone(),
            value: c.value.clone(),
        })
        .collect()
}

/// Every control of element `name` that `controls` do not write (and its
/// sub-controls, a group's own row left out), each with what it shows.
/// `top` is the band a body was solved from (the solver is what picks a
/// library default for it); `None` for a body the solver has not read.
///
/// # Errors
/// The element cannot be read from these controls, or a control has no
/// default, no value its element works out and no `absent` words.
pub fn unwritten(
    name: &str,
    controls: &[Setting],
    children: &[Child],
    top: Option<&SolvedTop>,
) -> Result<Vec<Unwritten>, String> {
    let element = schema::element(name).ok_or_else(|| format!("no element is called `{name}`"))?;
    let kids: Vec<dt_elements::Child> = children
        .iter()
        .map(|c| dt_elements::Child {
            name: c.what.value.clone(),
            controls: as_controls(&c.controls),
            weight: c.weight,
        })
        .collect();
    let worked = dt_elements::shown(name, &as_controls(controls), &kids).map_err(|e| {
        format!(
            "`{name}` cannot say what its unwritten controls are: {}",
            e.message
        )
    })?;
    let body = element.is_body();
    let written = |path: &str| {
        controls
            .iter()
            .any(|c| c.origin == Origin::Written && c.name == path)
    };
    let mut out = Vec::new();
    for c in element.controls {
        if body && c.name == "top" {
            if !written("top") {
                out.push(body_top(top));
            }
        } else {
            visit(name, c, c.name, &written, &worked, &mut out)?;
        }
    }
    Ok(out)
}

/// A body's `top`: the formula its band was solved from, and the library
/// file it is the default of.
fn body_top(top: Option<&SolvedTop>) -> Unwritten {
    let (text, from) = match top {
        Some(SolvedTop {
            text,
            default_from: Some(file),
        }) => (text.clone(), library_name(file)),
        Some(SolvedTop { text, .. }) => (text.clone(), "default".to_owned()),
        None => (
            "worked out once the tile has no mistakes".to_owned(),
            String::new(),
        ),
    };
    Unwritten {
        control: "top".to_owned(),
        text,
        from,
    }
}

/// A file as the editor names it where a value comes from it:
/// `lib:parts/edges.dtile` is `parts/edges`.
#[must_use]
pub fn library_name(path: &str) -> String {
    let p = path.strip_prefix("lib:").unwrap_or(path);
    p.strip_suffix(".dtile").unwrap_or(p).to_owned()
}

fn visit(
    element: &str,
    c: &'static Control,
    path: &str,
    written: &dyn Fn(&str) -> bool,
    worked: &[dt_elements::Shown],
    out: &mut Vec<Unwritten>,
) -> Result<(), String> {
    for s in c.subs {
        visit(
            element,
            s,
            &format!("{path}/{}", s.name),
            written,
            worked,
            out,
        )?;
    }
    if c.is_group() || c.required || written(path) {
        return Ok(());
    }
    let entry = if c.planned {
        Unwritten {
            control: path.to_owned(),
            text: "not built yet".to_owned(),
            from: String::new(),
        }
    } else if let Some(w) = worked.iter().find(|w| w.control == path) {
        Unwritten {
            control: path.to_owned(),
            text: w.text.clone(),
            from: w.from.clone(),
        }
    } else if !c.default.is_empty() {
        Unwritten {
            control: path.to_owned(),
            text: c.default_written(),
            from: "default".to_owned(),
        }
    } else if !c.absent.is_empty() {
        Unwritten {
            control: path.to_owned(),
            text: c.absent.to_owned(),
            from: String::new(),
        }
    } else {
        return Err(format!(
            "`{element}`'s `{path}` has no default, no value the element works out and no `absent` words: give it one in dt-doc's elements table"
        ));
    };
    out.push(entry);
    Ok(())
}
