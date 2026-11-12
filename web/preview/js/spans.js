// When the spans the page holds (the stack's, the map's) may be applied to
// the text shown.  A span is a byte range of the text one check read; once
// the text moves on, or the check found a mistake (the last good stack
// stays), the spans describe older text and writing at them corrupts the
// document.  So an edit that is driven by a span waits for a clean check of
// exactly the text on screen.

export const STALE_NOTE = 'Finish the edit in the source first: the sheet and the map wait for a clean check of what is written.';

/** `d` is the doc store's state; `tileId` the tile shown. True when its spans describe its text exactly. */
export function spansFresh(d, tileId) {
  return d.text != null && d.checked === d.text && d.ok === true
    && d.tileId === tileId && d.checkedFile === d.file && d.checkedTile === d.tileId;
}
