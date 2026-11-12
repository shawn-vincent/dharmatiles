//! `dharmatiles migrate`: documents from format 0 to format 1, text to text.
//!
//! This is the only crate that reads format 0.  The decoder in `dt-doc`
//! reads format 1 and refuses anything older (`dt::format-0`, with help
//! that names this command); [`old`] is the format 0 decoder kept as it
//! was, so the migration can read what the program no longer builds.
//!
//! - [`heights`]: the older language changes (`ground`, `level=`, `bumps=`);
//! - [`format1`]: the move to format 1 (header, `$` names, units, points);
//! - [`files`]: a tile, the parts it uses and the library's shared files,
//!   migrated together against the library as it was written in format 0.

pub mod files;
pub mod format1;
pub mod heights;
pub mod old;
