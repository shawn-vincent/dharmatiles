// When the spans the page holds (the stack's, the map's) may be applied to
// the text shown.  A span is a byte range of the text one check read; once
// the text moves on, or the check found a mistake (the last good stack
// stays), the spans describe older text and writing at them corrupts the
// document.  So an edit that is driven by a span waits for a clean check of
// exactly the text on screen.

export const STALE_NOTE = 'Finish the edit in the source first: the sheet and the map wait for a clean check of what is written.';

/** The index in `text` of byte `b` (the program counts bytes of UTF-8; the page's strings count UTF-16 units). */
export function byteToIndex(text, b) {
  if (!/[^\x00-\x7f]/.test(text)) return b;
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    if (bytes >= b) return i;
    const c = text.codePointAt(i);
    bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    if (c >= 0x10000) i++;
  }
  return text.length;
}

/** `d` is the doc store's state; `tileId` the tile shown. True when its spans describe its text exactly. */
export function spansFresh(d, tileId) {
  return d.text != null && d.checked === d.text && d.ok === true
    && d.tileId === tileId && d.checkedFile === d.file && d.checkedTile === d.tileId;
}
