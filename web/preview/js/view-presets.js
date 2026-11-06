export const FIXED_VIEWS = ['painted', 'top'];

/** Keep the two most recently used non-fixed views, newest first. */
export function rememberView(recent, name) {
  if (FIXED_VIEWS.includes(name)) return recent.slice(0, 2);
  return [name, ...recent.filter((view) => view !== name)].slice(0, 2);
}

/** ISO and Map are permanent; two recent alternatives follow them. */
export function shortcutViews(recent) {
  return [...FIXED_VIEWS, ...recent.filter((view) => !FIXED_VIEWS.includes(view)).slice(0, 2)];
}
