/* ---------- hand recipes: the four reference rocks drawn by hand (lengths in fh) ---------- */
ROCK.RECIPES = {
  R1C8: { kind: 'block', seed: 11, W: .62, D: .52, H: .52, kinks: 3, lobes: [
    { x0: 0, x1: .28, y0: 0, y1: .52, h: .52 }, { x0: .28, x1: .62, y0: 0, y1: .3, h: .52 }, { x0: .28, x1: .62, y0: .3, y1: .52, h: .5 }] },
  R2C4: { kind: 'pillar', seed: 12, W: .52, D: .4, H: .95, kinks: 3, lean: [.03, 0], jf: .3, lobes: [
    { x0: .26, x1: .52, y0: 0, y1: .4, h: .95 }, { x0: 0, x1: .26, y0: 0, y1: .4, h: .42 }] },
  R1C7: { kind: 'ramp', seed: 13, W: .6, D: .75, H: .75, kinks: 3, lobes: [
    { x0: 0, x1: .6, y0: .3, y1: .75, h: .75, sy: .7 }, { x0: .4, x1: .6, y0: 0, y1: .3, h: .435, sy: .7 }, { x0: 0, x1: .4, y0: 0, y1: .3, h: .24 }] },
  R1C5: { kind: 'slab', seed: 15, W: .9, D: .62, H: .3, kinks: 1, jf: .2, chamfer: { top: [.09, .14], vert: [.1, .16], bottom: [.08, .12] }, lobes: [
    { x0: 0, x1: .56, y0: 0, y1: .62, h: .28 }, { x0: .56, x1: .9, y0: 0, y1: .62, h: .28 }] },
};

/* ---------- the generator: a recipe drawn from a seed, in the family the hand recipes span ---------- */
ROCK.MIX = { slab: .22, block: .30, pillar: .22, ramp: .26 };
ROCK.random = function (seed, opts) {
  const r = ROCK.mulberry(seed * 2654435761 + 17), U = (a, b) => a + (b - a) * r(), pickK = (t) => { let s = 0; for (const k in t) s += t[k]; let x = r() * s; for (const k in t) { x -= t[k]; if (x < 0) return k; } return Object.keys(t)[0]; };
  let kind = opts && opts.kind && opts.kind !== 'any' ? opts.kind : pickK(ROCK.MIX), stepped = false;
  if (kind === 'stepped') { kind = pickK({ block: .5, pillar: .5 }); stepped = true; }
  const rc = { kind, seed, lobes: [], kinks: 1 + Math.floor(r() * 3), jf: U(.2, .32), lean: [0, 0] };
  let W, D, H;
  const splitsAt = (n, lo, hi) => { const cuts = []; for (let i = 0; i < n; i++) cuts.push(U(lo, hi)); cuts.sort((a, b) => a - b); return cuts.filter((c, i) => i === 0 || c - cuts[i - 1] > .3); };
  if (kind === 'slab') {
    W = U(.75, 1.1); D = W * U(.6, .76); H = W * U(.28, .36); rc.chamfer = { top: [.09, .14], vert: [.1, .16], bottom: [.08, .12] }; rc.kinks = Math.min(rc.kinks, 1);
    const cuts = splitsAt(r() < .5 ? 1 : 2, .3, .7), xs = [0, ...cuts.map((c) => c * W), W];
    for (let i = 0; i < xs.length - 1; i++) rc.lobes.push({ x0: xs[i], x1: xs[i + 1], y0: 0, y1: D, h: H - (i && r() < .25 ? U(.025, .04) : 0) });
    if (stepped || r() < .12) rc.lobes[rc.lobes.length - 1].h = H * U(.6, .75);
  } else if (kind === 'block') {
    W = U(.5, .75); D = W * U(.75, .95); H = W * U(.68, .95);
    const cuts = splitsAt(pickK({ 0: .2, 1: .45, 2: .35 }) | 0, .3, .7), xs = [0, ...cuts.map((c) => c * W), W];
    for (let i = 0; i < xs.length - 1; i++) rc.lobes.push({ x0: xs[i], x1: xs[i + 1], y0: 0, y1: D, h: H - (i && r() < .3 ? U(.025, .045) : 0) });
    const last = rc.lobes[rc.lobes.length - 1];
    if (rc.lobes.length > 1 && r() < .55) { const yc = D * U(.45, .65), q = rc.lobes.pop(); rc.lobes.push({ ...q, y1: yc }, { ...q, y0: yc, h: q.h - (r() < .5 ? 0 : U(.025, .045)) }); }
    if (stepped || r() < .22) { const q = rc.lobes[0]; if (rc.lobes.length < 2) { const u = W * U(.35, .5); rc.lobes = [{ x0: 0, x1: u, y0: 0, y1: D, h: H }, { x0: u, x1: W, y0: 0, y1: D, h: H }]; } rc.lobes[0].h = H * U(.45, .65); }
  } else if (kind === 'pillar') {
    W = U(.42, .56); D = W * U(.7, .9); H = Math.min(1.3, W * U(1.7, 2.1)); rc.lean[0] = r() < .4 ? U(-.03, .03) : 0;
    if (stepped || r() < .5) { const u = W * U(.38, .52), side = r() < .7; rc.lobes = side ? [{ x0: 0, x1: u, y0: 0, y1: D, h: H * U(.35, .5) }, { x0: u, x1: W, y0: 0, y1: D, h: H }] : [{ x0: 0, x1: W - u, y0: 0, y1: D, h: H }, { x0: W - u, x1: W, y0: 0, y1: D, h: H * U(.35, .5) }]; }
    else if (r() < .5) { const u = W * U(.35, .65); rc.lobes = [{ x0: 0, x1: u, y0: 0, y1: D, h: H }, { x0: u, x1: W, y0: 0, y1: D, h: H - (r() < .5 ? 0 : U(.025, .04)) }]; }
    else rc.lobes = [{ x0: 0, x1: W, y0: 0, y1: D, h: H }];
  } else {
    W = U(.5, .7); D = U(.65, .85); H = U(.62, .82); const hL = H * U(.22, .32), sy = (H - hL) / D, hAt = (y) => H - sy * (D - y);
    if (stepped || r() < .6) { const wx = W * U(.35, .5), wy = D * U(.3, .4); rc.lobes = [{ x0: 0, x1: W, y0: wy, y1: D, h: H, sy }, { x0: wx, x1: W, y0: 0, y1: wy, h: hAt(wy), sy }, { x0: 0, x1: wx, y0: 0, y1: wy, h: hAt(wy) * U(.45, .6) }]; }
    else if (r() < .4) { const u = W * U(.35, .65); rc.lobes = [{ x0: 0, x1: u, y0: 0, y1: D, h: H, sy }, { x0: u, x1: W, y0: 0, y1: D, h: H, sy }]; }
    else rc.lobes = [{ x0: 0, x1: W, y0: 0, y1: D, h: H, sy }];
  }
  Object.assign(rc, { W, D, H });
  return rc;
};
