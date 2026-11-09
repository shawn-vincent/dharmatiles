// Unexpected failures in the page (docs/standards/errors.html): something
// broke that is not a mistake in the tile.  Each is kept with what was being
// done, the error's own words and its details (to copy), and shown in the
// error panel (editor/export.js draws it).  The console gets it too, but is
// never the only place.
//
//   showError(what, err)    one failure: "Could not save 1x1-cliff", the error
//   failed(what, result)    true (and shown) for a server reply that is an internal failure
//   installGlobalHandlers() script errors and unhandled rejections land here too
//
// A lost connection is not a problem: the connection chip shows it and the
// page tries again.

const entries = [];
const listeners = new Set();

/** What an error says, in its own words. */
export function messageOf(err) {
  if (err instanceof Error) return err.message || err.name;
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && typeof err.error === 'string') return err.error;
  try { return JSON.stringify(err) ?? String(err); } catch { return String(err); }
}

/** The details to copy: what, the words, the chain of causes and the stack. */
export function detailsOf(what, err) {
  const lines = [what];
  let e = err, depth = 0;
  while (e != null && depth++ < 8) {
    lines.push(e instanceof Error ? `${e.name}: ${e.message}` : messageOf(e));
    if (e instanceof Error && e.stack) lines.push(e.stack.split('\n').slice(1).join('\n'));
    e = e instanceof Error ? e.cause : null;
    if (e != null) lines.push('caused by:');
  }
  return lines.filter(Boolean).join('\n');
}

/** The problems now, oldest first: {what, message, details, count}. */
export const problems = () => entries;

/** Tell `fn` whenever the list changes; returns the way to stop. */
export function onProblems(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

const changed = () => { for (const fn of listeners) fn(entries); };

/** Put one failure in the error panel: `what` was being done, `err` is what went wrong. */
export function showError(what, err) {
  console.error(what, err);
  const message = messageOf(err);
  const same = entries.find((p) => p.what === what && p.message === message);
  if (same) same.count += 1;
  else entries.push({ what, message, details: detailsOf(what, err), count: 1 });
  changed();
}

/** Take one entry (or, with no index, all of them) off the panel. */
export function dismiss(entry) {
  if (entry) entries.splice(entries.indexOf(entry), 1);
  else entries.length = 0;
  changed();
}

/**
 * A reply from `host` ({status, body}) that is an internal failure goes to
 * the panel, and `failed` is true then: the program's 5xx, or a refusal that
 * gives no reason (the page asked something the program does not
 * understand).  A refusal with its reason (4xx, "a tile name is one line") is
 * for the caller to show where it was asked, and a lost connection (status
 * 0) is the chip's.
 */
export function failed(what, r) {
  if (r.status === 0) return false;
  if (r.status < 500 && !(r.status >= 400 && r.body == null)) return false;
  showError(what, r.body?.error || `the program answered ${r.status}`);
  return true;
}

// Chrome says this when a resize observer's callback changes the layout it
// observes; it is the browser's own notice and nothing is wrong.
const BENIGN = /^ResizeObserver loop/;

/** Script errors and unhandled promise rejections go to the panel (once; at load). */
export function installGlobalHandlers(target = globalThis) {
  target.addEventListener('error', (e) => {
    if (BENIGN.test(e.message || '')) return;
    showError('The page hit a script error', e.error ?? e.message);
  });
  target.addEventListener('unhandledrejection', (e) => {
    showError('The page hit an unhandled failure', e.reason);
  });
}
