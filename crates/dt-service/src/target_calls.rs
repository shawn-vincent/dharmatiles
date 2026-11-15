//! `setTarget`: choose what every build from now on is built for (the base system and the printer).  Its types and the
//! one function the program's HTTP handler and a web worker's `Service` both call, written on [`TargetHost`]: the host
//! says which printers it has, remembers the choice where it keeps one, and builds the tiles again.

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Verb};
use crate::tile_calls::CheckHost;

/// `setTarget`: the system (`db` or `ol`) and/or the printer's name; what is not given stays.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TargetReq {
    pub system: Option<dt_lang::System>,
    pub printer: Option<String>,
}

/// The target now.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct TargetRes {
    pub system: dt_lang::System,
    pub printer: String,
}

pub const SET_TARGET: Verb<TargetReq, TargetRes> = Verb::new("setTarget");

/// What `setTarget` asks of whoever runs it.
pub trait TargetHost: CheckHost {
    /// Choose the base system and the printer for every build from now on (either may be left as it is): the tiles built
    /// so far are built again, and the choice is remembered where the host keeps one.  Answers the target now.
    ///
    /// # Errors
    /// [`Refusal`] (bad request): a printer the host does not have, or a profile that does not read.
    fn set_target(
        &self,
        system: Option<dt_lang::System>,
        printer: Option<&str>,
    ) -> Result<TargetRes, Refusal>;
}

/// `setTarget`.
///
/// # Errors
/// As [`TargetHost::set_target`].
pub fn set_target(host: &impl TargetHost, req: TargetReq) -> Result<TargetRes, Refusal> {
    host.set_target(req.system, req.printer.as_deref())
}
