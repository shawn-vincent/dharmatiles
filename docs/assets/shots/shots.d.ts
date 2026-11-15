// Generated from xtask/src/shots.rs (`Book`) by `DT_CONTRACT=write cargo test -p xtask the_shots_types_are_current`.
// Do not edit: a test fails when this file is not what the Rust types say.  The docs' site.js and
// xtask/shots/shots.mjs are typed by it.

type Anchor = { x: number, y: number, w: number, h: number, };

type Book = { 
/**
 * The size every picture is drawn at, px.
 */
width: number, height: number, 
/**
 * Each picture, by name.
 */
shots: { [key in string]: Shot }, };

type Shot = { w: number, h: number, 
/**
 * Callout name to the box it points at.
 */
anchors: { [key in string]: Anchor }, };

export type { Anchor, Book, Shot };
