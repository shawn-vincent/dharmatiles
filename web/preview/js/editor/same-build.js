// The page and the program must be the same build: the page imports the contract stamp (js/contract.js) when it
// loads, the program compiled the same file in, and the first snapshot of every stream carries the program's.  A
// page read from disk after a merge, or a tab left open across a restart, has a different stamp: then nothing of
// the editor is shown, since every answer could be misread (a sheet that expects fields the program does not send
// shows false errors), only what to do.
import { CONTRACT } from '../contract.js';

/** Is a program that tells `stamp` the build this page was made for? @param {string | undefined} stamp */
export const sameBuild = (stamp) => stamp === CONTRACT;

/** Hide the editor and say only that the page and the program differ, and what to do. @param {Document} [d] */
export function refuseOtherBuild(d = document) {
  const app = d.getElementById('app'), box = d.getElementById('boot-error');
  if (app) app.hidden = true;
  const title = d.getElementById('boot-title'), why = d.getElementById('boot-why');
  if (title) title.textContent = 'This page and the program are different builds.';
  if (why) why.textContent = 'Reload the page; if this stays, restart the program (dharmatiles preview), then reload.';
  if (box) box.hidden = false;
}
