//! `GET /api/weightings`: the weighting functions as the editor's gallery
//! draws them.  The curves are the program's: the page sends text, the
//! program reads it as one range with its odds ([`dt_eval::ranged`]) and
//! answers with the density sampled on a grid, so no maths lives in the
//! page.
//!
//! | query | answer |
//! |---|---|
//! | `range=1..6` | every function over that range, each at its default numbers: its text, its numbers and limits, its curve |
//! | `text=low(1..6, 4)` | that value: its canonical text, its `function` and `numbers`, its curve |
//! | `fn=low&range=1..6&args=12.3` | the function over the range with those numbers, as `text=` would answer for what it writes |
//!
//! A value the program refuses is `{ok: false, code, message, help}`, the
//! same words a document's check gives (a number outside its limits is
//! `dt::range`).  A call that is not understood is a 400 with `error`.

use serde_json::{Value, json};

/// Slices of the range in a curve.
pub(crate) const BINS: usize = 64;

/// The number as written: no trailing noise (`4`, not `4.0`).
fn num(x: f64) -> String {
    format!("{x}")
}

/// What `text` reads as: its curve, or the program's refusal.
fn shown(text: &str) -> Value {
    match dt_eval::ranged(text) {
        Ok(r) => {
            let d = &r.dist;
            let curve = match d.weighting.masses() {
                Some(m) => json!({
                    "kind": "masses",
                    "values": m.iter().map(|&(t, share)| json!([t, share])).collect::<Vec<_>>(),
                }),
                None => json!({
                    "kind": "density",
                    "values": d.weighting.density(BINS).unwrap_or_default(),
                }),
            };
            json!({
                "ok": true,
                "text": d.written(),
                "function": r.function,
                "numbers": r.args,
                "range": [d.lo, d.hi],
                "curve": curve,
            })
        }
        Err(e) => json!({
            "ok": false,
            "code": e.code,
            "message": e.message,
            "help": e.help,
        }),
    }
}

/// A list over `lo..hi`: each whole number when the range is a few whole
/// numbers, else four evenly spaced values.
fn list_over(lo: f64, hi: f64) -> String {
    const WEIGHTS: [f64; 8] = [4.0, 2.0, 5.0, 1.0, 3.0, 2.0, 4.0, 1.0];
    let whole = lo.fract() == 0.0 && hi.fract() == 0.0 && (hi - lo) <= 7.0;
    let values: Vec<f64> = if whole {
        std::iter::successors(Some(lo), |v| Some(v + 1.0))
            .take_while(|v| *v <= hi)
            .collect()
    } else {
        (0..4)
            .map(|i| ((lo + (hi - lo) * f64::from(i) / 3.0) * 100.0).round() / 100.0)
            .collect()
    };
    let items: Vec<String> = values
        .iter()
        .zip(WEIGHTS)
        .map(|(v, w)| format!("{}:{}", num(*v), num(w)))
        .collect();
    format!("[{}]", items.join(", "))
}

/// The text of one function over `range` at its default numbers.
fn default_text(f: &dt_kit::WeightingFn, range: &str, lo: f64, hi: f64) -> String {
    match f.name {
        "flat" => range.to_owned(),
        "steps" => format!("steps({range}, 1, 3, 5, 2)"),
        "list" => list_over(lo, hi),
        "mix" => format!("mix(bell({range}, 0.15), 4, flat({range}), 1)"),
        name => {
            let mut t = format!("{name}({range}");
            for a in f.args {
                t.push_str(&format!(", {}", num(a.default.unwrap_or(a.least))));
            }
            t.push(')');
            t
        }
    }
}

/// The reply to one request, as (status, JSON).
pub(crate) fn answer(param: &dyn Fn(&str) -> Option<String>) -> (&'static str, Value) {
    let bad = |why: &str| ("400 Bad Request", json!({ "error": why }));
    if let Some(text) = param("text") {
        return ("200 OK", shown(&text));
    }
    let Some(range) = param("range") else {
        return bad("give a range, or the text of a value");
    };
    // the range as the program reads it: its two ends
    let (lo, hi, range) = match dt_eval::ranged(&range) {
        Ok(r) if r.dist.hi > r.dist.lo => {
            let (lo, hi) = (r.dist.lo, r.dist.hi);
            (lo, hi, format!("{}..{}", num(lo), num(hi)))
        }
        Ok(_) => {
            return (
                "200 OK",
                json!({ "ok": false, "code": "dt::range", "message": "a weighting needs a range with some width", "help": "write a range such as 1..6" }),
            );
        }
        Err(e) => {
            return (
                "200 OK",
                json!({ "ok": false, "code": e.code, "message": e.message, "help": e.help }),
            );
        }
    };
    if let Some(name) = param("fn") {
        let Some(f) = dt_kit::WEIGHTINGS.iter().find(|f| f.name == name) else {
            return bad(&format!("no weighting function `{name}`"));
        };
        let mut args = Vec::new();
        for a in param("args").unwrap_or_default().split(',') {
            let a = a.trim();
            if a.is_empty() {
                continue;
            }
            match a.parse::<f64>() {
                Ok(x) => args.push(x),
                Err(_) => return bad(&format!("`{a}` is not a number")),
            }
        }
        let text = match f.name {
            "flat" => range,
            "list" | "mix" => return bad("write a list or a mix as text"),
            n => {
                let mut t = format!("{n}({range}");
                for a in args {
                    t.push_str(&format!(", {}", num(a)));
                }
                t.push(')');
                t
            }
        };
        return ("200 OK", shown(&text));
    }
    let functions: Vec<Value> = dt_kit::WEIGHTINGS
        .iter()
        .map(|f| {
            let mut v = crate::elements::weighting_json(f);
            let shown = shown(&default_text(f, &range, lo, hi));
            if let Some(o) = shown.as_object() {
                for (k, x) in o {
                    v[k] = x.clone();
                }
            }
            v
        })
        .collect();
    (
        "200 OK",
        json!({ "ok": true, "range": [lo, hi], "functions": functions }),
    )
}
