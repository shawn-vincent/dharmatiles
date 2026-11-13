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
// strings, numbers, comments.  Blocks that already carry markup, and shell
// commands, are left as they are.
addEventListener('DOMContentLoaded', () => {
  const esc = (t) => t.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  const shell = /^\s*(\$|cargo |git |cd |\.\/|dharmatiles (?!version=)|brew |python|cp |mkdir |export |ls )/;
  // inside a string: a `$name` (read from the context) takes the node colour,
  // a number with its unit (8dmm, 90deg, 50%) the number colour; the rest
  // stays a string.  Only strings that are values: an `about` text is left alone.
  const value = /\$[A-Za-z_]|\d(?:%|sq|dmm|mm|in|layer|deg|rad)\b/;
  const inner = /(\$[A-Za-z_]\w*)|(-?\b\d+(?:\.\d+)?(?:%|(?:sq|dmm|mm|in|layer|deg|rad)\b))/g;
  const hlString = (str) => {
    if (!value.test(str)) return `<span class="hl-s">${esc(str)}</span>`;
    let o = '', at = 0;
    for (const m of str.matchAll(inner)) {
      if (m.index > at) o += `<span class="hl-s">${esc(str.slice(at, m.index))}</span>`;
      o += `<span class="${m[1] ? 'hl-n' : 'hl-v'}">${esc(m[0])}</span>`;
      at = m.index + m[0].length;
    }
    return o + (at < str.length ? `<span class="hl-s">${esc(str.slice(at))}</span>` : '');
  };
  const tok = /(\/\/[^\n]*)|("(?:[^"\\]|\\.)*")|(\b[A-Za-z][\w+-]*(?==))|((?:^|(?<=[{;]))[ \t]*[A-Za-z][\w+-]*)|(-?\b\d+(?:\.\d+)?\b)/gm;
  for (const code of document.querySelectorAll('pre > code')) {
    if (code.children.length || code.closest('.no-hl')) continue;
    const text = code.textContent;
    const first = text.split('\n').find((l) => l.trim()) || '';
    if (shell.test(first) || !/[{}=]/.test(text)) continue;
    let out = '', last = 0;
    text.replace(tok, (m, com, str, prop, node, num, at) => {
      out += esc(text.slice(last, at));
      if (com) out += `<span class="hl-c">${esc(com)}</span>`;
      else if (str) out += hlString(str);
      else if (prop) out += `<span class="hl-p">${esc(prop)}</span>`;
      else if (node) { const ws = node.match(/^[ \t]*/)[0]; out += `${ws}<span class="hl-n">${esc(node.slice(ws.length))}</span>`; }
      else out += `<span class="hl-v">${esc(num)}</span>`;
      last = at + m.length;
      return m;
    });
    code.innerHTML = out + esc(text.slice(last));
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
