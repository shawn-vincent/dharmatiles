//! The editor's service in a web worker: the same verbs the program answers over HTTP (`dt_service::tile_calls`),
//! called by name with a JSON request, over a [`MemStorage`] holding the tiles the page hands it.
//!
//! `Service::call(verb, tile, request)` is the whole transport: it reads the request as the verb's type, calls the
//! service's [`Table`], and writes the answer as the verb's response type.  `check` is the program's own
//! (`dt_service::tile_calls::check`, one function for both); `buildNow` is the worker's build, run inline in the call
//! (the program queues a build on a lane and answers 202, so it is a verb of its own, not a second meaning of
//! `build`).  What a check found is said as messages, which `events()` hands the page and clears.  One worker is one
//! service; the library is the one [`crate::set_library`] was given.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dt_eval::{Options, Source, Tier, TileRecord};
use dt_lang::System;
use dt_service::call::{CallError, Table, Verb};
use dt_service::message::{Message, Scope};
use dt_service::storage::{MemStorage, Storage};
use dt_service::tile_calls::{self, CHECK, CheckHost, Failed, TextReq};
use dt_value::LockExt;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{JsError, LIBRARY, MAIN, build_with, stl_sha256, system_of};

/// `buildNow` takes the target the tile is built for (`db` or `ol`).
#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BuildNowReq {
    pub system: String,
}

/// What a build came to: the program's own record, and the hash of the STL's triangles (empty when the gate
/// stopped the build: only a build that passed has an STL).
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BuildNowRes {
    pub record: TileRecord,
    pub passed: bool,
    pub stl_bytes: usize,
    pub stl_sha256: String,
}

/// The worker's build, as a verb.
pub const BUILD_NOW: Verb<BuildNowReq, BuildNowRes> = Verb::new("buildNow");

/// What a call runs on: the store, the target and the place a check's messages go; and the tile it is about.
struct Ctx {
    storage: Arc<MemStorage>,
    system: System,
    said: Arc<Mutex<Vec<Message>>>,
    tile: String,
}

/// The document is `tile.dtile` ([`MAIN`]); every other path is a library file, as the page's engine reads them.
struct Doc<'a> {
    library: &'a BTreeMap<String, String>,
    text: &'a str,
}

impl Source for Doc<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == MAIN {
            return Ok(self.text.to_owned());
        }
        self.library
            .get(path)
            .cloned()
            .ok_or_else(|| format!("the page's library has no `{path}`"))
    }
}

/// A check's host: the options (the library's default printer, on the call's target) and where messages go.
struct Checking<'a> {
    ctx: &'a Ctx,
    options: Options,
}

impl CheckHost for Checking<'_> {
    fn options(&self) -> Options {
        self.options.clone()
    }

    fn report(&self, _id: &str, _scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
        let tile = &self.ctx.tile;
        let name = Path::new(tile)
            .file_stem()
            .map_or_else(|| tile.clone(), |s| s.to_string_lossy().into_owned());
        self.ctx.said.lock_unpoisoned().extend(make(&name));
    }
}

fn options_in(source: &dyn Source, system: System) -> Result<Options, Failed> {
    let mut o = Options::in_source(source).map_err(|ds| {
        Failed(format!(
            "the library's default printer does not read: {}",
            ds.iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })?;
    o.system = system;
    Ok(o)
}

/// Run `f` on the document `text` (or, with none, the tile's text from the store) read through the library.
fn with_doc<T>(
    ctx: &Ctx,
    text: Option<String>,
    f: impl FnOnce(&Doc<'_>) -> Result<T, Failed>,
) -> Result<T, Failed> {
    let text = match text {
        Some(t) => t,
        None => ctx
            .storage
            .read_text(Path::new(&ctx.tile))
            .map_err(|e| Failed(format!("cannot read {}: {e}", ctx.tile)))?,
    };
    LIBRARY.with(|lib| {
        f(&Doc {
            library: &lib.borrow(),
            text: &text,
        })
    })
}

fn check(ctx: &Ctx, req: TextReq) -> Result<tile_calls::CheckRes, Failed> {
    with_doc(ctx, Some(req.text), |doc| {
        let host = Checking {
            ctx,
            options: options_in(doc, ctx.system)?,
        };
        tile_calls::check(&host, doc, MAIN)
    })
}

fn build_now(ctx: &Ctx, req: BuildNowReq) -> Result<BuildNowRes, Failed> {
    let system = req.system.parse::<System>().map_err(Failed)?;
    with_doc(ctx, None, |doc| {
        let (built, opts) = build_with(
            doc,
            system,
            None,
            0,
            Tier::Full,
            &dt_eval::Progress::none(),
            None,
        )
        .map_err(|ds| {
            Failed(format!(
                "{} did not build: {}",
                ctx.tile,
                ds.iter()
                    .map(|d| format!("{d:?}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?;
        let passed = built.verdict(&opts.printer).passed();
        let (stl_sha256, stl_bytes) = passed
            .as_ref()
            .map_or_else(|| (String::new(), 0), stl_sha256);
        let build_s = built.report.stage_secs.iter().map(|s| s.1).sum();
        Ok(BuildNowRes {
            record: TileRecord::of(&built, &opts.printer, build_s),
            passed: passed.is_some(),
            stl_bytes,
            stl_sha256,
        })
    })
}

/// The verbs a worker answers, once.
fn table() -> Table<Ctx, Failed> {
    Table::default()
        .with(&CHECK, check)
        .with(&BUILD_NOW, build_now)
}

/// Read `request` as the verb's request type, call it, and write the answer.
fn run<Q, S>(
    table: &Table<Ctx, Failed>,
    ctx: &Ctx,
    verb: &Verb<Q, S>,
    request: &str,
) -> Result<String, JsError>
where
    Q: serde::de::DeserializeOwned + 'static,
    S: Serialize + 'static,
{
    let req: Q = serde_json::from_str(request)
        .map_err(|e| JsError::new(&format!("{} does not take this: {e}", verb.name)))?;
    match table.call(ctx, verb, req) {
        Ok(answer) => serde_json::to_string(&answer)
            .map_err(|e| JsError::new(&format!("cannot write the answer: {e}"))),
        Err(CallError::Refused(Failed(why))) => Err(JsError::new(&why)),
        Err(e) => Err(JsError::new(&format!("the call table is wrong: {e:?}"))),
    }
}

/// The service in a worker.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Service {
    storage: Arc<MemStorage>,
    system: Mutex<System>,
    said: Arc<Mutex<Vec<Message>>>,
    table: Table<Ctx, Failed>,
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Service {
    /// An empty service: no tiles, the DungeonBlocks target.  The library comes from [`crate::set_library`].
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    #[must_use]
    pub fn new() -> Self {
        Self {
            storage: Arc::new(MemStorage::default()),
            system: Mutex::new(System::Db),
            said: Arc::new(Mutex::new(Vec::new())),
            table: table(),
        }
    }

    /// Put a tile's text in the store at `path` (a later `put` is a save).
    ///
    /// # Errors
    /// The store cannot be written.
    pub fn put(&self, path: &str, text: &str) -> Result<(), JsError> {
        self.storage
            .write_text(Path::new(path), text)
            .map_err(|e| JsError::new(&format!("cannot write {path}: {e}")))
    }

    /// Choose the build target for the calls that follow (`db` or `ol`).
    ///
    /// # Errors
    /// Not a target.
    pub fn set_target(&self, system: &str) -> Result<(), JsError> {
        *self.system.lock_unpoisoned() = system_of(system)?;
        Ok(())
    }

    /// Call the verb `verb` about the tile stored at `tile`, with `request` (JSON), and answer JSON.
    ///
    /// # Errors
    /// No such verb, a request that is not the verb's, or the call refused or failed: its words.
    pub fn call(&self, verb: &str, tile: &str, request: &str) -> Result<String, JsError> {
        let ctx = Ctx {
            storage: self.storage.clone(),
            system: *self.system.lock_unpoisoned(),
            said: self.said.clone(),
            tile: tile.to_owned(),
        };
        match verb {
            v if v == CHECK.name => run(&self.table, &ctx, &CHECK, request),
            v if v == BUILD_NOW.name => run(&self.table, &ctx, &BUILD_NOW, request),
            other => Err(JsError::new(&format!("no such call: {other}"))),
        }
    }

    /// What the calls since the last pull said (a check's mistakes and notes), as a JSON array of messages; they are
    /// handed over once.
    ///
    /// # Errors
    /// A message that cannot be written.
    pub fn events(&self) -> Result<String, JsError> {
        let said = std::mem::take(&mut *self.said.lock_unpoisoned());
        let said: Vec<_> = said.iter().map(Message::wire).collect();
        serde_json::to_string(&said)
            .map_err(|e| JsError::new(&format!("cannot write the events: {e}")))
    }
}
