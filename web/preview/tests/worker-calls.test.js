import test from 'node:test';
import assert from 'node:assert/strict';
import { ROUTES } from '../js/contract.js';
import { WORKER_CALLS, WORKER_VERBS, refusalOf } from '../js/worker-calls.js';

test('every call of the page\'s contract is answered by the worker\'s service or refused by name, and no row names a call that is not there', () => {
  const calls = Object.keys(ROUTES).sort();
  assert.deepEqual(Object.keys(WORKER_CALLS).sort(), calls);
  for (const [name, how] of Object.entries(WORKER_CALLS)) {
    assert.ok(typeof how === 'string' ? ['service', 'worker'].includes(how) : 'program' in how || 'later' in how, `${name} has a row that is none of the three answers`);
  }
});

test('the verbs the service answers are the rows marked so, and nothing refused or answered by the host is among them', () => {
  for (const name of WORKER_VERBS) assert.equal(WORKER_CALLS[name], 'service', name);
  for (const [name, how] of Object.entries(WORKER_CALLS)) assert.equal(WORKER_VERBS.has(name), how === 'service', name);
  // the page's own report, the stream, a mesh and the snapshot are the worker host's
  assert.deepEqual(Object.entries(WORKER_CALLS).filter(([, how]) => how === 'worker').map(([name]) => name).sort(), ['events', 'mesh', 'reportMessage', 'snapshot']);
});

test('a call the worker does not answer says what was asked and why: the program, or not yet', () => {
  assert.match(refusalOf('addPlace'), /^adding a folder as a place needs the program/);
  assert.match(refusalOf('reveal'), /^showing a file in the file manager needs the program/);
  assert.match(refusalOf('platesExport'), /^the plates is not in the browser editor yet/);
  assert.throws(() => refusalOf('check'), /answered in the worker/);
});

test('the program-only calls are the ones only a native program can do', () => {
  const program = Object.entries(WORKER_CALLS).filter(([, how]) => typeof how !== 'string' && 'program' in how).map(([name]) => name).sort();
  assert.deepEqual(program, ['addPlace', 'printersImport', 'printersScan', 'removePlace', 'reveal', 'session', 'testFailure', 'testIdle', 'testPanic']);
});
