//! What the preview says happened, as [messages](crate::message): the
//! makers of each kind from what a check, a build, an export or a failure
//! returns, and the route that reads the stream (`GET /api/messages`).
//! [`super::Shared::report`] and [`super::Shared::say`] put them in the log
//! and on the event stream; this module only makes them.

use dt_eval::Diag;
use serde_json::{Value, json};

pub(super) use crate::message::Scope;
use crate::message::{Kind, Message, Severity, render};

use super::route::Reply;
use super::{Shared, lock};

/// What a check says: that it read the tile, and each note it made
/// (`dt::steep`, `dt::edge`).  `stack` is the plan's stack items, for the
/// entry each note is about.
pub(super) fn of_check(
    c: &dt_eval::Checked,
    id: &str,
    name: &str,
    stack: &[Value],
) -> Vec<Message> {
    let mut out = Vec::new();
    if let Some(t) = &c.tile {
        out.push(
            Message::new(
                Kind::Check,
                Severity::Info,
                Scope::Check,
                id,
                name,
                format!(
                    "Read {name}: {}×{} squares, {} stack entries",
                    t.cols,
                    t.rows,
                    t.stack.len()
                ),
            )
            .data(json!({"cols": t.cols, "rows": t.rows, "entries": t.stack.len()})),
        );
    }
    out.extend(
        c.notes()
            .iter()
            .map(|d| Message::from_diag(d, Scope::Check, true, id, name, stack)),
    );
    out
}

/// A check or a build that stopped on mistakes: one message each.
pub(super) fn of_errors(
    errs: &[Diag],
    scope: Scope,
    id: &str,
    name: &str,
    stack: &[Value],
) -> Vec<Message> {
    errs.iter()
        .map(|d| Message::from_diag(d, scope, false, id, name, stack))
        .collect()
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// A finished build: its timing, and the gate's verdict with its numbers.
/// `record` is the build's record ([`crate::tile_json`]).  `reused` is how
/// many steps were kept from the last build; `cache` is what the disk cache
/// did: on a hit the build says it came from there (and how long it took when
/// it was made), the gate's verdict is the same either way, and each problem
/// the cache met (a damaged entry removed and built again, an entry that could
/// not be written) is a warning of its own.
pub(super) fn of_build(
    record: &Value,
    reused: usize,
    cache: &dt_eval::CacheUse,
    id: &str,
    name: &str,
) -> Vec<Message> {
    let a = &record["audit"];
    let secs = number(&record["build_s"]);
    let target = record["record"]["target"].as_str().unwrap_or("");
    let kept = if reused > 0 {
        format!(" ({reused} steps kept from the last build)")
    } else {
        String::new()
    };
    let hit = cache.state == dt_eval::CacheState::Hit;
    let made: f64 = record["stage_secs"]
        .as_object()
        .map_or(0.0, |m| m.values().map(number).sum());
    let text = if hit {
        format!(
            "From the cache ({made:.1} s when it was built{})",
            cache
                .made_by
                .as_deref()
                .map_or_else(String::new, |e| format!(", by {e}"))
        )
    } else {
        format!("Built in {secs:.1} s{kept}")
    };
    let build =
        Message::new(Kind::Build, Severity::Info, Scope::Build, id, name, text).data(json!({
            "build_s": record["build_s"], "triangles": a["triangles"], "target": target,
            "stage_secs": record["stage_secs"], "reused": reused,
            "cache": if hit { "hit" } else { "miss" },
            "made_s": if hit { json!(made) } else { Value::Null },
        }));
    let mut out = out_of_gate(build, a, id, name);
    for problem in &cache.problems {
        out.push(
            Message::new(
                Kind::Build,
                Severity::Warning,
                Scope::Build,
                id,
                name,
                format!("The build cache: {problem}"),
            )
            .code("cache")
            .help(Some(
                "the build itself is real; `dharmatiles cache clear` empties the cache",
            )),
        );
    }
    out
}

/// A tile that is being built shows a build the cache kept meanwhile: an
/// earlier build of it (it has changed since) or the library's stored
/// preview.  Said so, never a silent swap; the build's own report replaces it.
pub(super) fn of_shown(note: &str, kind: &str, id: &str, name: &str) -> Message {
    Message::new(
        Kind::Build,
        Severity::Info,
        Scope::Build,
        id,
        name,
        format!("Showing {note} while the tile builds"),
    )
    .code("cache")
    .data(json!({"shown": kind}))
}

/// A problem the cache met before the build began (a damaged entry it
/// removed while looking for one to show).
pub(super) fn of_cache_problem(problem: &str, id: &str, name: &str) -> Message {
    Message::new(
        Kind::Build,
        Severity::Warning,
        Scope::Build,
        id,
        name,
        format!("The build cache: {problem}"),
    )
    .code("cache")
    .help(Some(
        "the build itself is real; `dharmatiles cache clear` empties the cache",
    ))
}

fn out_of_gate(build: Message, a: &Value, id: &str, name: &str) -> Vec<Message> {
    let failures: Vec<String> = a["failures"]
        .as_array()
        .map(|f| {
            f.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let dropped = a["dropped"]
        .as_array()
        .map_or_else(|| number(&a["dropped"]), |d| d.len() as f64);
    let numbers = json!({
        "triangles": a["triangles"], "watertight": a["watertight"],
        "band_mm2": a["band_mm2"], "band_budget_mm2": a["band_budget_mm2"],
        "overhang_mm2": a["overhang_mm2"], "overhang_budget_mm2": a["overhang_budget_mm2"],
        "dropped": dropped, "holes": a["holes"], "voids_filled": a["voids_filled"],
        "failures": failures,
    });
    let gate = if failures.is_empty() {
        Message::new(
            Kind::Gate,
            Severity::Info,
            Scope::Build,
            id,
            name,
            "Passes the printability gate",
        )
    } else {
        Message::new(
            Kind::Gate,
            Severity::Error,
            Scope::Build,
            id,
            name,
            format!("Fails the printability gate: {}", failures.join("; ")),
        )
        .help(Some(
            "fix the geometry where it is made: the gate is not loosened",
        ))
    }
    .code("gate")
    .data(numbers);
    vec![build, gate]
}

/// A tile read as an STL: only its gate.
pub(super) fn of_stl(record: &Value, id: &str, name: &str) -> Vec<Message> {
    let build = Message::new(
        Kind::Build,
        Severity::Info,
        Scope::Build,
        id,
        name,
        format!("Read the STL in {:.1} s", number(&record["build_s"])),
    );
    out_of_gate(build, &record["audit"], id, name)
}

/// A finished export: where it wrote, and for which target.
pub(super) fn of_export(record: &Value, files: &[String], id: &str, name: &str) -> Message {
    let target = record["record"]["target"].as_str().unwrap_or("");
    let shown = files
        .iter()
        .filter(|f| f.ends_with(".stl") || f.ends_with(".3mf"))
        .map(|f| f.rsplit('/').next().unwrap_or(f).to_owned())
        .collect::<Vec<_>>();
    Message::new(
        Kind::Export,
        Severity::Info,
        Scope::Export,
        id,
        name,
        format!("Exported {} for {target}", shown.join(", ")),
    )
    .data(json!({"files": files, "target": target, "build_s": record["build_s"]}))
}

/// A failure of the program's own, as the one error path makes it
/// ([`super::fail::ServerError::json`]): `error` is its body, `tile` the tile
/// it was about, if any.  `origin` says who found it: `server` or `page`.
pub(super) fn of_server_error(error: &Value, tile: (&str, &str), origin: &str) -> Message {
    let kind = error["kind"].as_str().unwrap_or("failure");
    let mut m = Message::new(
        Kind::Server,
        Severity::Error,
        Scope::Server,
        tile.0,
        tile.1,
        error["message"].as_str().unwrap_or("a failure"),
    )
    .code(if kind == "bug" { "bug" } else { "failure" })
    .help(error["help"].as_str());
    m.data = json!({
        "origin": origin,
        "kind": kind,
        "route": error["context"]["route"],
        "causes": error["causes"],
        "backtrace": error["backtrace"],
        "version": error["context"]["version"],
    });
    m
}

/// `GET /api/messages?tile=&since=&format=text|json&state=&kind=&severity=`.
pub(super) fn read(shared: &Shared, param: &dyn Fn(&str) -> Option<String>) -> Reply {
    let since = param("since").and_then(|s| s.parse().ok()).unwrap_or(0);
    let tile = param("tile");
    let want = |k: &str| param(k).filter(|v| !v.is_empty() && v != "all");
    let (state, kind, severity) = (want("state"), want("kind"), want("severity"));
    let ev = lock(&shared.events);
    let found = ev.messages.query(tile.as_deref(), since, |m| {
        state.as_deref().is_none_or(|s| (s == "open") == m.open)
            && kind.as_deref().is_none_or(|k| k == m.kind.as_str())
            && severity.as_deref().is_none_or(|s| s == m.severity.as_str())
    });
    match param("format").as_deref() {
        Some("text") => Reply::Body("200 OK", "text/plain; charset=utf-8", render(&found)),
        Some("json") | None => Reply::Json(
            "200 OK",
            json!({
                "epoch": shared.epoch,
                "messages": found.iter().map(|m| m.json()).collect::<Vec<_>>(),
            }),
        ),
        Some(other) => super::fail::refused(
            "400 Bad Request",
            format!("format is text or json, not {other}"),
        )
        .into(),
    }
}
