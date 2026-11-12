// Small popups for the panel's rows: a menu of actions (a row's ⋯, the Add list) and a
// popover of its own content (a rename field).  A page here (it makes elements and listens),
// no store: the caller says what the items are and what each does.  Both sit over the page,
// close on Esc, on a press outside and on Tab, and give the keyboard back to what opened them.
// A menu is a list the arrow keys walk: Up, Down, Home, End, Enter; an item that cannot be
// chosen stays in the list with its reason under it, so what is missing is never a mystery.

let open = null;

/** Close the popup that is open, if one is. */
export function closePopup() { open?.close(); }

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

/** Put `box` by `anchor`: under it (above it when there is no room), `align` its left or right edge with the anchor's, kept on screen. */
function place(box, anchor, align) {
  const a = anchor.getBoundingClientRect();
  const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
  box.style.maxHeight = '';
  const room = { below: vh - a.bottom - 10, above: a.top - 10 };
  const want = Math.min(box.scrollHeight, 460);
  const up = room.below < want && room.above > room.below;
  const h = Math.min(want, up ? room.above : room.below);
  box.style.maxHeight = `${Math.max(120, h)}px`;
  const w = box.offsetWidth;
  let left = align === 'right' ? a.right - w : a.left;
  left = Math.max(8, Math.min(left, vw - w - 8));
  box.style.left = `${left}px`;
  const top = up ? a.top - Math.min(box.offsetHeight, h) - 4 : a.bottom + 4;
  box.style.top = `${Math.max(8, top)}px`;
}

/** Show `box` over the page by `anchor`; returns {close}.  `onClose` runs once when it goes. */
function show(box, anchor, { align = 'right', onClose } = {}) {
  closePopup();
  document.body.append(box);
  place(box, anchor, align);
  let done = false;
  const close = ({ restore = true } = {}) => {
    if (done) return;
    done = true;
    open = null;
    document.removeEventListener('pointerdown', away, true);
    document.removeEventListener('keydown', key, true);
    removeEventListener('resize', close);
    box.remove();
    if (restore && document.contains(anchor)) anchor.focus({ preventScroll: true });
    onClose?.();
  };
  const away = (e) => { if (!box.contains(e.target) && !anchor.contains(e.target)) close({ restore: false }); };
  const key = (e) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); } else if (e.key === 'Tab' && !box.contains(document.activeElement)) close({ restore: false });
  };
  document.addEventListener('pointerdown', away, true);
  document.addEventListener('keydown', key, true);
  addEventListener('resize', close);
  open = { close };
  return { close };
}

/**
 * A menu of `items` by `anchor`.  An item is `{label, sub, swatch, hint, danger, disabled, why, run}` (`why` is the
 * reason an item that cannot be chosen is not), `{head}` (a small heading) or `{sep: true}`.  Choosing an item closes the
 * menu and runs it.  `label` names the menu for a screen reader.
 */
export function openMenu(anchor, items, { label = 'Actions', cls = '', align = 'right', onClose } = {}) {
  const box = el('div', `rmenu ${cls}`);
  box.setAttribute('role', 'menu');
  box.setAttribute('aria-label', label);
  const live = [];
  for (const it of items) {
    if (it.sep) { box.append(el('hr', 'rsep')); continue; }
    if (it.head) { box.append(el('div', 'rhead', it.head)); continue; }
    const b = el('button', `ritem${it.danger ? ' danger' : ''}${it.disabled ? ' off' : ''}`);
    b.type = 'button';
    b.setAttribute('role', 'menuitem');
    b.tabIndex = -1;
    if (it.disabled) b.setAttribute('aria-disabled', 'true');
    if (it.swatch) b.append(el('i', `gl fam-${it.swatch}`));
    const tx = el('span', 'tx');
    tx.append(el('span', 'nm', it.label));
    const sub = it.disabled && it.why ? it.why : it.sub;
    if (sub) {
      const sp = el('span', `sub${it.disabled ? ' why' : ''}`, sub);
      sp.title = sub;
      tx.append(sp);
    }
    b.append(tx);
    if (it.hint) b.append(el('kbd', '', it.hint));
    b.onclick = () => { if (it.disabled) return; popup.close({ restore: !it.keepFocus }); it.run?.(); };
    box.append(b);
    live.push(b);
  }
  const popup = show(box, anchor, { align, onClose });
  const go = (i) => { live[(i + live.length) % live.length]?.focus({ preventScroll: false }); };
  box.addEventListener('keydown', (e) => {
    const i = live.indexOf(document.activeElement);
    if (e.key === 'ArrowDown') go(i < 0 ? 0 : i + 1);
    else if (e.key === 'ArrowUp') go(i < 0 ? live.length - 1 : i - 1);
    else if (e.key === 'Home') go(0);
    else if (e.key === 'End') go(live.length - 1);
    else return;
    e.preventDefault();
  });
  // the first choosable item has the keyboard
  (live.find((b) => !b.hasAttribute('aria-disabled')) || live[0])?.focus({ preventScroll: true });
  return popup;
}

/** A popover by `anchor` that `render(box, close)` fills (a field to type a name in). */
export function openPopover(anchor, render, { label = 'Edit', cls = '', align = 'left', onClose } = {}) {
  const box = el('div', `rmenu rpop ${cls}`);
  box.setAttribute('role', 'dialog');
  box.setAttribute('aria-label', label);
  const popup = show(box, anchor, { align, onClose });
  render(box, popup.close);
  place(box, anchor, align);
  return popup;
}
