/* settle.js — owns stage 2c, which pieces stay. Takes the treated pieces and the fall size; hands on each piece's state.
   CONTACT: two pieces touch when they share a joint key and the faces overlap by at least `minContact` mm^2: one function for
   neighbours in a stratum and across strata. HELD: a seat face, or a held joint face, of at least `minContact` (a top joint holds the stone for the union to be one body, not for printing: overhang stays the gate's job).
   The fixpoint: a piece falls when it is smaller than `fall`, not held and exposed (it has a surface face, or touches a fallen piece),
   or when it is not part of the one body the standing pieces make through contacts (the biggest that rests on the seat): a loose piece, even one resting on the ground, is not the stone. Both rules run until nothing changes (falls only grow, so it ends). No round caps.
   HELD PIECES NEVER FALL BY SIZE: fall peels the exposed pieces above the seat only.
   state: 'standing' | 'fell:size' | 'fell:float'. */
var Stone = Stone || {};
(function (S) {
  const { overlapArea } = S.convex;
  const MIN_CONTACT = 1;   // mm^2 at least; wear raises it so a joint stays wider than the groove wear cuts (see contactFor)
  const contactFor = (wearRadius) => Math.max(MIN_CONTACT, (2 * wearRadius) * (2 * wearRadius));

  const jointIndex = (pieces) => { const joints = new Map(); for (const p of pieces) for (const f of p.convex.faces) if (f.role === 'joint') { let l = joints.get(f.nb); if (!l) joints.set(f.nb, l = []); l.push({ p, f }); } return joints; };
  const touches = (a, b, minContact) => a.p !== b.p && overlapArea(a.f, b.f) >= minContact;   // the one contact test
  /* OUTER: a face is outer when the solid across it is not stone: the region's boundary, or a neighbour that fell (or is no piece at all).
     The joint faces of standing pieces that no standing neighbour touches: after settle these become surface (treat.expose), and everything after reads that. */
  function outerFaces(pieces, minContact) {
    const standing = pieces.filter((p) => p.state === 'standing'), out = [];
    for (const l of jointIndex(standing).values()) for (const a of l) if (!a.p.convex.held(a.f) && !l.some((b) => touches(a, b, minContact))) out.push(a);   // a held joint's neighbour is outside the stone: never exposed
    return out;
  }
  function settle(pieces, fall, minContact) {
    const joints = jointIndex(pieces);                 // key -> [{p, f}]; a fallen piece keeps its state when settle runs again
    for (const p of pieces) { p.size = Math.cbrt(p.convex.volume()); p.held = p.convex.heldArea() >= minContact; p.exposed = p.convex.faces.some((f) => f.role === 'surface'); p.contacts = []; p.state = p.state || 'standing'; }
    for (const l of joints.values()) for (let i = 0; i < l.length; i++) for (let j = i + 1; j < l.length; j++)
      if (touches(l[i], l[j], minContact)) { l[i].p.contacts.push(l[j].p); l[j].p.contacts.push(l[i].p); }
    const stands = (p) => p.state === 'standing', anyHeld = pieces.some((p) => p.held);
    let changed = true;
    while (changed) {
      changed = false;
      if (fall > 0) {                                  // outside in: everything eligible this round goes together
        const go = pieces.filter((p) => stands(p) && !p.held && p.size < fall && (p.exposed || p.contacts.some((o) => !stands(o))));
        for (const p of go) p.state = 'fell:size';
        if (go.length) changed = true;
      }
      if (anyHeld) {                                   // the stone is one body: the standing pieces joined through contacts, resting on the seat
        const comp = new Map(); let main = null, big = -1;
        for (const p of pieces) if (stands(p) && !comp.has(p)) {
          const members = [p]; comp.set(p, members); let held = false, vol = 0;
          for (let i = 0; i < members.length; i++) { const q = members[i]; held = held || q.held; vol += q.convex.volume(); for (const o of q.contacts) if (stands(o) && !comp.has(o)) { comp.set(o, members); members.push(o); } }
          if (held && vol > big) { big = vol; main = members; }
        }
        for (const p of pieces) if (stands(p) && comp.get(p) !== main) { p.state = 'fell:float'; changed = true; }
      }
    }
    const open = new Set(), stk = pieces.filter((p) => !stands(p) && p.exposed); stk.forEach((p) => open.add(p));
    while (stk.length) { const p = stk.pop(); for (const o of p.contacts) if (!stands(o) && !open.has(o)) { open.add(o); stk.push(o); } }
    const sealed = pieces.filter((p) => !stands(p) && !open.has(p)).length;   // fallen pieces with no way out: a void inside the stone
    return { anyHeld, sealed };
  }
  S.settle = { settle, outerFaces, touches, jointIndex, contactFor, MIN_CONTACT };
})(Stone);
