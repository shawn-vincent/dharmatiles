//! The elements' values in the canonical form (`dt_value::Canon`): what a
//! step's key, and a thing's, reads of a control's value and a formula.

use dt_value::{Canon, Canonical};

use crate::{Formula, Value};

impl Canonical for Formula {
    fn canon(&self, c: &mut Canon) {
        let Self { owner, child, key } = self;
        c.tag(b'=').str(owner.as_str()).put(child).put(key);
    }
}

impl Canonical for Value {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'V');
        match self {
            Self::Number(v) => {
                c.tag(b'n').put(v);
            }
            Self::Word(w) => {
                c.tag(b'w').put(w);
            }
            Self::Range(lo, hi) => {
                c.tag(b'r').put(lo).put(hi);
            }
            Self::Dist(d) => {
                c.tag(b'd').put(d);
            }
            Self::Draw(d) => {
                c.tag(b'x').put(d);
            }
            Self::Formula(f) => {
                c.tag(b'=').put(f);
            }
        }
    }
}
