// The viewer's fade: what is dimmed beside a selection or a hover, the stale mesh
// while a new build comes, the pulse of work still going, and the paint.
//
// Methods of `Viewer`, installed on its prototype by `viewer.js`: they run
// with `this` the viewer.

import * as THREE from 'three';
import { FADE, FADE_IN, FADE_OUT, HOVER_LIFT, HOVER_REST, HOVER_S, PULSE, PULSE_S, REGION_FADE, SKETCH_ROUGH, STALE, easeInOutCubic, reduceMotion } from './consts.js';

export const fadeMethods = {
  /** The mesh shown is an older build's (a new one is being built): fade all of it. */
  setStale(on) {
    if (this.stale === !!on) return;
    this.stale = !!on;
    this.applyFade();
    this.pulse();
    this.draw();
  },

  /** Whether anything shown stands for work still going: a rough stand-in, or the old build alone. */
  pulsing() {
    if (reduceMotion()) return false;
    const rough = [...this.sketches.values()].some((k) => k.rough);
    return rough || (this.stale && this.sketches.size === 0 && !!this.mesh);
  },

  /** Breathe what stands for work still going, for as long as there is any. */
  pulse() {
    if (this.pulseRaf || !this.pulsing()) return;
    const tick = (now) => {
      const w = 1 + PULSE * Math.sin((now / 1000) * ((2 * Math.PI) / PULSE_S));
      const going = this.pulsing();
      this.sketchLooks.rough.u.uGhost.value = SKETCH_ROUGH * (going ? w : 1);
      if (this.stale) this.ghost.u.uGhost.value = STALE * (going && this.sketches.size === 0 ? w : 1);
      this.draw();
      this.pulseRaf = going ? requestAnimationFrame(tick) : 0;
    };
    this.pulseRaf = requestAnimationFrame(tick);
  },

  /**
   * The colours to show: the acrylic paint, or unpainted grey plastic (the
   * overhang's red and orange kept when that is on, so print checks still
   * work). Grey is made here from the same buffers.
   */
  currentPaint() {
    if (this.showPaint) return this.showOverhang ? this.painted : this.clean;
    if (!this.grey) {
      const grey = new Uint8Array(this.clean.array.length).fill(168);
      const flagged = new Uint8Array(this.clean.array.length);
      const [p, c] = [this.painted.array, this.clean.array];
      for (let i = 0; i < p.length; i += 3) {
        if (p[i] !== c[i] || p[i + 1] !== c[i + 1] || p[i + 2] !== c[i + 2]) {
          flagged.set(p.subarray(i, i + 3), i);
        } else flagged.set([168, 168, 170], i);
      }
      this.grey = {
        plain: new THREE.BufferAttribute(grey, 3, true),
        flagged: new THREE.BufferAttribute(flagged, 3, true),
      };
    }
    return this.showOverhang ? this.grey.flagged : this.grey.plain;
  },

  refreshPaint() {
    if (this.mesh) this.mesh.geometry.setAttribute('paint', this.currentPaint());
    this.draw();
  },

  /** Tint the selection and the hovered thing: each {entry, piece} (piece null: the whole entry) or null. */
  setHighlight(sel, hov) {
    const put = (u, w) => u.value.set(w ? w.entry : -1, w && w.piece != null ? w.piece : -1);
    put(this.uniforms.uSel, sel);
    put(this.uniforms.uHov, hov);
    this.applyFade();
    this.draw();
  },

  applyFade() {
    // a build's sketches stand in for the older mesh, which steps aside
    const sketched = this.sketches.size > 0;
    if (this.mesh) this.mesh.visible = !this.stale && !sketched;
    this.ghost.u.uMode.value = this.stale ? 3 : 2;
    const u = this.uniforms, fx = this.fx;
    const sel = u.uSel.value, hov = u.uHov.value;
    const keep = u.uKeep.value, leave = u.uLeave.value, lift = u.uLift.value, drop = u.uDrop.value;
    const can = this.fade && !this.stale && !sketched;
    u.uTintSel.value = can ? 0 : 1;
    const same = (a, b) => a.x === b.x && a.y === b.y;
    // a selected region is the selected item: nothing in the tile stands out, all of it fades deeper
    const focus = !!this.regionLayer?.focus;
    const selecting = can && (sel.x >= 0 || focus);
    const hovering = can && hov.x >= 0 && !same(hov, sel);
    // where a thing stands now, whichever slot holds it
    const now = (w) => (same(w, keep) ? fx.keep : same(w, leave) ? fx.leave : same(w, lift) ? fx.lift : same(w, drop) ? fx.drop : fx.rest);
    let moved = false, selMoved = false;
    if (selecting && !same(sel, keep)) {
      // the selection moves: the new one rises from where it stands, the old one sinks
      const from = now(sel);
      if (keep.x >= 0) { leave.copy(keep); fx.leave = fx.keep; } else leave.set(-1, -1);
      keep.copy(sel); fx.keep = from;
      moved = selMoved = true;
    }
    const want = hovering ? hov : null;
    if (want ? !same(want, lift) : lift.x >= 0) {
      // the hover moves: what it left eases back into the rest
      const from = want ? now(want) : 0;
      if (lift.x >= 0) { drop.copy(lift); fx.drop = fx.lift; } else drop.set(-1, -1);
      if (want) { lift.copy(want); fx.lift = from; } else lift.set(-1, -1);
      moved = true;
    }
    const deep = focus ? REGION_FADE : FADE;
    const rest = selecting ? deep : hovering ? HOVER_REST : 1;
    const target = { rest, keep: 1, leave: rest, lift: selecting ? HOVER_LIFT : 1, drop: rest };
    const t = this.fxTarget;
    // a selection's change takes a selection's time; a hover's, a hover's
    const selChange = selMoved || (t.rest !== rest && (t.rest === FADE || rest === FADE || t.rest === REGION_FADE || rest === REGION_FADE));
    const retarget = Object.keys(target).some((k) => t[k] !== target[k]);
    if (!can || reduceMotion()) this.fadeTo(target, 0);
    // a hover moves less than a selection: timed against its own full move
    else if (moved || retarget) {
      if (selChange) this.fadeTo(target, selecting ? FADE_IN : FADE_OUT, 1 - deep);
      else this.fadeTo(target, HOVER_S, Math.max(1 - HOVER_REST, HOVER_LIFT - FADE));
    }
    this.showFade();
  },

  /** Ease the rest and each thing standing out of it to `target` over `seconds` (0: at once); a move shorter than `full` takes its share of the time. */
  fadeTo(target, seconds, full = 1 - FADE) {
    if (this.fadeTween) cancelAnimationFrame(this.fadeTween);
    this.fadeTween = null;
    this.fxTarget = target;
    const from = { ...this.fx };
    const far = Math.max(...Object.keys(target).map((k) => Math.abs(target[k] - from[k])));
    if (!seconds || far === 0) { Object.assign(this.fx, target); this.settleFade(); return; }
    // part of the way there already: the rest of the time, not all of it
    const ms = seconds * 1000 * Math.min(1, far / full);
    const t0 = performance.now();
    const tick = (now) => {
      const e = easeInOutCubic(Math.min(1, (now - t0) / ms));
      for (const k of Object.keys(target)) this.fx[k] = from[k] + (target[k] - from[k]) * e;
      this.fadeTween = e < 1 ? requestAnimationFrame(tick) : null;
      if (e >= 1) this.settleFade();
      this.showFade();
      this.draw();
    };
    this.fadeTween = requestAnimationFrame(tick);
  },

  /** At rest: what was left is part of the rest again; with nothing selected or hovered, nothing stands out. */
  settleFade() {
    const u = this.uniforms;
    u.uLeave.value.set(-1, -1);
    u.uDrop.value.set(-1, -1);
    this.fx.leave = this.fx.drop = this.fx.rest;
    if (u.uSel.value.x < 0 || !this.fade) u.uKeep.value.set(-1, -1);
  },

  /** Set the passes for the fade as it stands now. */
  showFade() {
    const sketched = this.sketches.size > 0;
    const [litDepth, litColour, ...rest] = this.ghosts;
    if (this.stale) {
      this.ghost.u.uGhost.value = STALE;
      for (const g of rest) g.visible = !sketched;
      if (litDepth) litDepth.visible = litColour.visible = false;
      this.styleRegions();
      return;
    }
    // something stands out of the fade, or is on its way in or out
    const u = this.uniforms, fx = this.fx;
    const focus = !!this.regionLayer?.focus && this.fade;
    const on = !sketched && (focus || [u.uKeep, u.uLeave, u.uLift, u.uDrop].some((w) => w.value.x >= 0));
    if (this.mesh) this.mesh.visible = !sketched && !on;
    this.ghost.u.uGhost.value = fx.rest;
    u.uKeepA.value = fx.keep;
    u.uLeaveA.value = fx.leave;
    u.uLiftA.value = fx.lift;
    u.uDropA.value = fx.drop;
    for (const g of this.ghosts) g.visible = on;
    this.styleRegions();
  },

  /** The theme's accent (`--accent`), used to tint selected geometry. */
  setAccent(css) {
    this.uniforms.uAccent.value.setStyle(css); // a bad theme token is a bug: let it throw to the page's error panel
    this.draw();
  }
};
