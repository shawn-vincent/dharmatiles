// A small DOM, enough for Preact to draw into and for a test to read and type into: nodes, attributes, properties,
// events.  Not a browser: no layout, no CSS, no parsing.  `install()` puts `document` on globalThis.

class Node {
  constructor(nodeType, nodeName) {
    this.nodeType = nodeType; this.nodeName = nodeName; this.parentNode = null; this.childNodes = [];
  }
  get parentElement() { return this.parentNode; }
  get firstChild() { return this.childNodes[0] ?? null; }
  get lastChild() { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling() { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) + 1] ?? null : null; }
  get previousSibling() { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) - 1] ?? null : null; }
  appendChild(c) { return this.insertBefore(c, null); }
  insertBefore(c, ref) {
    if (c.parentNode) c.parentNode.removeChild(c);
    const i = ref ? this.childNodes.indexOf(ref) : -1;
    if (i < 0) this.childNodes.push(c); else this.childNodes.splice(i, 0, c);
    c.parentNode = this;
    return c;
  }
  removeChild(c) { this.childNodes.splice(this.childNodes.indexOf(c), 1); c.parentNode = null; return c; }
  remove() { this.parentNode?.removeChild(this); }
  contains(n) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  get textContent() { return this.childNodes.map((c) => c.textContent).join(''); }
}

class Text extends Node {
  constructor(data) { super(3, '#text'); this.data = String(data); }
  get textContent() { return this.data; }
  set nodeValue(v) { this.data = String(v); }
  get nodeValue() { return this.data; }
}

class Element extends Node {
  constructor(name, ns) {
    super(1, name.toUpperCase());
    this.localName = name; this.namespaceURI = ns ?? 'http://www.w3.org/1999/xhtml';
    this.attributes = new Map(); this.listeners = new Map(); this.style = { cssText: '', setProperty() {}, removeProperty() {} };
    this.className = ''; this.disabled = false; this.title = ''; this.type = ''; this.placeholder = ''; this.spellcheck = true; this.hidden = false;
    if (name === 'input' || name === 'textarea') { this.value = ''; this.min = ''; this.step = ''; }
  }
  get dataset() {
    const out = {};
    for (const [k, v] of this.attributes) if (k.startsWith('data-')) out[k.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = v;
    return out;
  }
  closest(sel) { for (let n = this; n; n = n.parentNode) if (n.nodeType === 1 && ((sel.startsWith('.') && (n.className || n.getAttribute('class') || '').split(/\s+/).includes(sel.slice(1))) || n.localName === sel)) return n; return null; }
  setAttribute(k, v) { this.attributes.set(k, String(v)); }
  getAttribute(k) { return this.attributes.get(k) ?? null; }
  removeAttribute(k) { this.attributes.delete(k); }
  addEventListener(t, fn) { (this.listeners.get(t) ?? this.listeners.set(t, []).get(t)).push(fn); }
  removeEventListener(t, fn) { const l = this.listeners.get(t); if (l) l.splice(l.indexOf(fn) >>> 0, 1); }
  dispatchEvent(ev) {
    ev.target ??= this;
    for (let n = this; n; n = n.parentNode) {
      ev.currentTarget = n;
      for (const fn of [...(n.listeners?.get(ev.type) ?? [])]) fn.call(n, ev);
      if (ev.cancelBubble) break;
    }
    return true;
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] ?? null; }
  querySelectorAll(sel) {
    const out = [];
    const want = sel.startsWith('.') ? (n) => (n.className || n.getAttribute('class') || '').split(/\s+/).includes(sel.slice(1)) : (n) => n.localName === sel;
    const walk = (n) => { for (const c of n.childNodes) { if (c.nodeType === 1) { if (want(c)) out.push(c); walk(c); } } };
    walk(this);
    return out;
  }
}

export const document = {
  createElement: (name) => new Element(name),
  createElementNS: (ns, name) => new Element(name, ns),
  createTextNode: (d) => new Text(d),
};

/** Put `document` where Preact looks for it and return a fresh container to draw into. */
export function install() {
  globalThis.document = document;
  return document.createElement('div');
}

/** Press a key in an element: the event the page would see. */
export const press = (el, key) => el.dispatchEvent({ type: 'keydown', key, target: el });
/** Type `text` into an input as a user does: the value changes, then an input event. */
export function type(el, text) { el.value = text; el.dispatchEvent({ type: 'input', target: el }); }
/** Let what is waiting to be drawn be drawn. */
export const settle = () => new Promise((r) => setTimeout(r, 40));
