// A written control as the program sends it to the sheet (`WrittenControl`, preview/stack.rs): the whole control as written,
// its span, the value alone and the value as words.  The fixtures say what the program would; the page reads all four and
// works none out.
/**
 * @param {string} name @param {string} whole the control as written: `name=value`
 * @param {[number, number]} span
 */
export function setting(name, whole, span) {
  const value = whole.startsWith(`${name}=`) ? whole.slice(name.length + 1) : whole;
  const quoted = /^"([\s\S]*)"$/.exec(value);
  return { name, text: whole, span, value, plain: quoted ? quoted[1] : value, file: null, from: null };
}

/** What a control shows where it is not written (`UnwrittenControl`): as written, and as words. */
/** @param {string} control @param {string} text @param {string} from */
export function unwritten(control, text, from) {
  const quoted = /^"([\s\S]*)"$/.exec(text);
  return { control, text, plain: quoted ? quoted[1] : text, from };
}
