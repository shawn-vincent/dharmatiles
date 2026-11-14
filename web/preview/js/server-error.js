// The server's error as the page holds it.  `dharmatiles preview` answers
// every failure with one shape (crates/dharmatiles/src/preview/fail.rs):
//
//   {"error": {kind, message, help, causes, backtrace, context: {route, tile, version, atMs}}}
//
//   kind  'refused'  the request was wrong; shown where it was made
//         'failure'  the program tried and could not; the message is the headline
//         'bug'      a panic in dharmatiles: "something went wrong inside"
//
// `ServerError` carries those fields.  The page's own error (what it was
// doing) holds it as its `cause`, so one error has both stacks; problems.js
// shows and copies the whole chain.

export class ServerError extends Error {
  constructor(data, status = null) {
    super(data.message);
    this.name = 'ServerError';
    this.kind = data.kind;
    this.help = data.help ?? null;
    this.causes = Array.isArray(data.causes) ? data.causes : [];
    this.backtrace = data.backtrace ?? '';
    this.context = data.context ?? {};
    this.status = status;
    // the page's stack at this line says nothing about the server: the
    // server's own is `backtrace`
    this.stack = '';
  }
}

/**
 * A build that stopped on a mistake in the tile's own document (a `dt::` code, `dt::seam` say), as it reaches every caller that
 * wanted a picture or a piece of it: the tile's mistake to show where the picture would be, never a failure of the page or the
 * program.  `showError` (problems.js) does not list it as a failure: the program has already said it in the stream of
 * messages, at the tile, with its place and help.
 */
export class TileMistake extends Error {
  constructor(message, code = null) {
    super(message);
    this.name = 'TileMistake';
    this.mistake = true;
    this.code = code;
  }
}

export const isServerError = (e) => e instanceof ServerError;

const shaped = (x) => x && typeof x === 'object' && typeof x.message === 'string' && ['refused', 'failure', 'bug'].includes(x.kind);

/**
 * The error a reply says, as a ServerError, or null when it says none.
 * `body` is the parsed JSON (or null); `raw` the reply's text.  A reply
 * without the structure (the program's plain-text refusals, a proxy's page)
 * becomes one with the words it has and no trace, so there is one kind of
 * error to handle either way.
 */
export function serverErrorOf(status, body, raw = '') {
  if (status >= 200 && status < 400) return null;
  const e = body?.error;
  if (shaped(e)) return new ServerError(e, status);
  const words = typeof e === 'string' ? e : (raw || `the program answered ${status}`);
  return new ServerError({ kind: status >= 500 ? 'failure' : 'refused', message: words, causes: [], backtrace: '', context: {} }, status);
}
