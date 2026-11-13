// The printers the choosers list, as groups: your own first, then each brand, then the profiles
// that are only floors.  An AMS-style variant (`x1c-ams` beside `x1c`) is not a model of its own:
// it is the model's switch.  No DOM here (the chooser is js/editor/printer-chooser.js).

const AMS = /-ams$/;

/** `[{label, kind, models: [{name, title, p, ams}]}]` for the printers that match `query`. */
export function groupPrinters(printers, { needBed = false, query = '' } = {}) {
  const have = new Set(printers.map((p) => p.name));
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const models = [];
  for (const p of printers) {
    if (needBed && !p.bed && !p.broken) continue;
    // a variant folds into its plain model when that is listed too
    if (AMS.test(p.name) && have.has(p.name.replace(AMS, ''))) continue;
    const ams = printers.find((q) => q.name === `${p.name}-ams`) || null;
    // in your own group the brand is not a heading above it, so the name carries it
    const title = p.mine && p.brand && p.model ? `${p.brand} ${p.model}` : p.model || p.label || p.name;
    const hay = `${p.brand || ''} ${title} ${p.name} ${p.label || ''}`.toLowerCase();
    if (!words.every((w) => hay.includes(w))) continue;
    models.push({ name: p.name, title, p, ams });
  }
  const by = (label, kind, pick) => ({ label, kind, models: models.filter(pick) });
  const out = [by('Yours', 'mine', (m) => m.p.mine)];
  const brands = [...new Set(models.filter((m) => !m.p.mine && (m.p.bed || m.p.broken) && m.p.brand).map((m) => m.p.brand))].sort((a, b) => a.localeCompare(b));
  for (const b of brands) out.push(by(b, 'brand', (m) => !m.p.mine && m.p.brand === b));
  out.push(by('Floors only', 'floors', (m) => !m.p.mine && !m.p.bed && !m.p.broken));
  // a library profile that names no brand, with a bed
  out.push(by('Other', 'brand', (m) => !m.p.mine && m.p.bed && !m.p.brand));
  return out.filter((g) => g.models.length);
}

/** The plain model a profile name stands for in the list, and whether the AMS variant is chosen. */
export function selectionOf(printers, name) {
  const have = new Set(printers.map((p) => p.name));
  if (AMS.test(name) && have.has(name.replace(AMS, ''))) return { model: name.replace(AMS, ''), ams: true };
  return { model: name, ams: false };
}

/** The model's size in words: `256 × 256 mm · 1 filament`. */
export function sizeWords(p) {
  if (!p.bed) return 'printability floors only';
  const f = p.slots === 1 ? '1 filament' : `${p.slots} filaments`;
  return `${p.bed[0]} × ${p.bed[1]} mm · ${f}`;
}
