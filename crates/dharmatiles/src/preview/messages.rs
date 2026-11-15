//! What the preview says happened, as [messages](dt_service::message): the
//! makers of each kind from what a check, a build, an export or a failure
//! returns, and the route that reads the stream (`GET /api/messages`).
//! [`super::Shared::report`] and [`super::Shared::say`] put them in the log
//! and on the event stream; this module only makes them.

use super::api::ErrorDetail;
use dt_eval::TileRecord;
pub(super) use dt_service::message::Scope;
use dt_service::message::{
    BuildData, ExportData, Kind, Message, MessageData, MessagesRes, Origin, ServerData, Severity,
    ShownData, render,
};

use super::Shared;
use super::route::Reply;

pub(super) use dt_service::tile_calls::{of_check, of_errors};

/// A finished build: its timing, and the gate's verdict with its numbers.
/// `record` is the build's record ([`crate::tile_json`]).  `reused` is how
/// many steps were kept from the last build; `cache` is what the disk cache
/// did: on a hit the build says it came from there (and how long it took when
/// it was made), the gate's verdict is the same either way, and each problem
/// the cache met (a damaged entry removed and built again, an entry that could
/// not be written) is a warning of its own.
pub(super) fn of_build(record: &TileRecord, reused: usize, id: &str, name: &str) -> Vec<Message> {
    let cache = &record.cache;
    let secs = record.build_s;
    let target = record.target.as_deref().unwrap_or("");
    let kept = if reused > 0 {
        format!(" ({reused} steps kept from the last build)")
    } else {
        String::new()
    };
    let hit = cache.state == dt_eval::CacheState::Hit;
    let made: f64 = record.stage_secs.iter().map(|(_, s)| s).sum();
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
    let build = Message::new(Kind::Build, Severity::Info, Scope::Build, id, name, text).data(
        MessageData::Build(BuildData {
            build_s: record.build_s,
            triangles: record.audit.triangles,
            target: target.to_owned(),
            stage_secs: record.stage_secs.clone(),
            stage_peak_mb: record.stage_peak_mb.clone(),
            reused,
            cache: if hit { "hit" } else { "miss" }.to_owned(),
            made_s: hit.then_some(made),
        }),
    );
    let mut out = out_of_gate(build, &record.audit, id, name);
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
    .data(MessageData::Shown(ShownData {
        shown: kind.to_owned(),
    }))
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

fn out_of_gate(build: Message, a: &dt_kit::AuditSummary, id: &str, name: &str) -> Vec<Message> {
    let gate = if a.passes() {
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
            format!("Fails the printability gate: {}", a.failures.join("; ")),
        )
        .help(Some(
            "fix the geometry where it is made: the gate is not loosened",
        ))
    }
    .code("gate")
    .data(MessageData::Gate(a.clone()));
    vec![build, gate]
}

/// A tile read as an STL: only its gate.
pub(super) fn of_stl(record: &TileRecord, id: &str, name: &str) -> Vec<Message> {
    let build = Message::new(
        Kind::Build,
        Severity::Info,
        Scope::Build,
        id,
        name,
        format!("Read the STL in {:.1} s", record.build_s),
    );
    out_of_gate(build, &record.audit, id, name)
}

/// A finished export: where it wrote, and for which target.
pub(super) fn of_export(record: &TileRecord, files: &[String], id: &str, name: &str) -> Message {
    let target = record.target.as_deref().unwrap_or("");
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
    .data(MessageData::Export(ExportData {
        files: files.to_vec(),
        target: target.to_owned(),
        build_s: record.build_s,
    }))
}

/// A failure of the program's own, as the one error path makes it
/// ([`super::fail::ServerError::json`]): `error` is its body, `tile` the tile
/// it was about, if any.  `origin` says who found it: `server` or `page`.
pub(super) fn of_server_error(error: &ErrorDetail, tile: (&str, &str), origin: Origin) -> Message {
    let kind = error.kind.as_str();
    let mut m = Message::new(
        Kind::Server,
        Severity::Error,
        Scope::Server,
        tile.0,
        tile.1,
        error.message.clone(),
    )
    .code(if kind == "bug" { "bug" } else { "failure" })
    .help(error.help.as_deref());
    m.doing.clone_from(&error.context.route);
    m = m.data(MessageData::Server(ServerData {
        origin,
        kind: kind.to_owned(),
        route: error.context.route.clone(),
        causes: error.causes.clone(),
        backtrace: error.backtrace.clone(),
        version: error.context.version.clone(),
    }));
    m
}

/// `GET /api/messages?tile=&since=&seq=&format=text|json&state=&kind=&severity=`.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
)]
pub(super) fn read(shared: &Shared, param: &dyn Fn(&str) -> Option<String>) -> Reply {
    let since = param("since").and_then(|s| s.parse().ok()).unwrap_or(0);
    // one message by its number: the details of a row, in the same text as Copy all
    let seq: Option<u64> = param("seq").and_then(|s| s.parse().ok());
    let found = dt_service::message_calls::select(
        shared,
        &dt_service::message_calls::MessagesReq {
            tile: param("tile"),
            since: Some(since),
            seq,
            state: param("state"),
            kind: param("kind"),
            severity: param("severity"),
        },
    );
    match param("format").as_deref() {
        Some("text") => Reply::Body(
            "200 OK",
            "text/plain; charset=utf-8",
            render(&found.iter().collect::<Vec<_>>()),
        ),
        Some("json") | None => super::api::ok(&MessagesRes {
            epoch: shared.epoch.clone(),
            messages: found.iter().map(|m| m.wire()).collect(),
        })
        .into(),
        Some(other) => super::fail::refused(
            "400 Bad Request",
            format!("format is text or json, not {other}"),
        )
        .into(),
    }
}

/// `GET /api/messages/report?seq=N[&source=0][&env=TEXT]`: the filled-in new-issue form for a message that means the
/// program went wrong, as `{url, title, labels, body, truncated}` (the one producer, [`crate::report`]).  `env` is the
/// page's own environment text; without it the program says its own.  A document mistake gets none: 400.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
)]
pub(super) fn report(shared: &Shared, param: &dyn Fn(&str) -> Option<String>) -> Reply {
    let Some(seq) = param("seq").and_then(|s| s.parse::<u64>().ok()) else {
        return super::fail::refused("400 Bad Request", "report needs seq=N, a message's number")
            .into();
    };
    match dt_service::message_calls::report(
        shared,
        dt_service::message_calls::MessagesReportReq {
            seq,
            env: param("env"),
            leave_source_out: Some(param("source").as_deref() == Some("0")),
        },
    ) {
        Ok(r) => super::api::ok(&r).into(),
        Err(r) => super::api::refusal_reply(r).into(),
    }
}

impl dt_service::message_calls::MessageHost for Shared {
    fn messages(&self, tile: Option<String>, since: u64) -> Vec<dt_service::message::Message> {
        self.owner
            .ask(|reply| super::owner::Command::Messages { tile, since, reply })
    }

    fn message(&self, seq: u64) -> Option<dt_service::message::Message> {
        self.owner.ask(|r| super::owner::Command::Message(seq, r))
    }

    fn source_of(&self, tile: &str) -> Result<Option<String>, String> {
        self.text(tile, None)
            .map(|t| t.map(|t| t.text))
            .map_err(|e| e.message)
    }

    fn environment(&self) -> String {
        crate::report::environment()
    }
}
