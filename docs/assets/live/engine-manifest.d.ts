// Generated from xtask/src/wasm.rs (`EngineManifest`) by `DT_CONTRACT=write cargo test -p xtask the_engine_manifest_type_is_current`.
// Do not edit: a test fails when this file is not what the Rust type says.  The cards' engine.js is typed by it.

type EngineManifest = { 
/**
 * The engine's hash: 16 hex digits of the three files' hashes together.
 */
hash: string, 
/**
 * The glue, beside the manifest.
 */
js: string, 
/**
 * The module.
 */
wasm: string, 
/**
 * The library as JSON.
 */
library: string, };

export type { EngineManifest };
