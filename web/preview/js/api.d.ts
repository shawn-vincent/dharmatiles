// Generated from crates/dharmatiles/src/preview (api.rs and the types it names) by
// `DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current`.
// Do not edit: a test fails when this file is not what the Rust types say.

type AboutRes = { version: string, formats: string, };

type Changed = { snapshot: Snapshot, };

type CheckRes = { ok: boolean, errors: Array<Mistake>, stack: Array<JsonValue> | null, map: JsonValue | null, };

type CheckValueReq = { element: string, control: string, typed: string, };

type CheckValueRes = { message: string | null, label: string | null, help: string | null, };

type ColourWord = "single" | "multi";

type Deleted = { path: string, };

type Done = Record<string, never>;

type DraftReq = { text: string, file?: string | null, debug?: boolean | null, };

type EditReq = { "op": "set", text: string, at: number, name: string, value?: Scalar | null, } | { "op": "insert", text: string, node: string, place?: InsertPlace | null, at?: number | null, } | { "op": "move", text: string, at: number, before?: number | null, } | { "op": "remove", text: string, at: number, } | { "op": "remove-children", text: string, at: number, names: Array<string>, } | { "op": "rename", text: string, mark: MarkKind, old: string, new: string, } | { "op": "arg", text: string, at: number, index: number, value: Scalar, };

type EditRes = { text: string, at: number | null, };

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
 * Whether the build passed the printability gate.
 */
ok: boolean, failures: Array<string>, files: Array<string>, 
/**
 * The build's record as `dharmatiles build --json` prints it.
 */
build: JsonValue, };

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

type FolderDone = { snapshot: Snapshot, dir: string, };

type FolderJson = { root: number, path: string, };

type FolderNamedReq = { root: number, dir: string, name: string, };

type FolderReq = { root: number, 
/**
 * The folder, by its path below the place (`""` is the place).
 */
dir: string, };

type HeightName = { name: string, dmm: number, own: boolean, };

type InsertPlace = "stack" | "tile" | "before" | "in";

type InventoryGoneReq = { tile: string, seed: number, system: SystemWord, colour: ColourWord, note?: string | null, on?: string | null, };

type InventoryHaveReq = { tile: string, colour: ColourWord, system?: SystemWord | null, seed?: number | null, count?: number | null, 
/**
 * The date, like 2026-11-09 (today when left out).
 */
on?: string | null, };

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

type InventoryReprintReq = { tile: string, seed: number, system: SystemWord, colour: ColourWord, 
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

type InventoryWrote = { lines: Array<string>, file: string, };

type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;

type ListEdited = { 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type Listed = { path: string, id: string | null, tile: TileSummary | null, };

type ListedIn = { path: string, id: string | null, tile: TileSummary | null, snapshot: Snapshot, };

type ListsAddReq = { tile: string, 
/**
 * The list's id; none makes a new list.
 */
list?: string | null, seed?: number | null, system?: string | null, 
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

type MapsAddReq = { list: string, tile: string, at: string, rot?: number | null, seed?: number | null, system?: string | null, docRev: number, };

type MapsBatchReq = { list: string, edits: Array<PieceEdit>, docRev: number, };

type MapsBatched = { changed: number, 
/**
 * The list's id.
 */
list: string, path: string, before: string | null, text: string, docRev: number, scene: SceneJson | null, 
/**
 * Said once: the pieces were given ids.
 */
note: string | null, };

type MapsDupReq = { list: string, id: string, at: string, docRev: number, };

type MapsListReq = { list: string, docRev: number, };

type MapsPieceReq = { list: string, id: string, docRev: number, };

type MapsPieced = { id: string, 
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

type MarkKind = "line" | "region";

type Mistake = { code: string, message: string, file: string | null, line: number | null, column: number | null, help: string | null, 
/**
 * A failure of the program, not of the document: shown as a bug.
 */
unexpected: boolean, };

type NamesReq = { text: string, };

type NamesRes = { ok: boolean, message: string | null, defs: Array<string>, regions: Array<string>, lines: Array<string>, points: Array<string>, };

type NewReq = { name?: string | null, 
/**
 * Squares across and up, like `2x1`.
 */
size?: string | null, 
/**
 * The folder place and the folder in it, when not My tiles.
 */
root?: number | null, dir?: string | null, };

type Nothing = Record<string, never>;

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

type PageMessageReq = { tile?: string | null, what?: string | null, text: string, help?: string | null, causes?: JsonValue | null, backtrace?: string | null, };

type PieceEdit = { id: string, set: PieceWords, };

type PieceWords = { 
/**
 * Where, as written: `(2sq, 1sq)`.
 */
at?: string | null, 
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

type PlaceAdded = { added: boolean, root: number, label: string, path: string, snapshot: Snapshot, };

type PlacesAddRes = { cancelled: boolean, } | PlaceAdded;

type PlacesRemoveReq = { 
/**
 * The place, by its index in `roots`.
 */
root: number, };

type PlatesExportReq = { list: string, };

type PlatesExported = { files: Array<string>, format: string, 
/**
 * The pieces that fail the printability gate, by tile.
 */
failures: Array<string>, plates: number, totalS: number, out: string, };

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

type PrintersImportReq = { slicer: string, 
/**
 * The profile names of the models to write.
 */
names: Array<string>, force?: boolean | null, };

type PrintersImportRes = { written: Array<Written>, };

type PrintersScanReq = { slicer?: string | null, };

type PrintersScanRes = { slicers: Array<SlicerFound>, } | ScanJson;

type Progress = { fraction: number, label: string, key: string, 
/**
 * What the stage is counting: `[what, done, total]`; a total of 0 is just the step.
 */
count: [string, number, number] | null, elapsedS: number, etaS: number | null, estimateS: number | null, };

type RenameReq = { name: string, };

type Restored = { path: string, id: string | null, tile: TileSummary | null, snapshot: Snapshot, };

type RevealReq = { path: string, };

type RootJson = { index: number, label: string, 
/**
 * `files`, `dir` or `library`.
 */
kind: string, path: string | null, role: string, writable: boolean, exists: boolean, };

type SaveReq = { text: string, 
/**
 * The revision of the file the text was made from.
 */
docRev: number, file?: string | null, };

type SaveRes = { docRev: number, };

type Scalar = boolean | number | number | string;

type ScanJson = { slicer: string, title: string, version: string, label: string, models: Array<JsonValue>, 
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
 * `single` or `multi`.
 */
colour: string, system: string, printer: string, };

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
tileId: string | null, path: string, seed: number | null, seedUsed: number, system: string, colour: string, cols: number, rows: number, 
/**
 * One square, mm.
 */
sq: number, 
/**
 * Width and depth, mm.
 */
size: [number, number], at: [number, number] | null, rot: number | null, bottomMm: number, bottom: string | null, print: string, count: number, pinned: boolean, error: string | null, };

type SessionRes = { token: string, };

type SketchRef = { id: number, rough: boolean, };

type SlicerFound = { slicer: string, label: string, title: string, };

type Snapshot = { 
/**
 * Always `snapshot`: the stream's event kind.
 */
type: string, rev: number, epoch: string, system: string, printer: string, printers: Array<JsonValue>, overhangDeg: number, steepestDeg: number, roots: Array<RootJson>, folders: Array<FolderJson>, home: string | null, inventory: JsonValue, tiles: Array<TileSummary>, };

type SystemWord = "db" | "ol";

type TargetReq = { system?: string | null, printer?: string | null, };

type TargetRes = { system: string, printer: string, };

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

type TileMoveReq = { root: number, dir: string, };

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
 * What the plan pass or the plates' layout said (the page reads it by name; see `js/doc-views.js`).
 */
plan: JsonValue | null, 
/**
 * The build's record: audit, stages, timings.
 */
record: JsonValue | null, errors: Array<Mistake>, 
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
shown: JsonValue | null, };

type TrashUndoReq = { undo: number, };

type Trashed = { snapshot: Snapshot, dir: string | null, path: string | null, undo: number, };

type Written = { name: string, 
/**
 * The user's own profile of that name was kept.
 */
kept: boolean, };

/** Every call that writes: its request body and its answer. */
export interface Calls {
  "POST /api/edit": { req: EditReq; res: EditRes };
  "POST /api/names": { req: NamesReq; res: NamesRes };
  "POST /api/check-value": { req: CheckValueReq; res: CheckValueRes };
  "POST /api/new": { req: NewReq; res: Listed };
  "POST /api/messages": { req: PageMessageReq; res: Done };
  "POST /api/target": { req: TargetReq; res: TargetRes };
  "POST /api/reveal": { req: RevealReq; res: Done };
  "POST /api/places/add": { req: Nothing; res: PlacesAddRes };
  "POST /api/places/remove": { req: PlacesRemoveReq; res: Changed };
  "POST /api/folders/new": { req: FolderNamedReq; res: FolderDone };
  "POST /api/folders/rename": { req: FolderNamedReq; res: FolderDone };
  "POST /api/folders/trash": { req: FolderReq; res: Trashed };
  "POST /api/trash/undo": { req: TrashUndoReq; res: Restored };
  "POST /api/printers/scan": { req: PrintersScanReq; res: PrintersScanRes };
  "POST /api/printers/import": { req: PrintersImportReq; res: PrintersImportRes };
  "POST /api/plates/show": { req: PlatesShowReq; res: Done };
  "POST /api/plates/set": { req: PlatesSetReq; res: ListEdited };
  "POST /api/plates/export": { req: PlatesExportReq; res: PlatesExported };
  "POST /api/inventory/have": { req: InventoryHaveReq; res: InventoryWrote };
  "POST /api/inventory/record": { req: InventoryRecordReq; res: InventoryWrote };
  "POST /api/inventory/gone": { req: InventoryGoneReq; res: InventoryWrote };
  "POST /api/inventory/relink": { req: InventoryRelinkReq; res: InventoryWrote };
  "POST /api/inventory/reprint": { req: InventoryReprintReq; res: InventoryReprinted };
  "POST /api/lists/new": { req: ListsNewReq; res: Listed };
  "POST /api/lists/add": { req: ListsAddReq; res: ListsAddRes };
  "POST /api/lists/count": { req: ListsCountReq; res: ListEdited };
  "POST /api/lists/remove": { req: ListsRemoveReq; res: ListEdited };
  "POST /api/lists/restore": { req: ListsRestoreReq; res: ListEdited };
  "POST /api/maps/add": { req: MapsAddReq; res: MapsPieced };
  "POST /api/maps/set": { req: MapsSetReq; res: ListEdited };
  "POST /api/maps/batch": { req: MapsBatchReq; res: MapsBatched };
  "POST /api/maps/dup": { req: MapsDupReq; res: MapsPieced };
  "POST /api/maps/remove": { req: MapsPieceReq; res: ListEdited };
  "POST /api/maps/pin": { req: MapsPinReq; res: MapsPieced };
  "POST /api/maps/unpin": { req: MapsPieceReq; res: ListEdited };
  "POST /api/maps/unpin-all": { req: MapsListReq; res: ListEdited };
  "POST /api/tiles/:id/check": { req: TextReq; res: CheckRes };
  "POST /api/tiles/:id/save": { req: SaveReq; res: SaveRes };
  "POST /api/tiles/:id/mine": { req: Nothing; res: Listed };
  "POST /api/tiles/:id/delete": { req: Nothing; res: Deleted };
  "POST /api/tiles/:id/rename": { req: RenameReq; res: Listed };
  "POST /api/tiles/:id/move": { req: TileMoveReq; res: ListedIn };
  "POST /api/tiles/:id/duplicate": { req: Nothing; res: ListedIn };
  "POST /api/tiles/:id/trash": { req: Nothing; res: Trashed };
  "POST /api/tiles/:id/export": { req: ExportReq; res: ExportRes };
  "POST /api/tiles/:id/offers": { req: OffersReq; res: OffersRes };
  "POST /api/tiles/:id/draft": { req: DraftReq; res: ArrayBuffer };
}

/** Every read: its answer. */
export interface Reads {
  "GET /api/session": SessionRes;
  "GET /api/about": AboutRes;
  "GET /api/tiles": Snapshot;
  "GET /api/tiles/:id/text": TextRes;
}

/** The request body of call `R`. */
export type Req<R extends keyof Calls> = Calls[R]["req"];

/** The answer of call `R`. */
export type Res<R extends keyof Calls> = Calls[R]["res"];
