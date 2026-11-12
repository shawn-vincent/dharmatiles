// Making a library tile yours is not a question: the first change to it copies
// it into My tiles and is made there, so the change is never lost (editor/tiles.js
// does the copy).  What is pure about that is here: a change as a patch over the
// text it was made on, and the same patch made on the copy.
//
// The copy's text is the library tile's with at most its `use` lines pinned, so
// a position in the one is found in the other by the text both share before it
// and after it.

/** The change from `before` to `after` as {from, to, insert} over `before`: the middle that differs. */
export function patchOf(before, after) {
  let a = 0;
  while (a < before.length && a < after.length && before[a] === after[a]) a++;
  let b = 0;
  while (b < before.length - a && b < after.length - a && before[before.length - 1 - b] === after[after.length - 1 - b]) b++;
  return { from: a, to: before.length - b, insert: after.slice(a, after.length - b) };
}

/** Where position `x` of `lib` is in `copy`, or null when it is inside what the two do not share. */
export function mapPos(x, lib, copy) {
  if (lib === copy) return x;
  let a = 0;
  while (a < lib.length && a < copy.length && lib[a] === copy[a]) a++;
  let b = 0;
  while (b < lib.length - a && b < copy.length - a && lib[lib.length - 1 - b] === copy[copy.length - 1 - b]) b++;
  if (x <= a) return x;
  if (x >= lib.length - b) return x + (copy.length - lib.length);
  return null;
}

/**
 * `patch` (over `lib`) made on `copy`: {text, caret} (the caret just after what was written),
 * or null when the patch touches what the copy changed (nothing is guessed).
 */
export function patchOnCopy(patch, lib, copy) {
  const from = mapPos(patch.from, lib, copy), to = mapPos(patch.to, lib, copy);
  if (from == null || to == null || to < from) return null;
  return { text: copy.slice(0, from) + patch.insert + copy.slice(to), caret: from + patch.insert.length };
}

/** The line the toast says: where the copy is and that the change is in it. */
export function copiedNote(name, applied) {
  if (applied == null) return `Copied to My tiles as ${name}.`;
  return applied
    ? `Copied to My tiles as ${name}. Your change is in the copy.`
    : `Copied to My tiles as ${name}. Make your change there: it could not be carried over.`;
}
