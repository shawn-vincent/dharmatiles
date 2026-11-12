// Drafts: while a value is dragged the view shows quick builds of the text
// as it is, one in flight at a time, always asked for the latest text.  A
// draft is stamped with the text it was built from: a reply is shown only
// when that is still the text, and no full build of the current text has
// landed meanwhile (`supersede`), else it would sit over a newer mesh.
//
//   text()              the text now (null: nothing to draft)
//   request(text)       the draft of `text`: a promise of its reply
//   accept(reply, text) show a reply that is still current
//   fail(err)           a request that threw

export function createDrafter({ text, request, accept, fail }) {
  let drafting = false, again = false, epoch = 0;
  async function ask() {
    if (drafting) { again = true; return; }
    const sent = text();
    if (sent == null) return;
    drafting = true; again = false;
    const mine = epoch;
    try {
      const reply = await request(sent);
      if (mine === epoch && text() === sent) accept(reply, sent); // else: stale, dropped
    } catch (e) {
      fail(e);
    }
    drafting = false;
    if (again) ask();
  }
  return {
    ask,
    /** A full build of the text now has landed: a draft still on its way is of no use. */
    supersede() { epoch++; },
  };
}
