//! The editor's service: what sits between a transport (HTTP in the program, a web worker in the browser) and the
//! engine.
//!
//! It knows no disk, no sockets and no HTTP: files come through a [`storage::Storage`] the host gives it (the
//! program's disk, or a [`storage::MemStorage`] in a worker), and time through a [`clock::Clock`], so the same code
//! builds for `wasm32-unknown-unknown`.  `cargo build -p dt-service --target wasm32-unknown-unknown` holds that.

pub mod clock;
pub mod storage;
