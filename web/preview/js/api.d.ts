// Generated from crates/dharmatiles/src/preview (api.rs and the types it names) by
// `DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current`.
// Do not edit: a test fails when this file is not what the Rust types say.

type About = { at: number, value: Written | null, };

type AboutRes = { version: string, formats: string, };

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

type BuildData = { build_s: number, triangles: number, target: string, 
/**
 * Seconds each stage took, in build order.
 */
stage_secs: Array<[string, number]>, 
/**
 * The highest resident memory each stage reached, MB, under the same names (empty from the cache).
 */
stage_peak_mb: Array<[string, number]>, reused: number, 
/**
 * `hit` or `miss`.
 */
cache: string, 
/**
 * On a cache hit, how long the tile took when it was made.
 */
made_s: number | null, };

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

type CallAt = { file: string, span: [number, number], };

type CandidateJson = { 
/**
 * The profile name it would be written under.
 */
name: string, brand: string, model: string, 
/**
 * The slicer's own name for it.
 */
profile: string, bed: [number, number], height: number | null, nozzle: number | null, slots: number, 
/**
 * Keep-out zones on the bed.
 */
zones: number, 
/**
 * You already have a profile of that name.
 */
have: boolean, };

type Changed = { 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, };

type CheckData = { cols: number, rows: number, entries: number, };

type CheckRes = { ok: boolean, errors: Array<Mistake>, stack: Array<StackItem> | null, map: TileMap | null, 
/**
 * Every node of the text that was read, by the id an edit names it with (`/api/edit`), for a tile.
 */
nodes: Array<NodeJson>, 
/**
 * The revision of the text those nodes were read from; an edit names them with it (`EditReq::read`).
 */
nodesRev: string | null, };

type CheckValueReq = { element: string, control: string, typed: string, };

type CheckValueRes = { message: string | null, label: string | null, help: string | null, };

type Clause = { "kind": "within", at: number, distance: Written | null, of: OfPlace | null, printed: Array<string> | null, } | { "kind": "except", at: number, regions: Run, };

type CloudAt = { 
/**
 * `top` or `pin`.
 */
kind: string, file: string, span: [number, number], 
/**
 * A pin's place among its entry's pins.
 */
pin: number | null, };

type CloudColumns = { 
/**
 * An entry's key.
 */
owner: string, class: string, 
/**
 * `x, y, low, high, rolled, shade low, shade high, shade rolled`, for each column.
 */
v: Array<number>, };

type CloudSheet = { 
/**
 * An entry's key (`tile/dirt/0`).
 */
owner: string, class: string, nx: number, ny: number, step: number, 
/**
 * Row by row, `nx` by `ny`: the height, or none where the sheet has no node.
 */
z: Array<number | null>, 
/**
 * 0 to 1, one for each node.
 */
shade: Array<number>, };

type Clouds = { sheets: Array<CloudSheet>, columns: Array<CloudColumns>, thinned: boolean, };

type ColourWord = "single" | "multi";

type ControlJson = { name: string, 
/**
 * What it measures; none for a group.
 */
unit: string | null, numeric: boolean, 
/**
 * `[min, max]`, max none for no high end.
 */
limits: [number, number | null] | null, 
/**
 * `limits` refuses its minimum itself.
 */
above: boolean, ranges: boolean, weighted: boolean, 
/**
 * What it takes, in words.
 */
takes: string | null, default: string | null, 
/**
 * The default as a format 1 document writes it (a length with its unit).
 */
default1: string | null, 
/**
 * What leaving it out means, where there is no value to show for it.
 */
absent: string | null, required: boolean, expert: boolean, 
/**
 * Whether this build takes it.
 */
taken: boolean, about: string, words: Array<WordJson>, subs: Array<ControlJson>, };

type DebugMarks = { 
/**
 * By class: `[x, y, z, …]`.
 */
dots: { [key in string]: Array<number> }, 
/**
 * By class: polylines, each `[x, y, z, …]`.
 */
trails: { [key in string]: Array<Array<number>> }, thinned: boolean, };

type DefKind = { name: string, kind: string, };

type Deleted = { path: string, };

type DiagData = { label?: string, unexpected?: boolean, };

type Dims = { x: number, y: number, z: number, };

type Done = Record<string, never>;

type Download = { name: string, mediaType: string, data: string, };

type DraftReq = { text: string, 
/**
 * A file the tile uses, instead of its own document.
 */
file?: string | null, 
/**
 * Ask for the debug layer's marks too.
 */
debug?: boolean | null, };

type DraftTier = "draft" | "low";

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
read?: string | null, } & ({ "op": "set", text: string, id: string, name: string, value?: Scalar | null, } | { "op": "insert", text: string, node: string, place?: InsertPlace | null, id?: string | null, } | { "op": "make", text: string, node: NewNode, place?: InsertPlace | null, id?: string | null, } | { "op": "move", text: string, id: string, before?: string | null, } | { "op": "remove", text: string, id: string, } | { "op": "remove-children", text: string, id: string, names: Array<string>, } | { "op": "rename", text: string, id: string, new: string, } | { "op": "about", text: string, value: string, } | { "op": "arg", text: string, id: string, index: number, value: Scalar, } | { "op": "args", text: string, id: string, values: Array<string>, } | { "op": "point", text: string, id: string, slot: PointSlot, q: [number, number], snap?: boolean | null, } | { "op": "hide", text: string, id: string, } | { "op": "fix", text: string, fix: FixJson, });

type EditRes = { text: string, id: string | null, at: number | null, };

type ElementJson = { 
/**
 * The place words it takes: `at`, `side`.
 */
place: Array<string>, 
/**
 * What `at=` may name.
 */
takes: Array<string>, 
/**
 * The place words it needs.
 */
needs: Array<string>, holdsScatter: boolean, 
/**
 * Whether it is a body (fills a band over a region).
 */
body: boolean, controls: Array<ControlJson>, 
/**
 * The weighting functions, for an element with a control that takes one; empty for the rest.
 */
weightings: Array<WeightingJson>, name: string, 
/**
 * `surface`, `thing`, `run` or `scatter`.
 */
role: string, 
/**
 * `thing` (one solid from its controls and a seed) or `layer` (reads the scene so far), from the role.
 */
generator: string, about: string, 
/**
 * The element's page on the site.
 */
href: string, family: string, 
/**
 * Whether this build builds it.
 */
built: boolean, };

type ErrorBody = { error: ErrorDetail, 
/**
 * A 409: the file as it is now.
 */
text: string | null, 
/**
 * A 409: the revision the file has now.
 */
docRev: number | null, 
/**
 * A 409 for a map: the map as it now reads.
 */
scene: SceneJson | null, 
/**
 * A 422: the document's mistakes, each at its place.
 */
errors: Array<Mistake> | null, 
/**
 * A 409 for a draft: a newer draft took its place.
 */
superseded: boolean | null, };

type ErrorContext = { route: string | null, tile: string | null, version: string, atMs: number, };

type ErrorDetail = { 
/**
 * `refused`, `failure` or `bug` (see [`fail`]).
 */
kind: string, message: string, help: string | null, causes: Array<string>, backtrace: string, context: ErrorContext, };

type ExportBuilt = { path: string, 
/**
 * The files written, by path: the program's (empty where the files are downloads).
 */
files: Array<string>, 
/**
 * The files to save: the browser editor's (empty where the program wrote them).
 */
downloads: Array<Download>, 
/**
 * The build's record: whether it passes the gate is `build.audit.failures`.
 */
build: TileRecord, };

type ExportData = { files: Array<string>, target: string, build_s: number, };

type ExportReq = { 
/**
 * Also write the painted 3MF.
 */
threemf?: boolean | null, 
/**
 * Only say where the files would go.
 */
where?: boolean | null, };

type ExportRes = { out: string, shown: string, } | ExportBuilt;

type FixJson = { offset: number, len: number, 
/**
 * The text there now: the page applies the fix only where it still reads this.
 */
was: string, replacement: string, 
/**
 * The button's words: `Change to dirt`.
 */
label: string, };

type FolderDone = { 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, dir: string, };

type FolderJson = { root: number, path: string, };

type FolderNamedReq = { root: number, dir: string, name: string, };

type FolderReq = { root: number, 
/**
 * The folder, by its path below the place (`""` is the place).
 */
dir: string, };

type Height = { name: string, value: number | null, text: string, from: HeightFrom, };

type HeightFrom = "library" | "tile" | "override";

type HeightName = { name: string, dmm: number, own: boolean, };

type InsertPlace = "stack" | "tile" | "before" | "in";

type InventoryDoc = { writable: boolean, inventory: InventoryRead, };

type InventoryGoneReq = { tile: string, seed: number, system: System, colour: ColourWord, note?: string | null, on?: string | null, };

type InventoryHaveReq = { tile: string, colour: ColourWord, system?: System | null, seed?: number | null, count?: number | null, 
/**
 * The date, like 2026-11-09 (today when left out).
 */
on?: string | null, };

type InventoryRead = { text: string, pieces: Array<InventoryRow>, 
/**
 * What is wrong with the file; empty when it reads.
 */
problems: Array<Mistake>, };

type InventoryRecordReq = { list: string, 
/**
 * The plate, from 1.
 */
plate: number, 
/**
 * The pieces that failed, by their place in the plate's list.
 */
failed?: Array<number> | null, on?: string | null, };

type InventoryRelinkReq = { 
/**
 * The old path, as the log has it.
 */
from: string, 
/**
 * The id of the listed tile it is now.
 */
to: string, on?: string | null, };

type InventoryReprintReq = { tile: string, seed: number, system: System, colour: ColourWord, 
/**
 * The print list's id; none makes a new list.
 */
list?: string | null, 
/**
 * The revision of the list the caller last saw, when it has the list open.
 */
docRev?: number | null, };

type InventoryReprinted = { 
/**
 * The list's path.
 */
list: string, id: string, made: boolean, warning: string | null, always: boolean, before: string | null, text: string, docRev: number, note: string | null, };

type InventoryRow = { tile: string, seed: number, system: System, colour: ColourWord, have: number, printed: number, failed: number, gone: number, hash: string | null, last: string | null, edited: boolean, missing: boolean, lines: Array<number>, };

type InventoryState = { "state": "unavailable" } | { "state": "absent", home: string, 
/**
 * Where the file will be.
 */
path: string, id: string, } | { "state": "read", home: string, path: string, id: string, pieces: Array<InventoryRow>, lastColour: ColourWord, } | { "state": "broken", home: string, path: string, id: string, 
/**
 * What is wrong with it, one line each.
 */
problems: Array<string>, };

type InventoryWrote = { lines: Array<string>, file: string, };

type Kind = "progress" | "check" | "build" | "gate" | "error" | "note" | "export" | "server";

type Language = { names: Array<string>, units: Array<string>, functions: Array<string>, };

type LanguageJson = { 
/**
 * The documentation site, with its trailing slash.
 */
docs: string, nodes: Array<NodeSchema>, units: Array<string>, 
/**
 * What each unit word means, for the tooltip on a written unit.
 */
unitAbout: { [key in string]: string }, functions: Array<string>, weightings: Array<string>, 
/**
 * The `$` names.
 */
names: Array<string>, };

type LayoutData = { 
/**
 * How many tiles are drafts, not yet checked.
 */
low: number, plates: number, build_s: number, };

type LineSheet = { at: number, controls: Array<MapControl>, through: Run | null, };

type List = { ids: Array<string>, added: Array<TileSummary>, };

type ListEdited = { 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type ListPiece = { tile: string, count: number, 
/**
 * The id the piece has, or the one the first edit of the file will write on it.
 */
id: string, pinned: boolean, seed: number | null, system: System | null, print: string, };

type Listed = { path: string, id: string | null, tile: TileSummary | null, };

type ListedIn = { path: string, id: string | null, tile: TileSummary | null, 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, };

type ListsAddReq = { tile: string, 
/**
 * The list's id; none makes a new list.
 */
list?: string | null, seed?: number | null, system?: System | null, 
/**
 * The revision of the list the caller last saw, when it has the list open.
 */
docRev?: number | null, };

type ListsAddRes = { 
/**
 * How many copies of the piece the list now holds.
 */
count: number, 
/**
 * The list was made by this call.
 */
made: boolean, 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type ListsCountReq = { list: string, id: string, count: number, docRev: number, };

type ListsNewReq = { name?: string | null, root?: number | null, dir?: string | null, 
/**
 * A map (with a grid) rather than a print list.
 */
map?: boolean | null, };

type ListsRemoveReq = { list: string, id: string, docRev: number, };

type ListsRestoreReq = { list: string, text: string, docRev: number, };

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

type MapsAddReq = { list: string, tile: string, 
/**
 * The place, as written; or `near`.
 */
at?: string | null, 
/**
 * The nearest free square to this point (squares of the piece's system), settled in the write.
 */
near?: Near | null, rot?: number | null, seed?: number | null, 
/**
 * A seed to give it, settled in the write, when `seed` is not given.
 */
seedWord?: SeedWord | null, system?: System | null, docRev: number, };

type MapsBatchReq = { list: string, edits: Array<PieceEdit>, docRev: number, };

type MapsBatched = { changed: number, 
/**
 * How many pieces went to another square than the one wished.
 */
shifted: number, 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type MapsDupReq = { list: string, id: string, at?: string | null, near?: Near | null, docRev: number, };

type MapsListReq = { list: string, docRev: number, };

type MapsPieceReq = { list: string, id: string, docRev: number, };

type MapsPieced = { id: string, 
/**
 * The piece went to another square than the one wished (the nearest free one).
 */
shifted: number, 
/**
 * The seed it was given (settled in the write), when it was asked for one.
 */
seed: number | null, 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type MapsPinReq = { list: string, id: string, 
/**
 * The plate, counting from 1.
 */
plate: number, x: number, y: number, turn?: boolean | null, docRev: number, };

type MapsSetReq = { list: string, id: string, set: PieceWords, docRev: number, };

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

type MeshHeader = { kind: MeshKind, 
/**
 * How many triangles the arrays hold (set when the mesh is encoded).
 */
triangles: number, 
/**
 * How many vertices the shared positions hold, and how many colours the palette (set when the mesh is encoded).
 */
vertices: number, palette: number, 
/**
 * The mesh's own box, mm (set when the mesh is encoded).
 */
lo: [number, number, number], hi: [number, number, number], 
/**
 * The printer's overhang angle; a sketch has none.
 */
overhangDeg: number | null, 
/**
 * The tile's footprint, mm; a sketch has none.
 */
footprint: [number, number] | null, 
/**
 * Faces the gate painted red, and orange; a build and a draft have them.
 */
overhangFaces: number | null, bandFaces: number | null, 
/**
 * Whether the gate checked what this mesh shows: a low or super-low build, a sketch has not been.
 */
checked: boolean, 
/**
 * The gate's failures; empty when it passes or did not check.
 */
failures: Array<string>, 
/**
 * A draft's mesh was thinned.
 */
thinned: boolean, 
/**
 * A sketch is drawn rough.
 */
rough: boolean, 
/**
 * The tier: a map piece's name, or a plate's (`full`, `mixed`, `low`).
 */
tier: string | null, 
/**
 * The revision of the document a map piece is of.
 */
docRev: number | null, 
/**
 * A map piece's own box, mm.
 */
tileLo: [number, number, number] | null, tileHi: [number, number, number] | null, 
/**
 * The stack the mesh was built from; empty for a mesh made without one.
 */
stack: Array<MeshStackEntry>, clouds: Clouds | null, debug: DebugMarks | null, 
/**
 * A plate's printer bed, mm, and the zones kept clear, `[x0, y0, x1, y1]`.
 */
bed: [number, number] | null, excluded: Array<[number, number, number, number]> | null, 
/**
 * Which plate is shown, and how many there are.
 */
plate: number | null, plates: number | null, };

type MeshKind = "build" | "draft" | "mapPiece" | "sketch" | "stl" | "plates";

type MeshStackEntry = { index: number, key: string, label: string, element: string, what: string, z: [number, number] | null, };

type MessageData = { "type": "check" } & CheckData | { "type": "build" } & BuildData | { "type": "shown" } & ShownData | { "type": "gate" } & AuditSummary | { "type": "export" } & ExportData | { "type": "layout" } & LayoutData | { "type": "plate_files" } & PlateFilesData | { "type": "inventory" } & InventoryWrote | { "type": "diag" } & DiagData | { "type": "server" } & ServerData;

type MessageJson = { 
/**
 * Order in the stream.
 */
seq: number, 
/**
 * When, ISO 8601 UTC.
 */
t: string, kind: Kind, severity: Severity, scope: Scope, code: string | null, 
/**
 * The tile's id, and its name.
 */
tile: string, tile_name: string, 
/**
 * The stack entry it is about (its key).
 */
node: string | null, span: Span | null, text: string, help: string | null, 
/**
 * An edit that mends it (a button in the page), when the program has one.
 */
fix: FixJson | null, 
/**
 * The tile file it is about, as the program names it.
 */
file: string | null, 
/**
 * What was being done (a build, a picture, an export, with its system, printer, seed and tier).
 */
doing: string | null, 
/**
 * The line of the file the span is on.
 */
source: string | null, 
/**
 * The numbers of the kind, when it has any: one variant for each sort of message that carries data.
 */
data: MessageData | null, state: MessageState, 
/**
 * The program went wrong (a bug or failure of its own, a gate failure): the editor offers Report this defect.
 * Never true for a mistake in the document.
 */
reportable: boolean, };

type MessageState = "open" | "cleared";

type MessagesRes = { epoch: string, messages: Array<MessageJson>, };

type Mistake = { code: string, message: string, file: string | null, line: number | null, column: number | null, help: string | null, 
/**
 * An edit that mends it, when the program has one.
 */
fix: FixJson | null, 
/**
 * A failure of the program, not of the document: shown as a bug.
 */
unexpected: boolean, };

type NamedPoint = { name: string, at: [number, number], span: [number, number], };

type NamesReq = { text: string, };

type NamesRes = { ok: boolean, message: string | null, defs: Array<string>, regions: Array<string>, lines: Array<string>, points: Array<string>, 
/**
 * What each definition is as a generator, as a word of the language's one kind table (`thing` or `layer`): a
 * thing parameter may be given the things, a layer parameter the layers and the things.
 */
kinds: Array<DefKind>, 
/**
 * What each parameter holds (`kind` is a word of the kind table), for completing `param=` with the names that
 * fit: a thing slot offers things, a layer slot layers and things, a value slot values.
 */
params: Array<ParamKind>, };

type Near = { x: number, y: number, };

type NewControl = { 
/**
 * The control's name.
 */
name: string, 
/**
 * Its value.
 */
value: NewValue, };

type NewNode = { 
/**
 * Its name (`rock`, `scatter`, `line`).
 */
name: string, 
/**
 * Its positional arguments, in order.
 */
args: Array<NewValue>, 
/**
 * Its controls, in order.
 */
controls: Array<NewControl>, 
/**
 * Its children, in order.
 */
children: Array<NewNode>, 
/**
 * Whether the children stand on lines of their own (else they follow on its line: `line a { through p q }`).
 */
spread: boolean, };

type NewReq = { name?: string | null, 
/**
 * Squares across and up, like `2x1`.
 */
size?: string | null, 
/**
 * The folder place and the folder in it, when not My tiles.
 */
root?: number | null, dir?: string | null, };

type NewValue = { "word": string } | { "text": string } | { "int": number } | { "num": number } | { "bool": boolean } | { "point": [number, number] };

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

type NodeSchema = { name: string, about: string, href: string, controls: Array<ControlJson>, };

type Nothing = Record<string, never>;

type OfPlace = { text: string, written: string, span: [number, number], };

type Offer = { name: string, family: string, about: string, 
/**
 * The line of KDL it would write.
 */
node: string, 
/**
 * Whether the plan lets it stand there.
 */
ok: boolean, 
/**
 * When it does not, why.
 */
why: string | null, };

type OffersReq = { text: string, 
/**
 * The place in the stack: 0 is the top.
 */
at: number, file?: string | null, };

type OffersRes = { ok: boolean, at: number | null, offers: Array<Offer>, why: string | null, errors: Array<Mistake> | null, };

type Origin = "server" | "page" | "cli";

type OwnedSeed = { seed: number, have: number, };

type PageMessageReq = { tile?: string | null, what?: string | null, text: string, help?: string | null, causes?: Array<string> | null, backtrace?: string | null, };

type ParamKind = { define: string, param: string, kind: string, };

type PieceEdit = { id: string, set: PieceWords, };

type PieceWords = { 
/**
 * Where, as written: `(2sq, 1sq)`.
 */
at?: string | null, 
/**
 * Instead of `at`: the nearest free square to this point, settled by the program in the write.
 */
near?: Near | null, 
/**
 * Instead of `seed`: a seed nobody owns, that the map does not use and no other piece of this edit has.
 */
freshSeed?: boolean | null, 
/**
 * Quarter turns clockwise, in degrees: 0, 90, 180, 270.
 */
rot?: number | null, 
/**
 * A height: a `$` name or a length.
 */
bottom?: string | null, seed?: number | null, 
/**
 * `missing`, or the default (no word).
 */
print?: string | null, system?: string | null, };

type Place = { name: string, kind: PlaceKind, };

type PlaceAdded = { added: boolean, root: number, label: string, path: string, 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, };

type PlaceKind = "edge" | "line" | "region" | "tile" | "point";

type PlaceRef = { "kind": "point", name: string, at: [number, number], } | { "kind": "tile", name: string, pts: Array<[number, number]> | null, } | { "kind": "edge", name: string, pts: Array<[number, number]>, } | { "kind": "line", name: string, pts: Array<[number, number]>, } | { "kind": "region", name: string, } | { "kind": "unknown", name: string, };

type Places = { roots: Array<RootJson>, folders: Array<FolderJson>, home: string | null, };

type PlacesAddRes = { cancelled: boolean, } | PlaceAdded;

type PlacesRemoveReq = { 
/**
 * The place, by its index in `roots`.
 */
root: number, };

type PlanHeld = { name: string, have: number, };

type PlanKind = { 
/**
 * Its footprint, mm.
 */
size: [number, number], height: number, 
/**
 * Its filaments are the build's (not the stack's estimate).
 */
exact: boolean, filaments: Array<string>, tile: string, name: string, count: number, want: number, have: number, otherColour: number, 
/**
 * The tile as the inventory names it.
 */
canon: string, seedUsed: number, seed: number | null, system: System, };

type PlanPlaced = { 
/**
 * Which of the list's kinds.
 */
kind: number, copy: number, name: string, x: number, y: number, w: number, d: number, turned: boolean, 
/**
 * The list piece this copy is (an index into the list's pieces).
 */
piece: number | null, pinned: boolean, 
/**
 * That list piece's id, as an edit names it.
 */
id: string | null, };

type PlanPlate = { 
/**
 * From 1.
 */
plate: number, filaments: Array<string>, pieces: Array<PlanPlaced>, };

type PlateFilesData = { files: Array<string>, format: string, build_s: number, };

type PlatesDoc = { 
/**
 * The file can be edited.
 */
writable: boolean, plates: PlatesPanel, 
/**
 * The map as read; none while the file does not read as a list.
 */
scene: SceneJson | null, 
/**
 * The revision of the file the scene (and the layout) is of.
 */
docRev: number, squareMm: number, 
/**
 * Width by depth, mm: a tile's, or a print list's printer's bed; none when its printer names no bed (nothing is
 * made up for it).
 */
sizeMm: [number, number] | null, 
/**
 * The zero height: where a tile's base meets its body, or a bed's top.  None for an STL file, which has no zero of
 * its own: the mesh's lowest point stands in.
 */
datumZ: number | null, };

type PlatesExportReq = { list: string, 
/**
 * `bambu`, `stl` or `3mf`: what the files are; the printer's own (a Bambu project for a Bambu printer, else one STL
 * per plate) when none is named.
 */
format?: string | null, };

type PlatesExported = { 
/**
 * The files written, by path: the program's (empty where the files are downloads).
 */
files: Array<string>, 
/**
 * The files to save: the browser editor's (empty where the program wrote them).
 */
downloads: Array<Download>, format: string, 
/**
 * The pieces that fail the printability gate, by tile.
 */
failures: Array<string>, plates: number, totalS: number, out: string, };

type PlatesPanel = PlatesPlanned | PlatesWritten | PlatesUnread;

type PlatesPlanned = { planned: true, kinds: Array<PlanKind>, layout: Array<PlanPlate>, 
/**
 * The plate on show, from 0.
 */
view: number, 
/**
 * Every piece's mesh is its full build.
 */
exact: boolean, groupingCost: number | null, slotCheck: SlotCheck, writable: boolean, 
/**
 * How an export is written: `bambu`, `stl` or `3mf`.
 */
format: string, printers: Array<PrinterRow>, 
/**
 * The list's own pieces, in the order written.
 */
pieces: Array<ListPiece>, title: string, printer: string, label: string, bed: [number, number] | null, excluded: Array<[number, number, number, number]>, slots: number, verify: string | null, system: System, mode: string, group: string, gap: number, margin: number, turn: boolean, skipped: Array<string>, held: Array<PlanHeld>, checkInventory: boolean, };

type PlatesSetReq = { list: string, 
/**
 * `printer`, `mode`, `group`, `gap`, `margin`, `turn`, `system` or `inventory`.
 */
key: string, 
/**
 * The word; null takes it off.
 */
value?: string | null, docRev: number, };

type PlatesShowReq = { 
/**
 * The print list's id.
 */
list: string, 
/**
 * The plate in view, from 0.
 */
plate: number, };

type PlatesUnread = { planned: false, writable: boolean, printers: Array<PrinterRow>, };

type PlatesWritten = { planned: false, writable: boolean, printer: string, label: string | null, bed: [number, number] | null, slots: number | null, mode: string | null, group: string | null, gap: number | null, margin: number | null, printers: Array<PrinterRow>, checkInventory: boolean, pieces: Array<ListPiece>, };

type PointSlot = { "arg": number } | { "control": string };

type PosterMap = { 
/**
 * The square's size on the target the poster was made for, mm.
 */
squareMm: number, map: TileMap, };

type PrintedItem = { element: string, control: string, typed: string, };

type PrintedReq = { text: string, items: Array<PrintedItem>, };

type PrintedRes = { items: Array<PrintedSize | null>, };

type PrintedSetReq = { typed: string, end: number, mm: number, };

type PrintedSetRes = { typed: string, };

type PrintedSize = { ends: Array<string>, editable: boolean, };

type PrinterBroken = { name: string, label: string, slots: number, mine: boolean, 
/**
 * What is wrong with it.
 */
broken: string, };

type PrinterRead = { name: string, brand: string | null, model: string | null, label: string, bed: [number, number] | null, height: number | null, slots: number, verify: string | null, zones: number, fromSlicer: string | null, mine: boolean, };

type PrinterRow = PrinterRead | PrinterBroken;

type PrinterWritten = { name: string, 
/**
 * Where the profile is.
 */
path: string, 
/**
 * The user's own profile of that name was kept.
 */
kept: boolean, };

type PrintersImportReq = { slicer: string, 
/**
 * The profile names of the models to write.
 */
names: Array<string>, force?: boolean | null, };

type PrintersImportRes = { written: Array<PrinterWritten>, };

type PrintersScanReq = { slicer?: string | null, };

type PrintersScanRes = { slicers: Array<SlicerFound>, } | ScanJson;

type Progress = { 
/**
 * What the work is: `building` (a tile's stages) or `loading` (a map's pieces).  Work in flight for a document
 * is this one record, whatever the work: the bar, the running row and the status line read nothing else.
 */
what: string, fraction: number, label: string, key: string, 
/**
 * What the stage is counting: `[what, done, total]`; a total of 0 is just the step.
 */
count: [string, number, number] | null, elapsedS: number, etaS: number | null, estimateS: number | null, };

type RegionSheet = { at: number, boundedBy: Run | null, fill: Run | null, clauses: Array<Clause>, };

type RenameReq = { name: string, };

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

type Report = { url: string, title: string, labels: Array<string>, 
/**
 * The whole report: what the preview shows, and what goes to the clipboard when the URL is shortened.
 */
body: string, 
/**
 * The URL carries a short body; the whole is in `body`.
 */
truncated: boolean, };

type ResourceEvent = { 
/**
 * The stream's revision (for resuming).
 */
rev: number, 
/**
 * The resource's own revision: it rises when this resource changes and not otherwise.
 */
resRev: number, } & ({ "name": "places", "value": Places } | { "name": "target", "value": Target } | { "name": "printers", "value": Array<PrinterRow> } | { "name": "inventory", "value": InventoryState } | { "name": "list", "value": List });

type Restored = { path: string, id: string | null, tile: TileSummary | null, 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, };

type RevealReq = { path: string, };

type RootJson = { index: number, label: string, 
/**
 * `files`, `dir` or `library`.
 */
kind: string, path: string | null, role: string, writable: boolean, exists: boolean, };

type Run = { at: number, text: string, 
/**
 * `text` as words (one string's quotes off), as for a [`Written`].
 */
plain: string, span: [number, number] | null, };

type SaveReq = { text: string, 
/**
 * The revision of the file the text was made from.
 */
docRev: number, file?: string | null, };

type SaveRes = { docRev: number, };

type Scalar = boolean | number | number | string;

type ScanJson = { slicer: string, title: string, version: string, label: string, models: Array<CandidateJson>, 
/**
 * What could not be read, each in words.
 */
problems: Array<string>, };

type SceneGrid = { cols: number, rows: number, };

type SceneJson = { 
/**
 * The revision of the file this is the reading of.
 */
docRev: number, 
/**
 * A map (placed pieces or a grid) rather than a print list.
 */
map: boolean, grid: SceneGrid | null, about: string | null, pieces: Array<ScenePiece>, 
/**
 * How many pieces are placed.
 */
placed: number, 
/**
 * `[x0, y0, x1, y1]` of the placed pieces, mm, y down.
 */
bounds: [number, number, number, number] | null, heights: Array<HeightName>, 
/**
 * An inventory exists, so the shelf's word on each piece means something.
 */
shelfExists: boolean, 
/**
 * The pieces the inventory leaves off (`shelf.state` is `have`) and the pieces that print.
 */
have: number, toPrint: number, 
/**
 * `single` or `multi`.
 */
colour: string, system: System, 
/**
 * The square of `system`, mm: where a tile dropped on the map lands.
 */
squareMm: number, printer: string, };

type ScenePiece = { 
/**
 * The id the piece has, or the one the first edit of the file will write on it.
 */
id: string, 
/**
 * Whether the file has written it yet.
 */
written: boolean, 
/**
 * The tile as the piece names it.
 */
tile: string, canon: string, name: string, 
/**
 * The listed tile's id, when the tile is listed.
 */
tileId: string | null, path: string, seed: number | null, seedUsed: number, system: System, colour: string, cols: number, rows: number, 
/**
 * One square, mm.
 */
sq: number, 
/**
 * Width and depth, mm.
 */
size: [number, number], at: [number, number] | null, rot: number | null, bottomMm: number, bottom: string | null, print: string, count: number, pinned: boolean, error: string | null, 
/**
 * What the shelf says of the piece (none for a piece with a mistake).
 */
shelf: ShelfMark | null, 
/**
 * The seeds you own of this piece's tile in its system and colour mode, and how many of each.
 */
ownedSeeds: Array<OwnedSeed>, };

type Scope = "check" | "build" | "export" | "server";

type SeedWord = "fresh" | "if-needed";

type ServerData = { origin: Origin, 
/**
 * `refused`, `failure` or `bug` (see `preview::fail`).
 */
kind: string, route: string | null, 
/**
 * The error chain, outermost first, or a panic's place.
 */
causes: Array<string>, backtrace: string, version: string, };

type SessionRes = { token: string, };

type Severity = "error" | "warning" | "note" | "info";

type ShelfMark = { state: string, text: string, };

type Shown = { 
/**
 * What the mesh is of: `earlier` (a full build the cache kept), `preview` (a stored low tier, or the plates' drafts).
 */
kind: string, 
/**
 * Words for the status line.
 */
note: string, 
/**
 * Whether the gate has checked the mesh on show: a draft has not been.
 */
checked: boolean, 
/**
 * `low` or `full`, for a build on show; a plates preview has no tier.
 */
tier: string | null, };

type ShownData = { 
/**
 * What is shown: an earlier build of the tile, or the library's stored preview.
 */
shown: string, };

type SketchRef = { id: number, rough: boolean, };

type SlicerFound = { slicer: string, label: string, title: string, };

type SlotCheck = { need: number, have: number, ok: boolean, };

type Snapshot = { 
/**
 * The contract stamp of this program (`contract()`): a page that imported a different one is from another build.
 */
contract: string, rev: number, epoch: string, system: System, printer: string, printers: Array<PrinterRow>, overhangDeg: number, steepestDeg: number, squareMm: number, roots: Array<RootJson>, folders: Array<FolderJson>, home: string | null, inventory: InventoryState, tiles: Array<TileSummary>, };

type Span = { file: string, line: number, col: number, 
/**
 * Byte offset and length in the file's text.
 */
offset: number, len: number, };

type StackItem = { 
/**
 * The entry's id ([`dt_eval::doc::Entry::id`]): what the page keeps to find the entry again whatever the stack
 * looks like later.
 */
key: string, label: string, element: string, 
/**
 * The page on the site that documents the element.
 */
href: string, 
/**
 * The word that places it (`dirt`, `scatter`), or `base`.
 */
what: string, 
/**
 * The seed the entry pins (`seed=`): the page says which entries keep their own.
 */
seed: number | null, file: string, 
/**
 * Where it is written: byte offset and length.
 */
span: [number, number], 
/**
 * First and last line (1-based).
 */
lines: [number, number], call: CallAt | null, controls: Array<WrittenControl>, defaults: Array<UnwrittenControl>, clouds: Array<CloudAt>, place: Array<WrittenPlace>, 
/**
 * Where a fixed `at=` point is, in squares from the south-west corner, as the program reads it.
 */
at: [number, number] | null, things: Array<Thing>, };

type Stage = { 
/**
 * The stage, such as `surface` or `connector:tile/flat/connector`.
 */
name: string, 
/**
 * 64-bit FNV-1a of the stage's exact output (16 hex digits on the wire).
 */
hash: string, };

type StepDraft = "mesh" | "mistake" | "superseded";

type StepHead = { text: string, id: string | null, at: number | null, 
/**
 * The revision of `text` (`EditReq::read` of an edit made to it, as a check's `nodesRev`).
 */
rev: string, draft: StepDraft, };

type StepReq = { 
/**
 * The edit, as `edit` takes it (with the `read` of the text it is made to).
 */
edit: EditReq, 
/**
 * A file the tile uses, instead of its own document.
 */
file?: string | null, debug?: boolean | null, 
/**
 * How coarse the draft is: `low` while a drag is going on (the step has to keep up with the pointer), left out
 * (the usual draft) for the step that ends it and for every other change.
 */
tier?: DraftTier | null, };

type StreamEvent = { "type": "snapshot" } & Snapshot | { "type": "resumed", rev: number, epoch: string, } | { "type": "tile", rev: number, tile: TileSummary, } | { "type": "progress", rev: number, id: string, progress: Progress, } | { "type": "message", rev: number, message: MessageJson, } | { "type": "tier", rev: number, key: string, docRev: number, error: string | null, 
/**
 * The error is a mistake in the document's text (not a failure of the program).
 */
mistake: boolean, 
/**
 * The code of the document's mistake (`dt::seam`) when `mistake`.
 */
code: string | null, } | { "type": "resource" } & ResourceEvent;

type System = "db" | "ol";

type Target = { system: System, printer: string, overhangDeg: number, steepestDeg: number, 
/**
 * One square's side on the chosen system, mm (35 DungeonBlocks, 25.4 OpenLOCK): the page's only source for it.
 */
squareMm: number, };

type TargetReq = { system?: System | null, printer?: string | null, };

type TargetRes = { system: System, printer: string, };

type TextReq = { text: string, 
/**
 * A file the tile uses, instead of its own document.
 */
file?: string | null, };

type TextRes = { file: string, text: string, writable: boolean, 
/**
 * The revision of the file the text is of; 0 for a library file.
 */
docRev: number, };

type Thing = { what: string, 
/**
 * The offset its name starts at: a node is addressed by it.
 */
at: number, controls: Array<WrittenControl>, defaults: Array<UnwrittenControl>, };

type ThingControl = { control: string, text: string, from: string, };

type ThingDraw = { seed: number, 
/**
 * The triangles of the mesh.
 */
triangles: number, 
/**
 * This draw's own box, mm as printed.
 */
size: Dims, 
/**
 * Whether the gate checked this draw: a full one is, a low one is not (each face flagged by its angle alone).
 */
checked: boolean, 
/**
 * The mesh payload (`web/preview/js/payload.js` reads it), as standard base64: an answer is JSON.
 */
mesh: string, };

type ThingExtent = { least: Dims, most: Dims, leastDmm: Dims, mostDmm: Dims, };

type ThingRef = { "kind": "define", name: string, } | { "kind": "entry", key: string, child: number | null, };

type ThingReq = { text: string, 
/**
 * A file the tile uses, instead of its own document.
 */
file?: string | null, which: ThingRef, 
/**
 * One seed for the view, or many for the strip; each is drawn on its own.
 */
seeds: Array<number>, 
/**
 * How finely: `full` (joined and through the gate's overhang paint: what prints) when left out, `low` for a quick look (a
 * tree takes tens of seconds at full, and a fraction of one at low, as in a tile).
 */
tier?: Tier | null, };

type ThingRes = { 
/**
 * The element it is (`rock`).
 */
element: string, 
/**
 * Its controls as built.
 */
shown: Array<ThingControl>, 
/**
 * The smallest and largest box it can draw.
 */
extent: ThingExtent, 
/**
 * One for each seed asked, in order.
 */
draws: Array<ThingDraw>, 
/**
 * What the draws did for their bodies: made, and read from the shelf (a seed asked again is read, not made).
 */
used: ThingUse, 
/**
 * What went wrong with the shelf (a damaged entry dropped and made again, a body that could not be kept), in words: the
 * draws are real either way, and the page shows each of these.
 */
problems: Array<string>, };

type ThingUse = { 
/**
 * Bodies made.
 */
built: number, 
/**
 * Bodies a variant had already made in this build, copied.
 */
shared: number, 
/**
 * Bodies read from the shelf (made by an earlier build).
 */
kept: number, };

type Tier = "full" | "draft" | "low" | "super-low";

type TierWant = { tile: string, 
/**
 * `super-low`, `low` or `full`.
 */
tier: Tier, seed?: number | null, system?: System | null, printer?: string | null, 
/**
 * A source text to build in place of the file's own (a step of the document's history): the file is not
 * edited, and the picture is kept by the text, so a text built before is not built again.
 */
text?: string | null, 
/**
 * The file `text` is of: the tile's own document, or a file its last build read.
 */
file?: string | null, };

type TiersWantReq = { 
/**
 * Whose wants these are (a map, the picture shelf): a post replaces the last of the same scope.
 */
scope: string, 
/**
 * The document the work is for (a map's tile id): its pieces loading is one job of it, said as the document's
 * progress and in the messages like a build.  None for the picture shelf.
 */
doc?: string | null, wants: Array<TierWant>, };

type TiersWantRes = { keys: Array<string>, };

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

type TileMoveReq = { root: number, dir: string, };

type TileNode = { at: number, controls: Array<MapControl>, about: About | null, heights: Array<Height>, };

type TilePlan = { "kind": "tile", stack: Array<StackItem>, cols: number, rows: number, squareMm: number, 
/**
 * Width by depth, mm: a tile's, or a print list's printer's bed; none when its printer names no bed (nothing is
 * made up for it).
 */
sizeMm: [number, number] | null, 
/**
 * The zero height: where a tile's base meets its body, or a bed's top.  None for an STL file, which has no zero of
 * its own: the mesh's lowest point stands in.
 */
datumZ: number | null, } | { "kind": "stl", 
/**
 * The file's name.
 */
tile: string, stack: Array<StackItem>, cols: number, rows: number, squareMm: number, 
/**
 * Width by depth, mm: a tile's, or a print list's printer's bed; none when its printer names no bed (nothing is
 * made up for it).
 */
sizeMm: [number, number] | null, 
/**
 * The zero height: where a tile's base meets its body, or a bed's top.  None for an STL file, which has no zero of
 * its own: the mesh's lowest point stands in.
 */
datumZ: number | null, } | { "kind": "plates" } & PlatesDoc | { "kind": "inventory" } & InventoryDoc;

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
cache: CacheUse, 
/**
 * What the build did for its things: made, copied, read from the shelf.
 */
things: ThingUse, };

type TileSummary = { id: string, path: string, name: string, 
/**
 * The place it is listed under: an index into `roots`.
 */
root: number, group: string | null, size: string | null, 
/**
 * `idle`, `building`, `ready` or `failed`.
 */
status: string, 
/**
 * What the plan pass, the plates' layout or the reading of the inventory said.
 */
plan: TilePlan | null, 
/**
 * The build's record: audit, stages, timings (snake_case, as `dharmatiles build --json` prints it).
 */
record: TileRecord | null, errors: Array<Mistake>, 
/**
 * A build that panicked or could not be encoded: an error body's `error`.
 */
failure: ErrorDetail | null, progress: Progress | null, meshVersion: number | null, 
/**
 * The revision of the document the mesh is of: the mesh is fetched by it.
 */
docRev: number | null, meshBytes: number | null, sketches: Array<SketchRef>, 
/**
 * While the first build runs: the mesh on show is an earlier one (`kind`, `note`).
 */
shown: Shown | null, };

type TokensReq = { text: string, };

type TokensRes = { classes: Array<string>, pieces: Array<number>, };

type TrashUndoReq = { undo: number, };

type Trashed = { 
/**
 * The stream's revision once the change was told: the page has the change when its stream reaches it.
 */
rev: number, dir: string | null, path: string | null, undo: number, };

type UnwrittenControl = { control: string, text: string, 
/**
 * `text` as words: one string's quotes off, its escapes undone.
 */
plain: string, from: string, };

type WeightingArg = { name: string, default: number | null, least: number, 
/**
 * `least` itself is refused.
 */
above: boolean, 
/**
 * None for no limit.
 */
most: number | null, };

type WeightingJson = { name: string, call: string, about: string, args: Array<WeightingArg>, 
/**
 * The numbers it takes any number of.
 */
rest: WeightingRest | null, };

type WeightingRest = { name: string, least: number, atLeast: number, atMost: number, };

type WordJson = { word: string, about: string, 
/**
 * The numbers the word sets, by control name.
 */
sets: { [key in string]: number }, level: string | null, };

type Written = { text: string, span: [number, number], plain: string, };

type WrittenControl = { name: string, 
/**
 * The whole control as written (`name=value`, or the line `name value`), and its span.
 */
text: string, span: [number, number], 
/**
 * The value alone, as written.
 */
value: string, 
/**
 * The value as words: one string's quotes off, its escapes undone.
 */
plain: string, 
/**
 * The file it is written in, when not the entry's own.
 */
file: string | null, 
/**
 * Written in a part the entry calls: what the sheet says it comes from.
 */
from: string | null, };

type WrittenPlace = { name: string, text: string, 
/**
 * `text` as words: one string's quotes off, its escapes undone.
 */
plain: string, span: [number, number], };

type WrittenPoint = { text: string, span: [number, number], at: [number, number] | null, };

/** Every call made with a body, by its name: its request body and its answer. */
export interface Calls {
  "edit": { req: EditReq; res: EditRes };
  "names": { req: NamesReq; res: NamesRes };
  "tokens": { req: TokensReq; res: TokensRes };
  "checkValue": { req: CheckValueReq; res: CheckValueRes };
  "printedSet": { req: PrintedSetReq; res: PrintedSetRes };
  "newTile": { req: NewReq; res: Listed };
  "reportMessage": { req: PageMessageReq; res: Done };
  "setTarget": { req: TargetReq; res: TargetRes };
  "reveal": { req: RevealReq; res: Done };
  "addPlace": { req: Nothing; res: PlacesAddRes };
  "removePlace": { req: PlacesRemoveReq; res: Changed };
  "newFolder": { req: FolderNamedReq; res: FolderDone };
  "renameFolder": { req: FolderNamedReq; res: FolderDone };
  "trashFolder": { req: FolderReq; res: Trashed };
  "undoTrash": { req: TrashUndoReq; res: Restored };
  "tiersWant": { req: TiersWantReq; res: TiersWantRes };
  "printersScan": { req: PrintersScanReq; res: PrintersScanRes };
  "printersImport": { req: PrintersImportReq; res: PrintersImportRes };
  "platesShow": { req: PlatesShowReq; res: Done };
  "platesSet": { req: PlatesSetReq; res: ListEdited };
  "platesExport": { req: PlatesExportReq; res: PlatesExported };
  "inventoryHave": { req: InventoryHaveReq; res: InventoryWrote };
  "inventoryRecord": { req: InventoryRecordReq; res: InventoryWrote };
  "inventoryGone": { req: InventoryGoneReq; res: InventoryWrote };
  "inventoryRelink": { req: InventoryRelinkReq; res: InventoryWrote };
  "inventoryReprint": { req: InventoryReprintReq; res: InventoryReprinted };
  "listsNew": { req: ListsNewReq; res: Listed };
  "listsAdd": { req: ListsAddReq; res: ListsAddRes };
  "listsCount": { req: ListsCountReq; res: ListEdited };
  "listsRemove": { req: ListsRemoveReq; res: ListEdited };
  "listsRestore": { req: ListsRestoreReq; res: ListEdited };
  "mapsAdd": { req: MapsAddReq; res: MapsPieced };
  "mapsSet": { req: MapsSetReq; res: ListEdited };
  "mapsBatch": { req: MapsBatchReq; res: MapsBatched };
  "mapsDup": { req: MapsDupReq; res: MapsPieced };
  "mapsRemove": { req: MapsPieceReq; res: ListEdited };
  "mapsPin": { req: MapsPinReq; res: MapsPieced };
  "mapsUnpin": { req: MapsPieceReq; res: ListEdited };
  "mapsUnpinAll": { req: MapsListReq; res: ListEdited };
  "check": { req: TextReq; res: CheckRes };
  "printed": { req: PrintedReq; res: PrintedRes };
  "save": { req: SaveReq; res: SaveRes };
  "mine": { req: Nothing; res: Listed };
  "deleteTile": { req: Nothing; res: Deleted };
  "rename": { req: RenameReq; res: Listed };
  "moveTile": { req: TileMoveReq; res: ListedIn };
  "duplicateTile": { req: Nothing; res: ListedIn };
  "trashTile": { req: Nothing; res: Trashed };
  "exportTile": { req: ExportReq; res: ExportRes };
  "offers": { req: OffersReq; res: OffersRes };
  "buildThing": { req: ThingReq; res: ThingRes };
  "draft": { req: DraftReq; res: ArrayBuffer };
  "step": { req: StepReq; res: ArrayBuffer };
}

/** Every read, by its name: its answer. */
export interface Reads {
  "session": SessionRes;
  "about": AboutRes;
  "elements": Array<ElementJson>;
  "language": LanguageJson;
  "snapshot": Snapshot;
  "text": TextRes;
}

/** The request body of call `R`. */
export type Req<R extends keyof Calls> = Calls[R]["req"];

/** The answer of call `R`. */
export type Res<R extends keyof Calls> = Calls[R]["res"];
