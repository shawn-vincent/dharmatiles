// @ts-check
// The steps of a change to a document (js/documents.js `edit`): one call of the program, named, with its
// body less the two things the document supplies itself, the list it is made to and the revision it is made
// against.  Each step is built by one of these, so the body of every step is checked against the contract
// (js/api.d.ts) where it is written, not discovered when the call fails.

/** @template {keyof import('./api').Calls} R @typedef {Omit<import('./api').Req<R>, 'list' | 'docRev'>} Body */

export const steps = {
  /** @param {Body<'listsCount'>} body */
  listsCount: (body) => ({ name: /** @type {const} */ ('listsCount'), body }),
  /** @param {Body<'listsRemove'>} body */
  listsRemove: (body) => ({ name: /** @type {const} */ ('listsRemove'), body }),
  /** @param {Body<'mapsAdd'>} body */
  mapsAdd: (body) => ({ name: /** @type {const} */ ('mapsAdd'), body }),
  /** @param {Body<'mapsSet'>} body */
  mapsSet: (body) => ({ name: /** @type {const} */ ('mapsSet'), body }),
  /** @param {Body<'mapsBatch'>} body */
  mapsBatch: (body) => ({ name: /** @type {const} */ ('mapsBatch'), body }),
  /** @param {Body<'mapsDup'>} body */
  mapsDup: (body) => ({ name: /** @type {const} */ ('mapsDup'), body }),
  /** @param {Body<'mapsRemove'>} body */
  mapsRemove: (body) => ({ name: /** @type {const} */ ('mapsRemove'), body }),
  /** @param {Body<'mapsPin'>} body */
  mapsPin: (body) => ({ name: /** @type {const} */ ('mapsPin'), body }),
  /** @param {Body<'mapsUnpin'>} body */
  mapsUnpin: (body) => ({ name: /** @type {const} */ ('mapsUnpin'), body }),
  /** @param {Body<'mapsUnpinAll'>} body */
  mapsUnpinAll: (body) => ({ name: /** @type {const} */ ('mapsUnpinAll'), body }),
  /** @param {Body<'platesSet'>} body */
  platesSet: (body) => ({ name: /** @type {const} */ ('platesSet'), body }),
};
