// The view bar's cameras: four pinned in fixed places, and which preset the camera is at.

/** The cameras on the view bar, in their places: Iso, Top, Front, Close-up.  The other five are in the ⋯ menu. */
export const PINNED_VIEWS = ['painted', 'top', 'front', 'close'];

/** How near a camera must be to a preset's direction to count as at it, degrees. */
export const AT_DEGREES = 2;

const rad = (d) => (d * Math.PI) / 180;
const dir = (elev, azim) => [Math.cos(rad(elev)) * Math.cos(rad(azim)), Math.cos(rad(elev)) * Math.sin(rad(azim)), Math.sin(rad(elev))];
/** The angle between two camera directions, degrees. */
const between = (a, b) => (Math.acos(Math.max(-1, Math.min(1, a[0] * b[0] + a[1] * b[1] + a[2] * b[2]))) * 180) / Math.PI;

/**
 * The preset the camera is at, or null: the one whose direction is within AT_DEGREES of the camera's (`elev`, `azim`, degrees, as
 * `VIEWS` writes them), the nearest if two are; `prefer` (the preset the framing last placed it on) wins a tie.  The state follows the camera, not the click: an orbit that
 * comes back to a preset lights it, and one that leaves puts it out.
 * @param {Array<[string, string, number, number, boolean]>} views
 */
export function presetAt(views, elev, azim, prefer = null) {
  const cam = dir(elev, azim);
  const near = views.map(([name, , e, a]) => [name, between(cam, dir(e, a))]).filter(([, angle]) => angle <= AT_DEGREES);
  if (prefer && near.some(([name]) => name === prefer)) return prefer;
  near.sort((x, y) => x[1] - y[1]);
  return near[0]?.[0] ?? null;
}
