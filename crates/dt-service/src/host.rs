//! What the service asks of the program or the page that runs it: somewhere to say what is wrong, the time of day, and
//! the kinds of work and failure the transport has.
//!
//! The owner ([`crate::owner::Core`]) is generic over a [`Host`]: the program's is a unit type that says things to
//! stderr and has the lanes' jobs; a web worker's says them to the console; a test's says them to a list.

/// Where an alarm goes: a failure the service cannot go on past silently (a state that cannot be written, a library
/// that cannot be listed).  Never a place for what the user can fix in a document: those are diagnostics.
pub trait Alarm: Send + Sync {
    /// Say `msg`, loudly, now.
    fn alarm(&self, msg: &str);

    /// Say `msg` the first time it is said (a failure a periodic rescan would otherwise repeat).
    fn alarm_once(&self, msg: &str);
}

/// What the owner needs of the transport that runs it.
pub trait Host: Alarm + 'static {
    /// What a side lane runs: a typed job value, run by the lanes the transport has (threads in the program; the one
    /// thread of a worker in its turn).  The owner only holds, orders and cancels them.
    type Job: Send + 'static;
    /// What a draft that failed to build carries back to its requester (the program's is its server error).
    type Failure: Send + 'static;

    /// Milliseconds since the Unix epoch, for a message's time.
    fn now_ms(&self) -> u64;
}

/// The words of a caught panic (its message), for the log and the page.
pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}
