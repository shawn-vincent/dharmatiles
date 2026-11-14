// @ts-check
// The messages panel (js/messages.js is the stream): the filters, and under them the running build, the build's record and
// every open message as a row with its actions.  Neither owns a message: fix the cause and the row, the mark in the source and
// the count go with it.  `MessagesBody({m, act})` and `MessageFilters({m, act})` are functions of the model (`m`, built in
// editor/panel.js from the messages stream, the tile, the progress and the document) and of `act`, what the buttons do.
// Which filter is on and which message a mark in the source pointed to are the model's.

import { FILTERS, buildOf, explainHref, filterRows, firstError, gateOf, gateRows, gaugeOf, jobWords, serverOf } from '../messages.js';
import { CopyButton } from './copy-button.js';
import { html, useEffect, useState } from './html.js';
import { ReportButton, ReportPane, canReport } from './report-button.js';
import { Icon } from './icons.js';

const secs = (s) => (s == null ? '' : s >= 60 ? `${Math.floor(s / 60)}m${String(Math.round(s % 60)).padStart(2, '0')}s` : `${s.toFixed(1)} s`);

export function MessageFilters({ m, act }) {
  const v = m.view.value;
  const filter = m.filter.value;
  return html`${FILTERS.map(([id, label, keep]) => {
    const n = id === 'all' ? v.rows.length : v.rows.filter((r) => keep(r.m)).length;
    return html`<button type="button" key=${id} class=${filter === id ? 'on' : undefined} data-filter=${id} aria-pressed=${String(filter === id)} onClick=${() => act.setFilter(id)}>${label}${n ? html` <span class="n">${n}</span>` : null}</button>`;
  })}`;
}

/** The build under way: stage, elapsed against the estimate, the bar. */
function Running({ m }) {
  const t = m.tile.value;
  const p = m.progress.value;
  const words = p ? jobWords(p, t?.name ?? '') : null;
  const stage = words?.stage ?? 'starting';
  return html`<div class="msg run" id="msg-run"><i class="dot busy"></i><div class="body"><div class="line">${words?.title ?? `Building ${t?.name ?? ''}`}</div><div class="sub">${p ? `${stage} · ${secs(p.elapsedS)}${p.estimateS != null ? ` of ~${secs(p.estimateS)}` : ''}` : 'starting'}</div><div class="rbar"><i style=${`width: ${p ? Math.floor(p.fraction * 100) : 0}%`}></i></div></div></div>`;
}

/** One overhang budget as a bar: full at the budget, the multiple written in it when it is past. */
function Gauge({ label, g }) {
  return html`<div class=${`gauge ${g.tone}`}><span class="g-lab">${label}</span>
    <span class="g-bar" role="meter" aria-valuemin="0" aria-valuemax=${g.budget} aria-valuenow=${g.used} aria-label=${label}><i style=${`width: ${Math.round(g.fill * 100)}%`}>${g.over ? html`<b>${g.over}</b>` : null}</i></span>
    <span class="g-num">${`${g.used.toFixed(2)} of ${g.budget.toFixed(2)} mm²`}</span></div>`;
}

/**
 * Where the overhang is: the tile from below, the faces the gate painted in the colours they have on the model (`--over` past the
 * steepest angle, `--band` in the band).  `plan` is the viewer's own mesh read once (`undersidePlan`, payload.js): the page draws
 * the program's flags, it does not classify.  Seen from below, so x runs the other way.
 */
function Underside({ plan, angles }) {
  const { overhangDeg, steepestDeg } = angles;
  const w = plan.width, h = plan.height;
  return html`<figure class="underside"><svg viewBox=${`0 0 ${w} ${h}`} role="img" aria-label=${`The underside of the tile: where the faces past ${steepestDeg} degrees and in the ${overhangDeg} to ${steepestDeg} degree band are`}>
      <rect class="u-bed" x="0" y="0" width=${w} height=${h} />
      ${plan.orange ? html`<path class="u-band" d=${plan.orange} />` : null}${plan.red ? html`<path class="u-over" d=${plan.red} />` : null}</svg>
    <figcaption><b>Underside</b>, from below<br /><i class="sw over"></i>past ${steepestDeg}° <i class="sw band"></i>${overhangDeg} to ${steepestDeg}°</figcaption></figure>`;
}

/** The gate's verdict: the two budgets that decide as bars, everything else in one Details line. */
function Numbers({ gate, m, t }) {
  const a = gateOf(gate);
  if (!a) return null;
  const { overhangDeg, steepestDeg } = m.angles.value;
  const past = gaugeOf(a.overhang_mm2, a.overhang_budget_mm2), band = gaugeOf(a.band_mm2, a.band_budget_mm2);
  const rest = /** @type {any[][]} */ (gateRows(a, m.angles.value)).filter(([k]) => !/^(Band|Past) /.test(k));
  if (t?.record?.record?.seed != null) rest.push(['Seed', t.record.record.seed, '']);
  return html`<div class="gauges">${past ? html`<${Gauge} label=${`Past ${steepestDeg}°`} g=${past} />` : null}${band ? html`<${Gauge} label=${`${overhangDeg} to ${steepestDeg}° band`} g=${band} />` : null}</div>
    <details class="gate-details"><summary>Details</summary><dl class="nums">${rest.map(([k, v, cls]) => html`<div class="pair" key=${k}><dt>${k}</dt><dd class=${cls || undefined}>${v}</dd></div>`)}</dl></details>`;
}

function Record({ rec, m, act }) {
  const t = m.tile.value;
  const gate = rec.gate, fails = gate?.severity === 'error';
  // "Built in 2.1 s · passes the printability gate": the numbers below, and each reason the gate fails on its own line
  const verdict = gate?.text.split(':')[0].replace(/^(Passes|Fails)/, (w) => w.toLowerCase());
  const target = buildOf(rec.build)?.target ? ` · for ${buildOf(rec.build)?.target}` : '';
  void m.shown.value; // the worst face and the underside are known once the mesh is
  const plan = gate ? act.underside() : null;
  return html`<div class=${`msg rec${rec.dim ? ' dim' : ''}`}><i class=${`dot ${fails ? 'bad' : 'ok'}`}></i><div class="body">
    <div class="line">${`${[rec.build?.text, verdict].filter(Boolean).join(' · ')}${target}`}</div>
    ${(gateOf(gate)?.failures ?? []).map((f, i) => html`<div class="fail" key=${i}>${f}</div>`)}
    ${gate ? html`<${Numbers} gate=${gate} m=${m} t=${t} />` : null}
    <div class="acts">${gate && act.canShowWorst() ? html`<button type="button" class="btn" title="Take the camera to the largest face the gate painted" onClick=${act.showWorst}>Show worst face</button>` : null}
      <${CopyButton} text=${() => `${rec.build?.text ?? ''}\n${gate?.text ?? ''}\n${JSON.stringify(gateOf(gate) ?? {}, null, 2)}`} title="Copy the record" /></div></div>
    ${plan ? html`<${Underside} plan=${plan} angles=${m.angles.value} />` : null}</div>`;
}

/** The program's `quoted` words as the page's own mono words, never with the backticks. @param {string} text */
function tickWords(text) {
  return String(text).split(/`([^`]+)`/).map((part, i) => (i % 2 ? html`<code class="w">${part}</code>` : part));
}

/** The Explain link of a message with a `dt::` code: the code's error page, from the docs served with the program. */
function explain(/** @type {string | null | undefined} */ code) {
  const href = explainHref(code);
  return href ? html`<a class="btn explain" href=${href} target="_blank" rel="noopener" title="What this error means, and the rule behind it">Explain</a>` : null;
}

function Row({ row, m, act }) {
  const { m: msg, dim } = row;
  const t = m.tile.value;
  const lit = m.lit.value === msg.seq;
  const [reporting, setReporting] = useState(false);
  useEffect(() => {
    if (!lit) return undefined;
    const timer = setTimeout(() => m.clearLit(msg.seq), 1600);
    return () => clearTimeout(timer);
  }, [lit]);
  const source = msg.span && msg.span.file === m.file.value ? m.text.value ?? null : null;
  const line = msg.span?.line ? () => act.showLine(msg.span.file === t?.path ? null : msg.span.file, msg.span.line) : null;
  const here = msg.span ? `${msg.span.file.split('/').slice(-1)[0]}:${msg.span.line}:${msg.span.col}` : null;
  // the sentence first; the code, a name for looking up, last and quiet.  A row with a place goes to it when clicked.
  return html`<div class=${`msg ${msg.severity}${dim ? ' dim' : ''}${lit ? ' lit' : ''}${line ? ' goes' : ''}`} data-seq=${msg.seq} ref=${(el) => { if (el && lit) el.scrollIntoView({ block: 'nearest' }); }}
      title=${line ? 'Show the line in the source' : undefined} role=${line ? 'button' : undefined} tabIndex=${line ? 0 : undefined} onKeyDown=${line ? (e) => { if (e.key === 'Enter' && e.target === e.currentTarget) line(); } : undefined} onClick=${line ? (e) => { if (!e.target.closest('.acts') && !e.target.closest('.report-pane')) line(); } : undefined}>
    <i class=${`dot ${msg.severity}`}></i>
    <div class="body"><div class="head"><span class="text">${serverOf(msg)?.origin === 'page' ? 'The page: ' : ''}${tickWords(msg.text)}</span></div>
      ${msg.help ? html`<div class="help">${tickWords(msg.help)}</div>` : null}
      <div class="where">${here ? html`<span title=${msg.span.file}>${here}</span>` : null}${msg.code ? html`<code class="code">${msg.code}</code>` : null}</div>
      ${reporting ? html`<${ReportPane} msg=${msg} act=${act} close=${() => setReporting(false)} />` : null}</div>
    <div class="acts">
      ${msg.fix ? html`<button type="button" class="btn primary fix" title="Make this change in the source" onClick=${() => act.applyFix(msg)}>${msg.fix.label}</button>` : null}
      ${explain(msg.code)}
      ${msg.node && t ? html`<button type="button" class="btn" title="Select the stack entry this is about" onClick=${() => act.showNode(msg.node)}>Show on tile</button>` : null}
      <${CopyButton} text=${() => act.details(msg, source)} title="Copy details: the code, where it is, its source line and this environment" />
      ${canReport(msg) ? html`<${ReportButton} open=${reporting} toggle=${() => setReporting(!reporting)} />` : null}
      ${msg.kind === 'server' ? html`<button type="button" class="btn icon" title="Put this away" aria-label="Put this message away" onClick=${() => act.dismiss(msg.seq)}><${Icon} name="x" size=${14} /></button>` : null}
    </div></div>`;
}

export function MessagesBody({ m, act }) {
  if (!m.open.value) return null; // shut: nothing is drawn
  const t = m.tile.value;
  const v = m.view.value;
  const shown = filterRows(v.rows, m.filter.value);
  const fresh = shown.filter((r) => !r.dim), stale = shown.filter((r) => r.dim);
  // work in flight for the document: a tile's build or a map's pieces loading, the one record the program sends
  const working = v.running || !!m.progress.value;
  return html`${working ? html`<${Running} m=${m} />` : null}${!v.running && v.record ? html`<${Record} rec=${v.record} m=${m} act=${act} />` : null}
    ${v.cleared.length ? html`<div class="cleared" title=${v.cleared.map((x) => `${x.code ?? x.kind}: ${x.text}`).join('\n')}>${`${v.cleared.length} cleared by this build`}</div>` : null}
    ${fresh.map((r) => html`<${Row} key=${r.m.seq} row=${r} m=${m} act=${act} />`)}
    ${stale.length || (v.running && v.record) ? html`<div class="cap from-last">From the last build · shown until the new one finishes</div>${v.running && v.record ? html`<${Record} rec=${v.record} m=${m} act=${act} />` : null}${stale.map((r) => html`<${Row} key=${r.m.seq} row=${r} m=${m} act=${act} />`)}` : null}
    ${!shown.length && !v.record && !working ? html`<div class="empty-msg">${t ? 'Nothing has been said about this tile yet.' : 'Choose a tile.'}</div>` : null}`;
}

/** Copy all: the program's own text of the tile's open messages (one renderer), never the page's. */
export const CopyAll = ({ act }) => html`<${CopyButton} text=${act.allText} label="Copy all" title="Copy every open message of this tile as text" />`;

/**
 * The view's card when a document does not build: the first error in its words, laid over the plate in the plate's own light
 * colours (the `--chip-*` tokens), so the place the user looks says what is wrong.  The messages panel stays the full record.
 * Nothing is drawn while the tile builds or reads.  `m` is the panel's model, `act` its actions.
 */
export function ErrorCard({ m, act }) {
  const t = m.tile.value;
  const v = m.view.value;
  if (!t || t.status !== 'failed' || v.running) return null;
  const first = firstError(t, v);
  if (!first) return null;
  const go = first.span?.line ? () => act.showLine(first.span.file === t.path ? null : first.span.file, first.span.line) : null;
  return html`<div class="doc-error" role="alert"><div class="de-title">Nothing built</div>
    <div class="de-text">${tickWords(first.text)}${first.span?.line ? html` <span class="de-at">${`(line ${first.span.line})`}</span>` : null}</div>
    ${first.help ? html`<div class="de-help">${tickWords(first.help)}</div>` : null}
    <div class="de-acts">${first.fix ? html`<button type="button" class="btn primary fix" onClick=${() => act.applyFix(first)}>${first.fix.label}</button>` : null}${explain(first?.code)}${go ? html`<button type="button" class="btn" onClick=${go}>Go to line</button>` : null}<button type="button" class="btn" onClick=${() => act.openPanel()}>All messages</button></div></div>`;
}

/**
 * The tile level of the left panel for a tile that is open and could not be read or built: it says so, shows the first
 * mistake (code and words) and offers Go to line.  `first` is `firstError` (the same record as the card and the messages);
 * `go` jumps into the source (null: the mistake has no line).
 */
export function Unread({ name, first, go, openPanel }) {
  return html`<div class="empty unread" role="alert"><b>${name ? `${name} could not be read` : 'This tile could not be read'}</b>
    ${first ? html`<div class="de-text">${first.code ? html`<code>${first.code}</code> ` : null}${tickWords(first.text)}${first.span?.line ? html` <span class="de-at">${`(line ${first.span.line})`}</span>` : null}</div>
      ${first.help ? html`<div class="de-help">${tickWords(first.help)}</div>` : null}` : html`<div class="de-text">The program gave no reason: see the messages.</div>`}
    <div class="de-acts">${explain(first?.code)}${go ? html`<button type="button" class="btn" onClick=${go}>Go to line</button>` : null}<button type="button" class="btn" onClick=${openPanel}>All messages</button></div></div>`;
}
