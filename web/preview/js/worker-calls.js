// How the editor in a web worker (`?host=worker`, host.js's `workerHost`) answers each call of the page's contract: the one table.
//
// Every call there is (js/contract.js's ROUTES, generated from the program's one table of routes) has a row here, so a new call
// cannot be forgotten: `tsc` fails on a row that is missing, and tests/worker-calls.test.js fails on a row that names no call.
// A call is answered by the service in the worker (`service`), or refused in words by name: it needs the program, as a folder
// chooser or showing a file in the file manager does (`program`), or it is not answered in the worker yet (`later`).  Never
// answered empty.

import { ROUTES } from './contract.js';

/**
 * @typedef {'service' | { program: string } | { later: string }} InWorker
 * `service`: the worker's service answers it (a verb by its name, or the stream, a mesh, a read the worker host answers itself).
 * `program`: what only the native program can do, said as the thing the page asked for (`adding a folder`).
 * `later`: not answered in the worker yet, said the same way.
 */

/** @type {Record<keyof typeof ROUTES, InWorker>} */
export const WORKER_CALLS = {
  edit: 'service',
  names: 'service',
  tokens: 'service',
  checkValue: 'service',
  printedSet: 'service',
  newTile: 'service',
  reportMessage: 'service',
  setTarget: 'service',
  reveal: { program: 'showing a file in the file manager' },
  addPlace: { program: 'adding a folder as a place' },
  removePlace: { program: 'removing a place' },
  newFolder: 'service',
  renameFolder: 'service',
  trashFolder: 'service',
  undoTrash: 'service',
  tiersWant: { later: 'the map\'s tiers' },
  printersScan: { program: 'reading the slicers installed on this computer' },
  printersImport: { program: 'importing a printer from an installed slicer' },
  platesShow: { later: 'the plates' },
  platesSet: { later: 'the plates' },
  platesExport: { later: 'the plates' },
  inventoryHave: { later: 'the inventory' },
  inventoryRecord: { later: 'the inventory' },
  inventoryGone: { later: 'the inventory' },
  inventoryRelink: { later: 'the inventory' },
  inventoryReprint: { later: 'the inventory' },
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
  exportTile: { later: 'writing an STL or a 3MF' },
  offers: 'service',
  draft: 'service',
  step: 'service',
  build: 'service',
  session: { program: 'the program\'s session token' },
  about: 'service',
  elements: 'service',
  language: 'service',
  snapshot: 'service',
  text: 'service',
  mesh: 'service',
  sketch: { later: 'a build\'s sketches' },
  tierResult: { later: 'the map\'s tiers' },
  events: 'service',
  messages: { later: 'the messages as text' },
  messagesReport: { later: 'a defect report' },
  testPanic: { program: 'the program\'s own failure test' },
  testIdle: { program: 'the program\'s own idle test' },
  testFailure: { program: 'the program\'s own failure test' },
};

/** The calls the worker's service answers as verbs by their name: the ones made with a body (a POST). */
export const WORKER_VERBS = new Set(
  Object.entries(WORKER_CALLS)
    .filter(([name, how]) => how === 'service' && ROUTES[/** @type {keyof typeof ROUTES} */ (name)][0] === 'POST')
    .map(([name]) => name),
);

/**
 * What the page is told of a call the worker does not answer: what was asked, and why (the program, or not yet).
 * @param {keyof typeof ROUTES} call @returns {string}
 */
export function refusalOf(call) {
  const how = WORKER_CALLS[call];
  if (how === 'service') throw new Error(`${call} is answered by the worker's service: it has no refusal`);
  return 'program' in how
    ? `${how.program} needs the program (dharmatiles preview): a web page cannot do it`
    : `${how.later} is not in the browser editor yet: it needs the program (dharmatiles preview)`;
}
