// Generated from crates/dt-web/src/lib.rs (the engine's answers and the types they name) by
// `DT_CONTRACT=write cargo test -p dt-web --lib the_engine_contract_is_current`.
// Do not edit: a test fails when this file is not what the Rust types say.  The site's scripts
// (docs/assets/live/*.js) are checked against it by `npm test` in web/preview.

type About = { at: number, value: Written | null, };

type Analysis = { 
/**
 * False when the document has a mistake: `diagnostics` says where.
 */
ok: boolean, diagnostics: Array<Diagnostic>, tile: TileInfo | null, stack: Array<OutlineItem>, };

type AuditSummary = { 
/**
 * Triangles written.
 */
triangles: number, 
/**
 * Vertices written.
 */
vertices: number, 
/**
 * Volume written, mm³.
 */
volume_mm3: number, 
/**
 * Closed and consistently oriented after the float32 reload.
 */
watertight: boolean, 
/**
 * Bodies before keep-largest.
 */
bodies: number, 
/**
 * Loose bodies keep-largest dropped, largest first: parts the print would lose.
 */
dropped: Array<Dropped>, 
/**
 * The repairs the gate made, counted.
 */
repairs: Repairs, 
/**
 * Area of the faces past the steepest angle, mm², and the area allowed: the gate's own budget, the
 * printer's or the band's share past its cap, whichever is larger.
 */
overhang_mm2: number, overhang_budget_mm2: number, 
/**
 * Area of the faces in the band, mm², and the area allowed.
 */
band_mm2: number, band_budget_mm2: number, 
/**
 * Vertical rays inside the footprint that miss the mesh.
 */
holes: number, 
/**
 * Why the gate fails, empty if it passes.
 */
failures: Array<string>, };

type BuildMeta = { 
/**
 * False when the document did not build: `diagnostics` says why.
 */
built: boolean, diagnostics: Array<Diagnostic>, 
/**
 * Cells per square the build used; 0 is the build's own finest.
 */
cells: number, footprint: [number, number], 
/**
 * SHA-256 of the STL's triangles (the file after its 80-byte header,
 * which names the engine and document), as hex.
 */
stlSha256: string, stlBytes: number, 
/**
 * The build's record; none when it did not build.
 */
record: TileRecord | null, };

type BuildRecord = { 
/**
 * Engine version, as the build script formats it (`v1.0.0-12-gabc1234`,
 * `gabc1234-dirty`, …).
 */
engine: string, 
/**
 * The document's path, such as `lib:tests/ground/1x1-flat.dtile`.
 */
document: string, 
/**
 * 64-bit FNV-1a of the document's text (16 hex digits on the wire).
 */
document_hash: string, 
/**
 * The tile's root seed.
 */
seed: number, 
/**
 * The connector system.
 */
system: System, 
/**
 * The printer profile's name (`default`, or another).
 */
printer: string, };

type CacheState = "off" | "miss" | "hit";

type CacheUse = { 
/**
 * Whether it was used.
 */
state: CacheState, 
/**
 * On a hit, the engine that made the entry.
 */
made_by: string | null, 
/**
 * What went wrong, in words, for the front end to show: a damaged entry
 * that was removed and built again, an entry that could not be written.
 * The build is real either way.
 */
problems: Array<string>, };

type Checked = { ok: boolean, errors: Array<Diagnostic>, notes: Array<Diagnostic>, 
/**
 * The tile's map, or null when the document has no tile or did not read.
 */
map: TileMap | null, 
/**
 * One square's side on the target, mm.
 */
squareMm: number, 
/**
 * What a full build of this tile will need (`dt_eval::full_cost`, the program's one estimate), so the
 * page can say so before it tries, and refuse on a device that cannot carry it; null when the document
 * has no tile that reads.
 */
fullCost: FullCost | null, 
/**
 * Every node of the text, by the id an edit names it with (`edit_tile`); empty when the document did not read.
 */
nodes: Array<NodeJson>, 
/**
 * The revision of the text those nodes were read from: what an edit carries as `read`.
 */
nodesRev: string | null, };

type Clause = { "kind": "within", at: number, distance: Written | null, of: OfPlace | null, printed: Array<string> | null, } | { "kind": "except", at: number, regions: Run, };

type Diagnostic = { code: string, message: string, label: string, help: string | null, 
/**
 * The file it is in (`tile.dtile` is the page's document).
 */
file: string, 
/**
 * Byte offset and length in that file.
 */
span: [number, number], 
/**
 * UTF-16 offset and length, for a text area.
 */
span16: [number, number], line: number, col: number, 
/**
 * An edit that mends it, when the program has one: write `replacement` over the `len` bytes at `offset`.
 */
fix: FixJson | null, };

type Dropped = { 
/**
 * Its triangles.
 */
triangles: number, 
/**
 * Its volume, mm³.
 */
volume_mm3: number, 
/**
 * Its bounding box, mm (low corner, high corner): where to element.
 */
bounds: [[number, number, number], [number, number, number]], };

type EditReq = { 
/**
 * The `nodesRev` of the check the ids come from.
 */
read?: string | null, } & ({ "op": "set", text: string, id: string, name: string, value?: Scalar | null, } | { "op": "insert", text: string, node: string, place?: InsertPlace | null, id?: string | null, } | { "op": "move", text: string, id: string, before?: string | null, } | { "op": "remove", text: string, id: string, } | { "op": "remove-children", text: string, id: string, names: Array<string>, } | { "op": "rename", text: string, id: string, new: string, } | { "op": "about", text: string, value: string, } | { "op": "arg", text: string, id: string, index: number, value: Scalar, } | { "op": "args", text: string, id: string, values: Array<string>, } | { "op": "point", text: string, id: string, slot: PointSlot, q: [number, number], snap?: boolean | null, } | { "op": "hide", text: string, id: string, } | { "op": "fix", text: string, fix: FixJson, });

type Edited = { 
/**
 * The revision of `made.text` (`EditReq::read` of an edit made to it).
 */
rev: string, text: string, id: string | null, at: number | null, };

type Explored = { 
/**
 * The odds' function: `flat`, `low`, `list`, ...
 */
function: string, args: Array<number>, 
/**
 * As the program writes the range (`low(1..6, 4)`), in dmm.
 */
written: string, 
/**
 * The ends, dmm at the 35 mm reference.
 */
loDmm: number, hiDmm: number, 
/**
 * The ends printed on the target, mm.
 */
loMm: number, hiMm: number, 
/**
 * The program's own curve: each of `bins` slices' share, as a multiple
 * of a flat range's.  Null for a list.
 */
density: Array<number> | null, 
/**
 * A list's values: position along the range and share of the draws.
 */
masses: Array<[number, number]> | null, 
/**
 * How many of `draws` landed in each of `bins` equal slices.
 */
counts: Array<number>, draws: number, 
/**
 * The mean of the draws, dmm; null for no draws (JSON has no NaN).
 */
meanDmm: number | null, };

type FixJson = { offset: number, len: number, 
/**
 * The text there now: the page applies the fix only where it still reads this.
 */
was: string, replacement: string, 
/**
 * The button's words: `Change to dirt`.
 */
label: string, };

type FullCost = { 
/**
 * The highest memory the build reaches, MB.
 */
peakMb: number, 
/**
 * CPU seconds one thread takes (the browser has one).
 */
cpuSecs: number, };

type Height = { name: string, value: number | null, text: string, from: HeightFrom, };

type HeightFrom = "library" | "tile" | "override";

type InsertPlace = "stack" | "tile" | "before" | "in";

type Language = { names: Array<string>, units: Array<string>, functions: Array<string>, };

type LineSheet = { at: number, controls: Array<MapControl>, through: Run | null, };

type MapControl = { name: string, text: string, span: [number, number], 
/**
 * The value with its quotes off and its escapes undone.
 */
plain: string, 
/**
 * The whole control, `name=value` as written, and where.
 */
whole: Written, };

type MapLine = { name: string, pts: Array<[number, number]>, 
/**
 * How far the line may wander, printed mm on this target.
 */
wobbleMm: number | null, span: [number, number] | null, markers: Array<Marker>, points: Array<WrittenPoint>, sheet: LineSheet | null, };

type MapRegion = { name: string, boundedBy: Array<string>, nx: number, ny: number, 
/**
 * One `0` or `1` for each lattice node.
 */
bits: string, span: [number, number] | null, markers: Array<Marker>, fills: Array<WrittenPoint>, sheet: RegionSheet | null, refs: Array<PlaceRef>, };

type Marker = { 
/**
 * The source as written.
 */
label: string, kind: MarkerKind, at: [number, number] | null, span: [number, number] | null, 
/**
 * True when it lies somewhere in a range, so `at` is only the middle of where it may land.
 */
ranged: boolean, };

type MarkerKind = "literal" | "name";

type NamedPoint = { name: string, at: [number, number], span: [number, number], };

type NodeJson = { id: string, 
/**
 * The node's name as written (`rock`).
 */
what: string, 
/**
 * The first word of a node that names itself (`shore` of `line shore`).
 */
name: string | null, 
/**
 * The byte its text starts at.
 */
at: number, 
/**
 * Its length in bytes.
 */
len: number, };

type OfPlace = { text: string, written: string, span: [number, number], };

type OutlineControl = { name: string, 
/**
 * The value as written, without its quotes.
 */
value: string, span16: [number, number], 
/**
 * What the schema says it measures (`Dmm`), when it is a known control.
 */
axis: string | null, about: string | null, printed: PrintedBoth | null, };

type OutlineItem = { what: string, 
/**
 * `element` or `definition`.
 */
callee: string, line: number, span16: [number, number], controls: Array<OutlineControl>, children: Array<OutlineItem>, };

type Place = { name: string, kind: PlaceKind, };

type PlaceKind = "edge" | "line" | "region" | "tile" | "point";

type PlaceRef = { "kind": "point", name: string, at: [number, number], } | { "kind": "tile", name: string, pts: Array<[number, number]> | null, } | { "kind": "edge", name: string, pts: Array<[number, number]>, } | { "kind": "line", name: string, pts: Array<[number, number]>, } | { "kind": "region", name: string, } | { "kind": "unknown", name: string, };

type PointSlot = { "arg": number } | { "control": string };

type PrintedBoth = { 
/**
 * DungeonBlocks: the printed mm of each end, empty when it prints as written.
 */
db: Array<string>, 
/**
 * OpenLOCK.
 */
ol: Array<string>, };

type RegionSheet = { at: number, boundedBy: Run | null, fill: Run | null, clauses: Array<Clause>, };

type Repairs = { 
/**
 * Sealed voids filled.
 */
voids_filled: number, 
/**
 * Vertices pinch separation moved.
 */
pinches_moved: number, 
/**
 * Loose boolean slivers removed.
 */
slivers: number, 
/**
 * Triangles collapsed in the float32 merge.
 */
collapsed: number, 
/**
 * Loose bodies dropped.
 */
dropped: number, };

type Run = { at: number, text: string, 
/**
 * `text` as words (one string's quotes off), as for a [`Written`].
 */
plain: string, span: [number, number] | null, };

type Scalar = boolean | number | number | string;

type Stage = { 
/**
 * The stage, such as `surface` or `connector:tile/flat/connector`.
 */
name: string, 
/**
 * 64-bit FNV-1a of the stage's exact output (16 hex digits on the wire).
 */
hash: string, };

type StageTick = { 
/**
 * 0 to 0.99 (a finished build is the page's `built` answer).
 */
fraction: number, label: string, 
/**
 * Stages begun before this one, and the stages the build will announce.
 */
done: number, total: number, };

type System = "db" | "ol";

type TileInfo = { name: string, cols: number, rows: number, seed: number | null, about: string | null, };

type TileMap = { language: Language, cols: number, rows: number, 
/**
 * Lattice nodes per square of every region's bits.
 */
cells: number, lines: Array<MapLine>, regions: Array<MapRegion>, 
/**
 * The tile's own `point` declarations.
 */
points: Array<NamedPoint>, places: Array<Place>, 
/**
 * The tile node itself; none when the document has no `tile` node.
 */
tile: TileNode | null, };

type TileNode = { at: number, controls: Array<MapControl>, about: About | null, heights: Array<Height>, };

type TileRecord = { 
/**
 * The document's path, or the STL's.
 */
path: string, 
/**
 * Where the build came from; none for an STL.
 */
record: BuildRecord | null, 
/**
 * The build target (`db`, `ol`, `db-fine`); none for an STL.
 */
target: string | null, 
/**
 * The gate's verdict and the numbers it judged.  A low build is not checked: its failures are empty.
 */
audit: AuditSummary, 
/**
 * Whether the gate checked this build: only a full build is.
 */
checked: boolean, 
/**
 * Stage hashes, in build order.
 */
stages: Array<Stage>, 
/**
 * Seconds the build took.
 */
build_s: number, 
/**
 * Seconds each stage took, in build order (`3:grass`, `union`, `audit`).
 */
stage_secs: Array<[string, number]>, 
/**
 * The highest resident memory each stage reached, MB, under the stage names of `stage_secs` (measured
 * natively by sampling the process: read it from a build run alone).  Empty for a build from the cache and
 * for an STL; varies from run to run.
 */
stage_peak_mb: Array<[string, number]>, 
/**
 * The edge level the ground meets every tile edge at, mm.
 */
edge_mm: number | null, 
/**
 * Numbers the elements noted (counts), by name; a name noted again is `name#2`, `name#3`.
 */
stats: { [key in string]: number }, 
/**
 * Pieces elements left out because they could not make them, by element.
 */
left_out: { [key in string]: number }, 
/**
 * What the disk cache did.
 */
cache: CacheUse, };

type Written = { text: string, span: [number, number], plain: string, };

type WrittenPoint = { text: string, span: [number, number], at: [number, number] | null, };

/** Each engine function that answers JSON, and what the JSON is. */
export interface Answers {
  analyze: Analysis;
  check_tile: Checked;
  explore: Explored;
  printed: PrintedBoth | null;
  edit_tile: Edited;
  "build_tile().meta": BuildMeta;
  "dtProgress": StageTick;
}

/** A built tile (`Tile`): its getters, as wasm-bindgen exports them. */
export interface EngineTile {
  readonly meta: string;
  readonly stl: Uint8Array;
  readonly positions: Float32Array;
  readonly colours: Float32Array;
  free(): void;
}

/** The compiled module (docs/engine/dt_web.js, `cargo xtask wasm`, not in the repo): its exports, written here beside the Rust functions they are, so a script is checked the same whether a build is on disk or not. */
export interface Engine {
  default(init: { module_or_path: WebAssembly.Module | string }): Promise<unknown>;
  version(): string;
  set_library(json: string): void;
  analyze(text: string, system: string): string;
  check_tile(text: string, system: string): string;
  fix_tile(text: string, fix: string): string;
  edit_tile(req: string, system: string): string;
  printed(typed: string): string;
  build_tile(text: string, system: string, seed: number | null, cells: number, tier: string, card: string | null): EngineTile;
  forget_card(card: string): void;
  explore(typed: string, system: string, bins: number, draws: number, seed: number): string;
}
