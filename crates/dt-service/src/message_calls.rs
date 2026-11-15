//! The messages as text (`messages`) and a defect report (`messagesReport`): what the page copies from its panel, made by
//! one renderer ([`crate::message::render`], [`crate::report::of`]) for both transports.  The program answers them over
//! HTTP (`GET /api/messages`, `GET /api/messages/report`, which also say the messages as JSON); the browser editor's
//! service answers them as verbs.

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::message::{Message, render};
use crate::report::{Report, of};

/// Which messages: the rows of a tile (`tile`), or one by its number (`seq`), narrowed by the panel's own filters.
#[derive(Deserialize, Serialize, Debug, Default, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct MessagesReq {
    pub tile: Option<String>,
    pub since: Option<u64>,
    pub seq: Option<u64>,
    /// `open` or `closed`; anything else is every state.
    pub state: Option<String>,
    pub kind: Option<String>,
    pub severity: Option<String>,
}

/// The messages in the program's one text: what Copy all and Copy on a row put on the clipboard.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct MessagesText {
    pub text: String,
}

/// `messagesReport`'s request: the filled-in issue for the message `seq`.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MessagesReportReq {
    pub seq: u64,
    /// The page's own environment text; without it the report says the host's own.
    pub env: Option<String>,
    /// Leave the tile's source out.
    pub leave_source_out: Option<bool>,
}

pub const MESSAGES: Verb<MessagesReq, MessagesText> = Verb::new("messages");
pub const MESSAGES_REPORT: Verb<MessagesReportReq, Report> = Verb::new("messagesReport");

/// What the message calls ask of whoever runs them: the owner's messages and a tile's source.
pub trait MessageHost {
    /// The messages of `tile` (every tile's when none) from number `since`.
    fn messages(&self, tile: Option<String>, since: u64) -> Vec<Message>;
    /// The message with this number.
    fn message(&self, seq: u64) -> Option<Message>;
    /// The tile's source text, for a report; none when it has none.
    ///
    /// # Errors
    /// The source could not be read, in words.
    fn source_of(&self, tile: &str) -> Result<Option<String>, String>;
    /// What the host says of itself in a report that the page did not give its own environment for.
    fn environment(&self) -> String;
}

/// The messages `req` names, in the owner's order.
#[must_use]
pub fn select(host: &impl MessageHost, req: &MessagesReq) -> Vec<Message> {
    let want = |v: &Option<String>| v.clone().filter(|v| !v.is_empty() && v != "all");
    let (state, kind, severity) = (want(&req.state), want(&req.kind), want(&req.severity));
    host.messages(req.tile.clone(), req.since.unwrap_or(0))
        .into_iter()
        .filter(|m| {
            req.seq.is_none_or(|n| n == m.seq)
                && state.as_deref().is_none_or(|s| (s == "open") == m.open)
                && kind.as_deref().is_none_or(|k| k == m.kind.as_str())
                && severity.as_deref().is_none_or(|s| s == m.severity.as_str())
        })
        .collect()
}

/// `messages`: the chosen messages as the one text.
///
/// # Errors
/// Never refuses: nothing chosen is the empty text.
pub fn messages(host: &impl MessageHost, req: MessagesReq) -> Result<MessagesText, Refusal> {
    let found = select(host, &req);
    Ok(MessagesText {
        text: render(&found.iter().collect::<Vec<_>>()),
    })
}

/// `messagesReport`: the filled-in new-issue form for a message that means the program went wrong.
///
/// # Errors
/// [`Refusal`]: no such message, a message about the tile (not a defect in the program: there is nothing to report), a
/// source that cannot be read for the report.
pub fn report(host: &impl MessageHost, req: MessagesReportReq) -> Result<Report, Refusal> {
    let Some(m) = host.message(req.seq) else {
        return Err(Refusal::refused(
            Status::NotFound,
            format!("no message {}", req.seq),
        ));
    };
    if !m.reportable() {
        return Err(Refusal::refused(
            Status::BadRequest,
            "this message is about your tile, not a defect in the program: there is nothing to report",
        ));
    }
    let source = if req.leave_source_out == Some(true) {
        None
    } else {
        host.source_of(&m.tile).map_err(|e| {
            Refusal::failure(
                Status::Internal,
                format!("the tile's source could not be read for the report: {e}"),
            )
        })?
    };
    let env = req.env.unwrap_or_else(|| host.environment());
    Ok(of(&m, &env, source.as_deref()))
}
