// The docs' one script: the light/dark switch, the same control the preview
// has.  The system's setting until it is used; then the choice is
// remembered in this browser.  Runs in <head>, so the page never flashes.
(() => {
  const KEY = 'dt-theme';
  const root = document.documentElement;
  const sys = matchMedia('(prefers-color-scheme: dark)');
  const stored = () => { try { return localStorage.getItem(KEY); } catch { return null; } };
  const apply = () => {
    const s = stored();
    if (s === 'light' || s === 'dark') root.dataset.theme = s; else delete root.dataset.theme;
    const b = document.getElementById('theme');
    if (b) {
      const dark = s ? s === 'dark' : sys.matches;
      b.textContent = dark ? '☀' : '☾';
      b.setAttribute('aria-label', dark ? 'Switch to Lichen & Linen' : 'Switch to Moonpool');
      b.title = b.getAttribute('aria-label');
    }
  };
  apply();
  sys.addEventListener('change', apply);
  addEventListener('DOMContentLoaded', () => {
    const nav = document.querySelector('nav.top .wrap');
    if (!nav) return;
    // the section links in one row of their own, so a phone can scroll it
    const links = document.createElement('div');
    links.className = 'links';
    for (const a of [...nav.querySelectorAll(':scope > a:not(.brand)')]) links.append(a);
    nav.append(links);
    const b = document.createElement('button');
    b.id = 'theme'; b.type = 'button'; b.className = 'btn icon theme';
    b.onclick = () => {
      const dark = root.dataset.theme ? root.dataset.theme === 'dark' : sys.matches;
      try { localStorage.setItem(KEY, dark ? 'light' : 'dark'); } catch { /* private mode */ }
      apply();
    };
    nav.append(b);
    apply();
  });
})();

addEventListener("DOMContentLoaded", () => {
  // an element without a picture gets its name on a plate
  for (const a of document.querySelectorAll(".elements a")) {
    if (!a.querySelector("img, svg")) a.dataset.name = (a.querySelector("code") || a).textContent.trim();
  }
});

// .dtile examples, coloured the same everywhere: node names, properties,
// strings, numbers, comments.  A block may carry <mark> spans (an error page's
// broken document and its fix, the change marked): the colours are laid under
// them.  Blocks with any other markup, and shell commands, are left as they are.
addEventListener('DOMContentLoaded', () => {
  const esc = (t) => t.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  const shell = /^\s*(\$|cargo |git |cd |\.\/|dharmatiles (?!version=)|brew |python|cp |mkdir |export |ls )/;
  // inside a string: a `$name` (read from the context) takes the node colour,
  // a number with its unit (8dmm, 90deg, 50%) the number colour; the rest
  // stays a string.  Only strings that are values: an `about` text is left alone.
  const value = /\$[A-Za-z_]|\d(?:%|sq|dmm|mm|in|layer|deg|rad)\b/;
  const inner = /(\$[A-Za-z_]\w*)|(-?\b\d+(?:\.\d+)?(?:%|(?:sq|dmm|mm|in|layer|deg|rad)\b))/g;
  // the coloured pieces of a text, in order: [text, class or '']
  const hlString = (str, out) => {
    if (!value.test(str)) { out.push([str, 'hl-s']); return; }
    let at = 0;
    for (const m of str.matchAll(inner)) {
      if (m.index > at) out.push([str.slice(at, m.index), 'hl-s']);
      out.push([m[0], m[1] ? 'hl-n' : 'hl-v']);
      at = m.index + m[0].length;
    }
    if (at < str.length) out.push([str.slice(at), 'hl-s']);
  };
  const tok = /(\/\/[^\n]*)|("(?:[^"\\]|\\.)*")|(\b[A-Za-z][\w+-]*(?==))|((?:^|(?<=[{;]))[ \t]*[A-Za-z][\w+-]*)|(-?\b\d+(?:\.\d+)?\b)/gm;
  const pieces = (text) => {
    const out = [];
    let last = 0;
    text.replace(tok, (m, com, str, prop, node, num, at) => {
      out.push([text.slice(last, at), '']);
      if (com) out.push([com, 'hl-c']);
      else if (str) hlString(str, out);
      else if (prop) out.push([prop, 'hl-p']);
      else if (node) { const ws = node.match(/^[ \t]*/)[0]; out.push([ws, ''], [node.slice(ws.length), 'hl-n']); }
      else out.push([num, 'hl-v']);
      last = at + m.length;
      return m;
    });
    out.push([text.slice(last), '']);
    return out.filter(([t]) => t);
  };
  for (const code of document.querySelectorAll('pre > code')) {
    if (code.closest('.no-hl')) continue;
    if ([...code.children].some((c) => c.tagName !== 'MARK' || c.children.length)) continue;
    const text = code.textContent;
    const first = text.split('\n').find((l) => l.trim()) || '';
    if (shell.test(first) || !/[{}=]/.test(text)) continue;
    // the marks, by where they fall in the text
    const marks = [];
    let at = 0;
    for (const n of code.childNodes) {
      const len = n.textContent.length;
      if (n.nodeType === 1) marks.push({ from: at, to: at + len, cls: n.className });
      at += len;
    }
    // cut the coloured pieces where a mark starts or ends, and open and shut the marks between them
    const cuts = [...new Set(marks.flatMap((m) => [m.from, m.to]))].sort((a, b) => a - b);
    let html = '', pos = 0, open = null;
    const shut = () => { if (open) { html += '</mark>'; open = null; } };
    const markAt = (p) => marks.find((m) => m.from <= p && p < m.to) || null;
    const empties = (p) => marks.filter((m) => m.from === p && m.to === p).forEach((m) => { shut(); html += `<mark class="${m.cls}"></mark>`; });
    for (const [t, cls] of pieces(text)) {
      let from = 0;
      while (from < t.length) {
        const p = pos + from;
        empties(p);
        const next = cuts.find((c) => c > p);
        const to = Math.min(t.length, next === undefined ? t.length : next - pos);
        const m = markAt(p);
        if (m !== open) { shut(); if (m) { html += `<mark class="${m.cls}">`; open = m; } }
        const part = esc(t.slice(from, to));
        html += cls ? `<span class="${cls}">${part}</span>` : part;
        from = to;
      }
      pos += t.length;
    }
    shut();
    empties(pos);
    code.innerHTML = html;
    code.parentElement.classList.add('dtile');
  }
});

// The contents list is one disclosure: open in the margin column on a wide
// screen (where it is not a control), folded above the content on a phone.
addEventListener('DOMContentLoaded', () => {
  const toc = document.querySelector('nav.toc > details');
  if (!toc) return;
  const wide = matchMedia('(min-width: 1000px)');
  const sync = () => { toc.open = wide.matches; };
  sync();
  wide.addEventListener('change', sync);
});

// The editor's pictures (cargo xtask shots): each figure.shot is a picture of the editor and a key of numbered callouts
// (li[data-at]).  Where each callout's part of the picture is comes from shots.json, written by the same command from the
// live page, so the boxes and numbers are drawn here over the picture, never into it, and follow a redrawn picture.
addEventListener('DOMContentLoaded', async () => {
  const figs = [...document.querySelectorAll('figure.shot[data-shot]')];
  if (!figs.length) return;
  const first = figs[0].querySelector('img');
  let book;
  try {
    const r = await fetch(new URL('shots.json', first.src));
    if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
    book = await r.json();
  } catch (e) {
    // the key under each picture still reads without its boxes: say why they are missing
    console.error(`the callouts of the editor's pictures need assets/shots/shots.json (${e.message}); serve docs/ over http`);
    return;
  }
  for (const fig of figs) {
    const shot = book.shots[fig.dataset.shot];
    const frame = fig.querySelector('.frame');
    if (!shot) { console.error(`shots.json has no picture "${fig.dataset.shot}"`); continue; }
    const pins = document.createElement('ol');
    pins.className = 'pins';
    pins.setAttribute('aria-hidden', 'true');
    const items = [...fig.querySelectorAll('ol.key > li[data-at]')];
    items.forEach((li, i) => {
      const box = shot.anchors[li.dataset.at];
      if (!box) { console.error(`picture "${fig.dataset.shot}" has no callout "${li.dataset.at}"`); return; }
      const pin = document.createElement('li');
      pin.className = 'pin' + (box.y > shot.h * 0.9 ? ' foot' : '');
      const pct = (v, of) => `${(100 * v / of).toFixed(3)}%`;
      pin.style.cssText = `left:${pct(box.x, shot.w)};top:${pct(box.y, shot.h)};width:${pct(box.w, shot.w)};height:${pct(box.h, shot.h)}`;
      pin.innerHTML = `<b>${i + 1}</b>`;
      pins.append(pin);
      const on = (v) => () => pin.classList.toggle('on', v);
      li.addEventListener('mouseenter', on(true));
      li.addEventListener('mouseleave', on(false));
    });
    frame.append(pins);
  }
});
