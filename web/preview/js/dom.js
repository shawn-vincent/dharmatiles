// @ts-check
// The one lookup of an element the page's own markup (or a piece the code just built) always has: it throws, naming
// what is missing, rather than hand back `null` for the next line to trip over.

/**
 * The first element under `root` that matches `selector`.  A miss is a bug in the markup the code wrote, so it is loud.
 * @template {Element} [T=HTMLElement]
 * @param {ParentNode} root
 * @param {string} selector
 * @returns {T}
 */
export function need(root, selector) {
  const found = root.querySelector(selector);
  if (!found) throw new Error(`The page has no element matching "${selector}" where it was looked for.`);
  return /** @type {T} */ (found);
}
