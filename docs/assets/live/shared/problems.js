// Unexpected failures in the page (docs/standards/errors.html): something
// broke that is not a mistake in the tile.  This is the one place every such
// failure goes through, whether it began in the page's JavaScript or in the
// program's Rust: script errors, unhandled rejections, store subscribers
// that throw, replies that failed, builds that panicked.  Each is kept with
// what was being done, how to say it to the user, the whole chain of errors
// under it (the page's, then the server's with its causes and backtrace) and
// a plain-text report to copy into a bug report.  The messages panel
// (editor/panel.js) draws them, from the program's stream of messages.  The console gets each too, but is never the
// only place.
//
//   showError(what, err)    one failure: "Could not save 1x1-cliff", the error; also reported to the stream of messages (setReporter)
//   failed(what, result)    true (and shown) for a reply that is an internal failure
//   installGlobalHandlers() script errors and unhandled rejections land here too
//   detailsOf(what, err)    the report: what, the chain with every stack, the environment
//
// A lost connection is not a problem: the connection chip shows it and the
// page tries again.

import { isServerError, serverErrorOf } from './server-error.js';

const entries = [];
const listeners = new Set();

/** What an error says, in its own words. */
export function messageOf(err) {
  if (err instanceof Error) return err.message || err.name;
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && typeof err.error === 'string') return err.error;
  try { return JSON.stringify(err) ?? String(err); } catch { return String(err); }
}

const BUG_HEADLINE = 'Something went wrong inside dharmatiles';

/** The first of the server's errors in `err`'s chain of causes, or null. */
export function serverErrorIn(err) {
  let e = err, depth = 0;
  while (e != null && depth++ < 12) {
    if (isServerError(e)) return e;
    e = e instanceof Error ? e.cause : null;
  }
  return null;
}

/**
 * How the failure is said to the user: {kind, headline, help, detail}.
 * The server's "failure" and "refused" have reasonable words of their own,
 * which are the headline.  A bug (a server panic, or anything the page itself
 * threw: a TypeError, a bad reply) is just "something went wrong inside
 * dharmatiles", with what was being done, and the words in `detail`.
 */
export function describe(err) {
  const se = serverErrorIn(err);
  if (se && se.kind !== 'bug') return { kind: se.kind, headline: se.message, help: se.help, detail: null };
  return { kind: 'bug', headline: BUG_HEADLINE, help: se?.help ?? null, detail: se ? se.message : rootMessage(err) };
}

/** The deepest error's words: what actually went wrong, under the page's "could not …". */
function rootMessage(err) {
  let e = err, last = messageOf(err), depth = 0;
  while (e instanceof Error && e.cause != null && depth++ < 12) { e = e.cause; last = messageOf(e); }
  return last;
}

// The lines of a JS stack without its first line when that only repeats
// "Name: message" (a message of several lines starts the stack with all of them).
function stackLines(e) {
  const stack = typeof e.stack === 'string' ? e.stack : '';
  const head = `${e.name}: ${e.message}`;
  const body = stack.startsWith(head) ? stack.slice(head.length) : stack.split('\n').slice(1).join('\n');
  return body.split('\n').map((l) => l.trimEnd()).filter(Boolean);
}

/**
 * The chain of errors in `err`, outermost first, each a level to show and to
 * copy: {side: 'page' | 'server' | 'other', title, message, help, context,
 * causes, trace}.  A page error's `trace` is its JS stack; a server error's is
 * the Rust backtrace, and it brings its own causes and what it was doing.
 */
export function chainOf(err) {
  const levels = [];
  let e = err, depth = 0;
  while (e != null && depth++ < 12) {
    if (isServerError(e)) {
      const c = e.context || {};
      const where = [c.route, c.tile ? `tile ${c.tile}` : null].filter(Boolean).join(', ');
      levels.push({
        side: 'server', title: `The server's error (${e.kind}${e.status ? `, ${e.status}` : ''})`, message: e.message,
        help: e.help, context: where || null, causes: e.causes, trace: e.backtrace ? e.backtrace.split('\n').map((l) => l.trimEnd()).filter(Boolean) : [],
      });
    } else if (e instanceof Error) {
      levels.push({
        side: 'page', title: levels.length ? `${e.name} in the page` : `The page: ${e.name}`, message: e.message,
        help: null, context: null, causes: [], trace: stackLines(e),
      });
    } else {
      levels.push({ side: 'other', title: 'Thrown value', message: messageOf(e), help: null, context: null, causes: [], trace: [] });
    }
    e = e instanceof Error ? e.cause : null;
  }
  return levels;
}

/**
 * What the report says about where it happened, from the page's own globals
 * (`g`, so a test can give it fake ones) and what the editor knows
 * (`setEnvironment`): version, target, tile, file, browser, screen, time, URL.
 */
export function collectEnvironment(g = globalThis, known = {}, now = new Date()) {
  const nav = g.navigator || {}, scr = g.screen || {}, loc = g.location || {};
  return {
    version: known.version ?? null,
    system: known.system ?? null,
    printer: known.printer ?? null,
    tile: known.tile ?? null,
    file: known.file ?? null,
    userAgent: nav.userAgent ?? null,
    platform: nav.userAgentData?.platform || nav.platform || null,
    screen: scr.width ? `${scr.width}x${scr.height} at ${g.devicePixelRatio || 1}x, window ${g.innerWidth}x${g.innerHeight}` : null,
    time: now.toISOString(),
    url: loc.href ?? null,
  };
}

let known = () => ({});
/** The editor says what it knows (`fn()` -> {version, system, printer, tile, file}); read when a failure happens. */
export function setEnvironment(fn) { known = fn; }

const indent = (lines, by) => lines.map((l) => `${by}${l}`);

/**
 * The report to paste into a bug report: what was being done, the whole
 * chain with both stacks, and the environment.  Plain text.
 */
export function detailsOf(what, err, env = collectEnvironment(globalThis, known())) {
  const d = describe(err), se = serverErrorIn(err);
  const out = ['dharmatiles error report', '', `What:     ${what}`, `Said:     ${d.headline}`];
  if (d.detail) out.push(`Details:  ${d.detail}`);
  out.push(`Kind:     ${d.kind}`, '');
  out.push(...environmentLines(env, env.version ?? se?.context?.version ?? null));
  out.push('', 'Chain of errors, outermost first');
  chainOf(err).forEach((l, i) => {
    out.push('', `[${i + 1}] ${l.title}`, `    ${l.message}`);
    if (l.help) out.push(`    help: ${l.help}`);
    if (l.context) out.push(`    while: ${l.context}`);
    if (l.causes.length) out.push('    caused by:', ...indent(l.causes.map((c) => `- ${c}`), '      '));
    if (l.trace.length) out.push(`    ${l.side === 'server' ? 'backtrace (Rust)' : 'stack (JavaScript)'}:`, ...indent(l.trace, '      '));
    else if (l.side === 'server') out.push('    (the server sent no backtrace: this is a refusal it made without one)');
  });
  return out.join('\n');
}

/** The environment block of a report: version, target, tile, browser, screen, time, page. */
function environmentLines(env, version) {
  const rows = [
    ['dharmatiles', version], ['target system', env.system], ['printer', env.printer], ['tile', env.tile], ['file', env.file],
    ['browser', env.userAgent], ['platform', env.platform], ['screen', env.screen], ['time', env.time], ['page', env.url],
  ];
  return ['Environment', ...rows.map(([k, v]) => `  ${k.padEnd(14)}${v ?? 'unknown'}`)];
}

/**
 * The report for one of the tile's own errors in the panel (a mistake in the
 * tile, or a bug the build met while building it): its code, where, the
 * words and help, the source line it points at when known, and the
 * environment.  Plain text, for the clipboard.
 */
export function tileErrorDetails(e, { source = null, env = collectEnvironment(globalThis, known()) } = {}) {
  const out = [e.unexpected ? 'dharmatiles bug report' : 'dharmatiles tile error', ''];
  out.push(`Code:     ${e.code}`);
  if (e.file) out.push(`Where:    ${e.file}:${e.line}:${e.column}`);
  out.push(`Said:     ${e.message}`);
  if (e.help) out.push(`Help:     ${e.help}`);
  out.push(`Kind:     ${e.unexpected ? 'bug (not a mistake in the tile)' : 'a mistake in the tile'}`);
  if (source != null && e.line) {
    const lines = source.split('\n'), at = e.line - 1;
    if (at >= 0 && at < lines.length) {
      out.push('', 'Source');
      for (let i = Math.max(0, at - 2); i <= Math.min(lines.length - 1, at + 2); i++) {
        out.push(`${i === at ? '>' : ' '} ${String(i + 1).padStart(4)} | ${lines[i]}`);
      }
    }
  }
  out.push('', ...environmentLines(env, env.version ?? null));
  return out.join('\n');
}

/** The problems now, oldest first: {what, message, kind, headline, help, detail, chain, details, count, open}. */
export const problems = () => entries;

/** Tell `fn` whenever the list changes; returns the way to stop. */
export function onProblems(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

const changed = () => { for (const fn of listeners) fn(entries); };

let reporter = null;
/**
 * The editor says where a failure of the page's own goes besides here: `fn(what, err, entry)`
 * (editor/panel.js puts it in the program's stream of messages, where the messages panel
 * shows it and an assistant reads it).  Called once for each new failure.
 */
export function setReporter(fn) { reporter = fn; }

/** Put one failure in the list of problems (and the stream): `what` was being done, `err` is what went wrong. */
export function showError(what, err) {
  console.error(what, err);
  const d = describe(err);
  const message = serverErrorIn(err)?.message ?? messageOf(err);
  const same = entries.find((p) => p.what === what && p.message === message);
  if (same) same.count += 1;
  else {
    const env = collectEnvironment(globalThis, known());
    const entry = { what, message, ...d, chain: chainOf(err), details: detailsOf(what, err, env), count: 1, open: false };
    entries.push(entry);
    reporter?.(what, err, entry);
  }
  changed();
}

/** Take one entry (or, with no index, all of them) off the panel. */
export function dismiss(entry) {
  if (entry) entries.splice(entries.indexOf(entry), 1);
  else entries.length = 0;
  changed();
}

/**
 * What a reply that is not a success says, in words, for where it was asked: a
 * document's first mistake with its help (a 422's `errors`), else the program's
 * message.  The one way the page reads a refusal.
 */
export function whyOf(r) {
  const e = r?.body?.errors?.[0];
  if (e) return `${e.message}${e.help ? `. ${e.help}` : ''}`;
  const se = r?.error;
  if (se?.message) return se.help ? `${se.message}. ${se.help}` : se.message;
  const b = r?.body?.error;
  return typeof b === 'string' ? b : b?.message || 'that could not be done';
}

/**
 * A reply from `host` ({status, body, error}) that is an internal failure
 * goes to the panel, and `failed` is true then: the program's 5xx, or a
 * refusal that gives no reason (the page asked something the program does not
 * understand).  A refusal with its reason (4xx, "a tile name is one line") is
 * for the caller to show where it was asked, and a lost connection (status
 * 0) is the chip's.  The panel's entry is the page's own error for `what`
 * with the program's error under it as its cause.
 */
export function failed(what, r) {
  if (r.status === 0) return false;
  if (r.status < 500 && !(r.status >= 400 && r.body == null)) return false;
  const cause = r.error ?? serverErrorOf(r.status, r.body);
  showError(what, new Error(what, { cause }));
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
