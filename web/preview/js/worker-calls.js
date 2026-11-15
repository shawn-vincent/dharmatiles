// How the editor in a web worker (`?host=worker`, host.js's `workerHost`) answers each call of the page's contract: the one table.
//
// Every call there is (js/contract.js's ROUTES, generated from the program's one table of routes) has a row here, so a new call
// cannot be forgotten: `tsc` fails on a row that is missing, and tests/worker-calls.test.js fails on a row that names no call.
// A call is answered by the service in the worker (`service`), or refused in words by name: it needs the program, as a folder
// chooser or showing a file in the file manager does (`program`).  Never answered empty.

import { ROUTES } from './contract.js';

/**
 * @typedef {'service' | 'worker' | { program: string }} InWorker
 * `service`: the worker's service answers it as a verb of its table, by the call's name (`xtask/service.mjs` holds the table to this).
 * `worker`: the worker host answers it itself, in the page (the stream, a mesh, the page's own report of a failure, a sketch: a build
 * here runs inside one call, so there is no sketch of it to fetch while it builds).
 * `program`: what only the native program can do, said as the thing the page asked for (`adding a folder`).
 */

/** @type {Record<keyof typeof ROUTES, InWorker>} */
export const WORKER_CALLS = {
  edit: 'service',
  names: 'service',
  tokens: 'service',
  checkValue: 'service',
  printedSet: 'service',
  newTile: 'service',
  reportMessage: 'worker',
  setTarget: 'service',
  reveal: { program: 'showing a file in the file manager' },
  addPlace: { program: 'adding a folder as a place' },
  removePlace: { program: 'removing a place' },
  newFolder: 'service',
  renameFolder: 'service',
  trashFolder: 'service',
  undoTrash: 'service',
  tiersWant: 'service',
  printersScan: { program: 'reading the slicers installed on this computer' },
  printersImport: { program: 'importing a printer from an installed slicer' },
  platesShow: 'service',
  platesSet: 'service',
  platesExport: 'service',
  inventoryHave: 'service',
  inventoryRecord: 'service',
  inventoryGone: 'service',
  inventoryRelink: 'service',
  inventoryReprint: 'service',
  listsNew: 'service',
  listsAdd: 'service',
  listsCount: 'service',
  listsRemove: 'service',
  listsRestore: 'service',
  mapsAdd: 'service',
  mapsSet: 'service',
  mapsBatch: 'service',
  mapsDup: 'service',
  mapsRemove: 'service',
  mapsPin: 'service',
  mapsUnpin: 'service',
  mapsUnpinAll: 'service',
  check: 'service',
  printed: 'service',
  save: 'service',
  mine: 'service',
  deleteTile: 'service',
  rename: 'service',
  moveTile: 'service',
  duplicateTile: 'service',
  trashTile: 'service',
  exportTile: 'service',
  offers: 'service',
  draft: 'service',
  step: 'service',
  build: 'service',
  session: { program: 'the program\'s session token' },
  about: 'service',
  elements: 'service',
  language: 'service',
  snapshot: 'worker',
  text: 'service',
  mesh: 'worker',
  sketch: 'worker',
  tierResult: 'service',
  events: 'worker',
  messages: 'service',
  messagesReport: 'service',
  testPanic: { program: 'the program\'s own failure test' },
  testIdle: { program: 'the program\'s own idle test' },
  testFailure: { program: 'the program\'s own failure test' },
};

/** The calls the worker's service answers as verbs of its table, by the call's name. */
export const WORKER_VERBS = new Set(
  Object.entries(WORKER_CALLS)
    .filter(([, how]) => how === 'service')
    .map(([name]) => name),
);

/**
 * What the page is told of a call the worker does not answer: what was asked, and that only the program can do it.
 * @param {keyof typeof ROUTES} call @returns {string}
 */
export function refusalOf(call) {
  const how = WORKER_CALLS[call];
  if (typeof how === 'string') throw new Error(`${call} is answered in the worker: it has no refusal`);
  return `${how.program} needs the program (dharmatiles preview): a web page cannot do it`;
}
