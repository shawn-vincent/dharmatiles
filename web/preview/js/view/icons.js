// The one icon set (docs/standards/visual-system.html, "Icons"): every icon of the page is drawn here and put on the
// page by `Icon` (a component), `iconSvg` (a string) or `fillIcons` (index.html's `<i data-icon>` marks); no icon is a
// text character.  The first group is Lucide (ISC licence, web/preview/vendor/LICENSE-lucide), a 24 grid with stroke 2,
// round caps and joins, scaled to the size asked; the second is the page's own (the panel, the dice, the folders); the
// third is the nine cameras, drawn after FreeCAD's standard views (a see-through cube with the face you look at filled
// for the six sides; the cube solid for Iso, Low and Close-up), in `currentColor` and the accent.
import { html } from './html.js';

const LUCIDE = {
  'chevron-right': '<path d="m9 18 6-6-6-6"/>',
  'chevron-left': '<path d="m15 18-6-6 6-6"/>',
  'chevron-down': '<path d="m6 9 6 6 6-6"/>',
  ellipsis: '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  minus: '<path d="M5 12h14"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  'rotate-ccw': '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
  'rotate-cw': '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
  moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>',
  'external-link': '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
  'arrow-down': '<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>',
  copy: '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
  'undo-2': '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>',
  'redo-2': '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"/>',
  'panel-left': '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/>',
  'panel-bottom': '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 15h18"/>',
  // the page's own: the folder row glyphs (a folder, a house, shelves, a flask, a disk, a file, a solid) and the seed's dice
  folder: '<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2.5h8.5A1.5 1.5 0 0 1 21 10v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z"/>',
  home: '<path d="M4 11 12 4l8 7M6.5 9.5V19h11V9.5M10 19v-5h4v5"/>',
  library: '<path d="M5 5h3.5v14H5zM10.5 5H14v14h-3.5zM15.6 6.4l3.3-.9 2.6 12.6-3.3.9z"/>',
  tests: '<path d="M9.5 4h5M10.5 4v5.5L5.5 18a1.4 1.4 0 0 0 1.2 2h10.6a1.4 1.4 0 0 0 1.2-2l-5-8.5V4M8 15h8"/>',
  added: '<path d="M3.5 9.5 6 5h12l2.5 4.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5zM3.5 9.5h17M16.5 14.5h.01"/>',
  files: '<path d="M7 3.5h7l4 4V20H7zM14 3.5V8h4"/>',
  stl: '<path d="M12 3.5 20 8v8l-8 4.5L4 16V8zM4 8l8 4.5L20 8M12 12.5v8"/>',
  dice: '<rect x="3" y="3" width="18" height="18" rx="4"/><circle cx="9" cy="9" r="1.2" fill="currentColor"/><circle cx="15" cy="9" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="9" cy="15" r="1.2" fill="currentColor"/><circle cx="15" cy="15" r="1.2" fill="currentColor"/>',
};

const CAMERAS = {
  'view-iso': '<path d="M12 3 L20 7.5 L12 12 L4 7.5z" fill="var(--accent)" fill-opacity=".3" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 7.5 L12 12 L12 21 L4 16.5z" fill="var(--accent)" fill-opacity=".6" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M12 12 L20 7.5 L20 16.5 L12 21z" fill="var(--accent)" fill-opacity=".9" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-top': '<path d="M4 9 L15 9 L20 4 L9 4z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-front': '<path d="M4 9 L15 9 L15 20 L4 20z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-right': '<path d="M15 9 L20 4 L20 15 L15 20z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-back': '<path d="M9 4 L20 4 L20 15 L9 15z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-left': '<path d="M4 9 L9 4 L9 15 L4 20z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-under': '<path d="M4 20 L15 20 L20 15 L9 15z" fill="var(--accent)" fill-opacity=".8" stroke="none"/><path d="M9 4 L9 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L20 15" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M9 15 L4 20" stroke="currentColor" stroke-width="1" stroke-opacity=".45"/><path d="M4 9 L15 9 L15 20 L4 20z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 9 L9 4 L20 4 L15 9 M20 4 L20 15 L15 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-low': '<path d="M1.5 21.5h21" stroke="currentColor" stroke-width="1.1" stroke-dasharray="2 2"/><path d="M12 6.5 L20 8.5 L12 10.5 L4 8.5z" fill="var(--accent)" fill-opacity=".3" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M4 8.5 L12 10.5 L12 20 L4 18z" fill="var(--accent)" fill-opacity=".6" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M12 10.5 L20 8.5 L20 18 L12 20z" fill="var(--accent)" fill-opacity=".9" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/>',
  'view-close': '<clipPath id="CROPID"><rect x="3" y="3" width="18" height="18" rx="2"/></clipPath><g clip-path="url(#CROPID)"><path d="M12 -1 L27 7.5 L12 16 L-3 7.5z" fill="var(--accent)" fill-opacity=".3" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M-3 7.5 L12 16 L12 30 L-3 21.5z" fill="var(--accent)" fill-opacity=".6" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/><path d="M12 16 L27 7.5 L27 21.5 L12 30z" fill="var(--accent)" fill-opacity=".9" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round"/></g><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="currentColor" stroke-width="1.1" stroke-dasharray="2 2"/>',
};

/** name -> the svg's inner markup, on a 24 grid. */
export const ICONS = { ...LUCIDE, ...CAMERAS };
/** The icon of a camera preset (a name of VIEWS in viewer/consts.js). */
export const cameraIcon = (view) => `view-${view === 'painted' ? 'iso' : view}`;

let uid = 0;
/** The markup of an icon (a camera's clip path gets an id of its own: two of one icon in a page must not share it). */
function inner(name) {
  const g = ICONS[name];
  if (g == null) throw new Error(`icons.js: there is no icon named "${name}"`);
  return g.includes('CROPID') ? g.replaceAll('CROPID', `crop${++uid}`) : g;
}

/** An icon as a string of svg (for markup built by hand). */
export function iconSvg(name, size = 16) {
  return `<svg class="ico${name.startsWith('view-') ? ' cam' : ''}" viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true">${inner(name)}</svg>`;
}

/** An icon as a component: `<${Icon} name="chevron-right" />`; `size` is 16 (14 inline, 20 for a camera). */
export function Icon({ name, size = 16 }) {
  return html`<svg class=${`ico${name.startsWith('view-') ? ' cam' : ''}`} viewBox="0 0 24 24" width=${size} height=${size} aria-hidden="true" dangerouslySetInnerHTML=${{ __html: inner(name) }} />`;
}

/** Draw the icon of every `<i data-icon="name" [data-size="14"]>` under `root` (index.html's own marks). */
export function fillIcons(root) {
  for (const el of root.querySelectorAll('i[data-icon]')) el.outerHTML = iconSvg(el.dataset.icon, Number(el.dataset.size) || 16);
}
