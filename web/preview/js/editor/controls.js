// Small controls the editor's panels share (the plates' panel, the inventory's).

/** A labelled segmented switch: `options` are `[value, text, title?]`; `on(value)` when another one is pressed. */
export function seg(label, options, current, on, disabled) {
  const box = document.createElement('div');
  box.className = 'pl-field';
  const lab = document.createElement('span');
  lab.className = 'pl-lab';
  lab.textContent = label;
  box.append(lab);
  const s = document.createElement('div');
  s.className = 'seg';
  s.setAttribute('role', 'group');
  s.setAttribute('aria-label', label);
  for (const [v, text, title] of options) {
    const b = document.createElement('button');
    b.textContent = text;
    b.type = 'button';
    b.setAttribute('aria-pressed', String(v === current));
    if (title) b.title = title;
    b.disabled = !!disabled;
    b.onclick = () => { if (v !== current) on(v); };
    s.append(b);
  }
  box.append(s);
  return box;
}
