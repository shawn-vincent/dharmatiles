const LIMIT = 76;

function clipped(text) {
  const one = text.replace(/\t/g, '    ').replace(/\n/g, ' ↵ ').trim();
  if (!one) return null;
  return one.length > LIMIT ? `${one.slice(0, LIMIT - 1)}…` : one;
}

function excerpt(lines) {
  const useful = lines.map((line) => line.trim()).filter(Boolean);
  if (!useful.length) return null;
  const shown = useful.slice(0, 2).join(' ↵ ');
  return clipped(useful.length > 2 ? `${shown} … +${useful.length - 2} lines` : shown);
}

function changedPart(before, after) {
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  let end = 0;
  while (end < before.length - start && end < after.length - start
    && before[before.length - 1 - end] === after[after.length - 1 - end]) end++;
  return {
    removed: clipped(before.slice(start, before.length - end || undefined)),
    added: clipped(after.slice(start, after.length - end || undefined)),
  };
}

/** A compact, display-ready account of one text history step. */
export function describeChange(before = '', after = '', kind = 'edit', custom = null) {
  const oldLines = before.split('\n'), newLines = after.split('\n');
  let start = 0;
  while (start < oldLines.length && start < newLines.length && oldLines[start] === newLines[start]) start++;
  let end = 0;
  while (end < oldLines.length - start && end < newLines.length - start
    && oldLines[oldLines.length - 1 - end] === newLines[newLines.length - 1 - end]) end++;
  const removedLines = oldLines.slice(start, oldLines.length - end);
  const addedLines = newLines.slice(start, newLines.length - end);
  const fragments = removedLines.length === 1 && addedLines.length === 1
    ? changedPart(removedLines[0], addedLines[0])
    : { removed: excerpt(removedLines), added: excerpt(addedLines) };
  const line = Math.max(1, start + 1);
  let action = custom;
  if (!action && kind === 'type') {
    action = fragments.removed && fragments.added ? 'Replaced text'
      : fragments.removed ? 'Deleted text' : fragments.added ? 'Typed' : 'Edited';
  }
  const names = { lens: 'Changed properties', view: 'Changed tile', disk: 'Disk change' };
  return {
    title: `${action || names[kind] || 'Edited'} on line ${line}`,
    line,
    removed: fragments.removed,
    added: fragments.added,
  };
}
