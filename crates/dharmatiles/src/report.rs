//! The command line's side of a report: what the machine says about itself, and the line a panic prints.  The report
//! itself (the filled-in new-issue form for a message that means the program went wrong) is `dt_service::report`: one
//! producer for the editor's `messagesReport` in the program and in the web worker, and for the command line.

use crate::preview::fail::Panic;
use dt_service::message::{Kind, Message, MessageData, Origin, Scope, ServerData, Severity};
use dt_service::report::of;

/// What the machine says about itself, for a report made where there is no page to say it (the command line).
pub(crate) fn environment() -> String {
    format!(
        "Environment\n  dharmatiles   {}\n  platform      {} {}",
        crate::cli::version_line(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// The command line met a panic (a bug, never a mistake in a document): one more line, the issue's address.
pub(crate) fn tell_panic(p: &Panic) {
    let mut m = Message::new(
        Kind::Server,
        Severity::Error,
        Scope::Server,
        "",
        "",
        p.message.clone(),
    )
    .code("bug");
    m.data = Some(MessageData::Server(ServerData {
        origin: Origin::Cli,
        kind: "bug".to_owned(),
        route: None,
        causes: vec![format!("panicked at {}", p.location)],
        backtrace: p.backtrace.clone(),
        version: env!("DT_ENGINE_VERSION").to_owned(),
    }));
    let r = of(&m, &environment(), None);
    if r.truncated {
        let path =
            std::env::temp_dir().join(format!("dharmatiles-report-{}.txt", std::process::id()));
        match std::fs::write(&path, &r.body) {
            Ok(()) => eprintln!(
                "report: {}\nreport: the full report is in {}",
                r.url,
                path.display()
            ),
            Err(e) => eprintln!(
                "report: {}\nreport: the full report could not be written to {}: {e}",
                r.url,
                path.display()
            ),
        }
    } else {
        eprintln!("report: {}", r.url);
    }
}
