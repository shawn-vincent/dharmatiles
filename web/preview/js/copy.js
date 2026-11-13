// Copying: the control is view/copy-button.js; this is the words of a version as the About box shows them.


/** The parts of a version as the About box shows them: `0.9.0` and `+123.gfd383a7.dirty`. */
export function versionParts(v) {
  const at = v.indexOf('+');
  return at < 0 ? { semver: v, build: '', dirty: false } : { semver: v.slice(0, at), build: v.slice(at), dirty: /(^|[+.])dirty$/.test(v.slice(at)) };
}
