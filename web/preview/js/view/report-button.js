// @ts-check
// Report this defect: a button on a message that means the program went wrong (never on a mistake in the maker's document),
// beside Copy.  It opens a short preview of the report the program made (`GET /api/messages/report`, crates/dharmatiles/src/
// report.rs: the message's copied text, the environment, the tile's source) and a button that opens GitHub's new-issue form,
// filled in.  When the address would be too long the whole report goes to the clipboard first (a refusal of the clipboard is
// said under the preview, never hidden) and the form says to paste it.  The controls stand above the report, which scrolls in its own box, so a short panel never hides them.  The row owns whether the preview is open: the button
// sits in its actions and the preview under its words.

import { html, useEffect, useState } from './html.js';

/** Does this message get the button?  The program says so (`reportable`); a message of the page's own that it could not be told has no number to ask about. */
/** @param {{ seq: number, reportable?: boolean }} m */
export const canReport = (m) => m.reportable === true && m.seq > 0;

/** @param {{ open: boolean, toggle: () => void }} p */
export function ReportButton({ open, toggle }) {
  return html`<button type="button" class="btn report-btn" aria-expanded=${String(open)} title="Open a GitHub issue about this defect in the program" onClick=${toggle}>Report this defect</button>`;
}

/** @param {{ msg: { seq: number }, act: { report: (seq: number, leaveSourceOut: boolean) => Promise<any>, openIssue: (url: string) => void }, close: () => void }} p */
export function ReportPane({ msg, act, close }) {
  const [leaveOut, setLeaveOut] = useState(false);
  /** @type {[any, Function]} */
  const [rep, setRep] = useState(null);
  const [problem, setProblem] = useState('');
  const [note, setNote] = useState('');
  useEffect(() => {
    let current = true;
    setProblem('');
    act.report(msg.seq, leaveOut).then(
      (r) => { if (current) setRep(r); },
      (e) => { if (current) { setRep(null); setProblem(`The report could not be made: ${e.message}`); } },
    );
    return () => { current = false; };
  }, [msg.seq, leaveOut]);
  const file = async () => {
    if (!rep) return;
    setNote('');
    if (rep.truncated) {
      try {
        await navigator.clipboard.writeText(rep.body);
        setNote('The whole report is on your clipboard: paste it into the issue.');
      } catch (e) {
        // said, not hidden: the form still opens, and the report is in the box above to copy by hand
        setNote(`Could not copy the report (${/** @type {Error} */ (e).message}); select it below and copy it yourself, then paste it into the issue.`);
      }
    }
    act.openIssue(rep.url);
  };
  return html`<div class="report-pane" data-report="1">
    <div class="report-head">This is what will go in the issue${rep ? html`: <b>${rep.title}</b>` : ''}</div>
    <label class="report-opt"><input type="checkbox" checked=${leaveOut} onChange=${(e) => setLeaveOut(e.currentTarget.checked)} /> Leave my tile's source out</label>
    <div class="report-acts"><button type="button" class="btn primary" disabled=${!rep} onClick=${file}>Open GitHub issue</button><button type="button" class="btn" onClick=${close}>Close</button></div>
    ${rep?.truncated ? html`<div class="report-note">Too long for the address: the issue will say to paste the report, which is copied for you.</div>` : null}
    ${note ? html`<div class="report-note" role="status">${note}</div>` : null}
    ${problem ? html`<div class="report-err" role="alert">${problem}</div>` : null}
    ${rep ? html`<pre class="report-body" tabIndex=${0}>${rep.body}</pre>` : problem ? null : html`<div class="report-wait">Making the report</div>`}
  </div>`;
}
