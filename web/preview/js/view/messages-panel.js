// @ts-check
// The messages panel (js/messages.js is the stream): the filters, and under them the running build, the build's record and
// every open message as a row with its actions.  Neither owns a message: fix the cause and the row, the mark in the source and
// the count go with it.  `MessagesBody({m, act})` and `MessageFilters({m, act})` are functions of the model (`m`, built in
// editor/panel.js from the messages stream, the tile, the progress and the document) and of `act`, what the buttons do.
// Which filter is on and which message a mark in the source pointed to are the model's.

import { FILTERS, buildOf, filterRows, gateOf, gateRows, serverOf } from '../messages.js';
import { CopyButton } from './copy-button.js';
import { html, useEffect } from './html.js';

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
  const stage = !p ? 'starting' : !p.count ? p.label : p.count[2] === 0 ? `${p.label}: ${p.count[0]}` : `${p.label}: ${p.count[0]} ${p.count[1]}/${p.count[2]}`;
  return html`<div class="msg run" id="msg-run"><i class="dot busy"></i><div class="body"><div class="line">${`Building ${t?.name ?? ''}`}</div><div class="sub">${p ? `${stage} · ${secs(p.elapsedS)}${p.estimateS != null ? ` of ~${secs(p.estimateS)}` : ''}` : 'starting'}</div><div class="rbar"><i style=${`width: ${p ? Math.floor(p.fraction * 100) : 0}%`}></i></div></div></div>`;
}

/** The gate's numbers as rows under the record. */
function Numbers({ gate, m, t }) {
  const a = gateOf(gate);
  if (!a) return html`<dl class="nums"></dl>`;
  const rows = gateRows(a, m.angles.value);
  if (t?.record?.record?.seed != null) rows.push(['Seed', t.record.record.seed, '']);
  return html`<dl class="nums">${rows.map(([k, v, cls]) => html`<div class="pair" key=${k}><dt>${k}</dt><dd class=${cls || undefined}>${v}</dd></div>`)}</dl>`;
}

function Record({ rec, m, act }) {
  const t = m.tile.value;
  const gate = rec.gate, fails = gate?.severity === 'error';
  // "Built in 2.1 s · passes the printability gate": the numbers below, and each reason the gate fails on its own line
  const verdict = gate?.text.split(':')[0].replace(/^(Passes|Fails)/, (w) => w.toLowerCase());
  const target = buildOf(rec.build)?.target ? ` · for ${buildOf(rec.build)?.target}` : '';
  void m.shown.value; // the worst face is known once the mesh is
  return html`<div class=${`msg rec${rec.dim ? ' dim' : ''}`}><i class=${`dot ${fails ? 'bad' : 'ok'}`}></i><div class="body">
    <div class="line">${`${[rec.build?.text, verdict].filter(Boolean).join(' · ')}${target}`}</div>
    ${(gateOf(gate)?.failures ?? []).map((f, i) => html`<div class="fail" key=${i}>${f}</div>`)}
    ${gate ? html`<${Numbers} gate=${gate} m=${m} t=${t} />` : null}
    <div class="acts">${gate && act.canShowWorst() ? html`<button type="button" class="btn" title="Take the camera to the largest face the gate painted" onClick=${act.showWorst}>Show worst face</button>` : null}
      <${CopyButton} text=${() => `${rec.build?.text ?? ''}\n${gate?.text ?? ''}\n${JSON.stringify(gateOf(gate) ?? {}, null, 2)}`} title="Copy the record" /></div></div></div>`;
}

function Row({ row, m, act }) {
  const { m: msg, dim } = row;
  const t = m.tile.value;
  const lit = m.lit.value === msg.seq;
  useEffect(() => {
    if (!lit) return undefined;
    const timer = setTimeout(() => m.clearLit(msg.seq), 1600);
    return () => clearTimeout(timer);
  }, [lit]);
  const source = msg.span && msg.span.file === m.file.value ? m.text.value ?? null : null;
  return html`<div class=${`msg ${msg.severity}${dim ? ' dim' : ''}${lit ? ' lit' : ''}`} data-seq=${msg.seq} ref=${(el) => { if (el && lit) el.scrollIntoView({ block: 'nearest' }); }}>
    <i class=${`dot ${msg.severity}`}></i>
    <div class="body"><div class="head">${msg.code ? html`<code class="code">${msg.code}</code>` : null}<span class="text">${serverOf(msg)?.origin === 'page' ? 'The page: ' : ''}${msg.text}</span></div>
      ${msg.help ? html`<div class="help">${msg.help}</div>` : null}
      ${msg.span ? html`<div class="where" title=${msg.span.file}>${`${msg.span.file.split('/').slice(-1)[0]}:${msg.span.line}:${msg.span.col}`}</div>` : null}</div>
    <div class="acts">
      ${msg.span?.line ? html`<button type="button" class="btn" title="Show the line in the source" onClick=${() => act.showLine(msg.span.file === t?.path ? null : msg.span.file, msg.span.line)}>Go to line</button>` : null}
      ${msg.node && t ? html`<button type="button" class="btn" title="Select the stack entry this is about" onClick=${() => act.showNode(msg.node)}>Show on tile</button>` : null}
      <${CopyButton} text=${() => act.details(msg, source)} title="Copy details: the code, where it is, its source line and this environment" />
      ${msg.kind === 'server' ? html`<button type="button" class="btn icon" title="Put this away" aria-label="Put this message away" onClick=${() => act.dismiss(msg.seq)}>×</button>` : null}
    </div></div>`;
}

export function MessagesBody({ m, act }) {
  if (!m.open.value) return null; // shut: nothing is drawn
  const t = m.tile.value;
  const v = m.view.value;
  const shown = filterRows(v.rows, m.filter.value);
  const fresh = shown.filter((r) => !r.dim), stale = shown.filter((r) => r.dim);
  return html`${v.running ? html`<${Running} m=${m} />` : v.record ? html`<${Record} rec=${v.record} m=${m} act=${act} />` : null}
    ${v.cleared.length ? html`<div class="cleared" title=${v.cleared.map((x) => `${x.code ?? x.kind}: ${x.text}`).join('\n')}>${`${v.cleared.length} cleared by this build`}</div>` : null}
    ${fresh.map((r) => html`<${Row} key=${r.m.seq} row=${r} m=${m} act=${act} />`)}
    ${stale.length || (v.running && v.record) ? html`<div class="from-last">From the last build · shown until the new one finishes</div>${v.running && v.record ? html`<${Record} rec=${v.record} m=${m} act=${act} />` : null}${stale.map((r) => html`<${Row} key=${r.m.seq} row=${r} m=${m} act=${act} />`)}` : null}
    ${!shown.length && !v.record && !v.running ? html`<div class="empty-msg">${t ? 'Nothing has been said about this tile yet.' : 'Choose a tile.'}</div>` : null}`;
}

/** Copy all: the program's own text of the tile's open messages (one renderer), never the page's. */
export const CopyAll = ({ act }) => html`<${CopyButton} text=${act.allText} label="Copy all" title="Copy every open message of this tile as text" />`;
