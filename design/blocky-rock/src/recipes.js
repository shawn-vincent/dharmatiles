/* ---------- hand recipes: the four reference rocks drawn by hand (lengths in fh) ---------- */
ROCK.RECIPES = {
  R1C8: { kind: 'block', seed: 21, W: .62, D: .52, H: .52, kinks: 3, chips: 4, lobes: [
    { x0: 0, x1: .26, y0: 0, y1: .52, h: .52 }, { x0: .26, x1: .62, y0: 0, y1: .3, h: .52 }, { x0: .26, x1: .62, y0: .3, y1: .52, h: .5 }] },
  R2C4: { kind: 'pillar', seed: 22, W: .52, D: .4, H: .95, kinks: 3, chips: 3, lean: [.02, 0], jf: .25, lobes: [
    { x0: .26, x1: .52, y0: 0, y1: .4, h: .95 }, { x0: 0, x1: .26, y0: 0, y1: .4, h: .42 }] },
  /* one ramp, rising to the back, with the seat cut down into its front-left corner: the back piece is the whole ramp, the seat is a level shelf */
  R1C7: { kind: 'ramp', seed: 23, W: .66, D: .8, H: .72, kinks: 3, chips: 2, jf: .12, lobes: [
    { x0: 0, x1: .66, y0: .32, y1: .8, h: .72, sy: .62 }, { x0: .4, x1: .66, y0: 0, y1: .32, h: .52, sy: .62 }, { x0: 0, x1: .4, y0: 0, y1: .32, h: .24, sy: .22 }] },
  R1C5: { kind: 'slab', seed: 25, W: .9, D: .62, H: .3, kinks: 1, chips: 3, jf: .15, chamfer: { top: [.07, .11], vert: [.09, .14], bottom: [.06, .1] }, lobes: [
    { x0: 0, x1: .3, y0: 0, y1: .62, h: .3 }, { x0: .3, x1: .64, y0: 0, y1: .62, h: .3 }, { x0: .64, x1: .9, y0: 0, y1: .62, h: .29 }] },
};

/* ---------- the generator: a recipe drawn from a seed, in the family the hand recipes span ---------- */
ROCK.MIX = { slab: .22, block: .30, pillar: .22, ramp: .26 };
ROCK.random = function (seed, opts) {
  const r = ROCK.mulberry(seed * 2654435761 + 17), U = (a, b) => a + (b - a) * r(), pickK = (t) => { let s = 0; for (const k in t) s += t[k]; let x = r() * s; for (const k in t) { x -= t[k]; if (x < 0) return k; } return Object.keys(t)[0]; };
  let kind = opts && opts.kind && opts.kind !== 'any' ? opts.kind : pickK(ROCK.MIX), stepped = false;
  if (kind === 'stepped') { kind = pickK({ block: .5, pillar: .5 }); stepped = true; }
  const rc = { kind, seed, lobes: [], crack: r() < .22, kinks: 1 + Math.floor(r() * 3), jf: U(.15, .25), lean: [0, 0], chips: +pickK({ 2: .2, 3: .3, 4: .3, 5: .2 }) };
  let W, D, H;
  const lobesX = (xs, ys, hs) => { for (let i = 0; i < xs.length - 1; i++) rc.lobes.push({ x0: xs[i], x1: xs[i + 1], y0: 0, y1: D, h: hs[i] }); };
  if (kind === 'slab') {
    W = U(.75, 1.1); D = W * U(.6, .76); H = W * U(.28, .36); rc.chamfer = { top: [.07, .11], vert: [.09, .14], bottom: [.06, .1] }; rc.kinks = Math.min(rc.kinks, 1); rc.chips = Math.min(rc.chips, 4);
    const nlob = pickK({ 1: .3, 2: .35, 3: .35 }) | 0, c1 = W * U(.28, .45), c2 = W * U(.62, .76), xs = nlob === 1 ? [0, W] : nlob === 2 ? [0, c1, W] : [0, c1, c2, W];
    lobesX(xs, 0, xs.slice(0, -1).map((_, i) => H - (i && r() < .3 ? U(.012, .03) : 0)));
    if (stepped || r() < .12) rc.lobes[rc.lobes.length - 1].h = H * U(.6, .75);
  } else if (kind === 'block') {
    W = U(.5, .75); D = W * U(.75, .95); H = W * U(.68, .95);
    const nlob = pickK({ 1: .45, 2: .3, 3: .25 }) | 0, c1 = W * U(.3, .48), c2 = W * U(.62, .78), xs = nlob === 1 ? [0, W] : nlob === 2 ? [0, c1, W] : [0, c1, c2, W];
    lobesX(xs, 0, xs.slice(0, -1).map((_, i) => H - (i && r() < .3 ? U(.012, .03) : 0)));
    if (nlob === 3 && r() < .5) { const yc = D * U(.45, .65), q = rc.lobes.pop(); rc.lobes.push({ ...q, y1: yc }, { ...q, y0: yc, h: q.h - U(.012, .03) }); }
    if (stepped || r() < .22) { if (rc.lobes.length < 2) { const u = W * U(.35, .5); rc.lobes = [{ x0: 0, x1: u, y0: 0, y1: D, h: H }, { x0: u, x1: W, y0: 0, y1: D, h: H }]; } rc.lobes[0].h = H * U(.45, .65); }
  } else if (kind === 'pillar') {
    W = U(.42, .56); D = W * U(.7, .9); H = Math.min(1.3, W * U(1.7, 2.1)); rc.lean[0] = r() < .4 ? U(-.03, .03) : 0;
    if (stepped || r() < .5) { const u = W * U(.38, .52), side = r() < .7; rc.lobes = side ? [{ x0: 0, x1: u, y0: 0, y1: D, h: H * U(.35, .5) }, { x0: u, x1: W, y0: 0, y1: D, h: H }] : [{ x0: 0, x1: W - u, y0: 0, y1: D, h: H }, { x0: W - u, x1: W, y0: 0, y1: D, h: H * U(.35, .5) }]; }
    else rc.lobes = [{ x0: 0, x1: W, y0: 0, y1: D, h: H }];
  } else {
    W = U(.5, .7); D = U(.65, .85); H = U(.62, .82); const hL = H * U(.22, .32), sy = (H - hL) / D, hAt = (y) => H - sy * (D - y);
    if (stepped || r() < .6) { const wx = W * U(.35, .5), wy = D * U(.3, .4); rc.lobes = [{ x0: 0, x1: W, y0: wy, y1: D, h: H, sy }, { x0: wx, x1: W, y0: 0, y1: wy, h: hAt(wy), sy }, { x0: 0, x1: wx, y0: 0, y1: wy, h: hAt(wy) * U(.45, .6) }]; rc.jf = .12; }
    else rc.lobes = [{ x0: 0, x1: W, y0: 0, y1: D, h: H, sy }];
  }
  Object.assign(rc, { W, D, H });
  return rc;
};
