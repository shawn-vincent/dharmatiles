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
      b.setAttribute('aria-label', dark ? 'Switch to light' : 'Switch to dark');
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
    if (!a.querySelector("img")) a.dataset.name = (a.querySelector("code") || a).textContent.trim();
  }
});

// .dtile examples, coloured the same everywhere: node names, properties,
// strings, numbers, comments.  Blocks that already carry markup, and shell
// commands, are left as they are.
addEventListener('DOMContentLoaded', () => {
  const esc = (t) => t.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  const shell = /^\s*(\$|cargo |git |cd |\.\/|dharmatiles |brew |python|cp |mkdir |export |ls )/;
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
      else if (str) out += `<span class="hl-s">${esc(str)}</span>`;
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
