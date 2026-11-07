//! One leaf: its seat on the crown, its blade, and the checks it must pass.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fail {
    Oval,
    Skew,
    Neck,
    Error,
    Drape,
}

#[allow(clippy::large_enum_variant, reason = "short-lived, one per root")]
pub(super) enum Outcome {
    Placed(TriMesh, V),
    Branch,
    Failed(Fail),
}

pub(super) struct Job<'a> {
    pub(super) crown: &'a Crown,
    pub(super) bodies: &'a [Body],
    pub(super) shape: &'a LeafShape,
    pub(super) tris: &'a [[u32; 3]],
}

impl Job<'_> {
    /// Re-centre on the root, attempt, cull skewers.
    #[allow(clippy::too_many_arguments, reason = "the per-root inputs")]
    pub(super) fn leaf(
        &self,
        base: V,
        nrm: V,
        t_leaf: V,
        lying_nz: f64,
        standoff: f64,
        ls: f64,
        ws: f64,
    ) -> Outcome {
        let on_top = nrm[2] >= TOP_NORMAL_Z;
        let st = leaf_zone(nrm[2], lying_nz);
        let flush = st < 0.5;
        let mut arch_mm = (1.0 - st) * FLUSH_ARCH_MM;
        let mut blade = Blade {
            arch_deg: leaf::ARCH_DEG,
            length_mm: ls,
            width_mm: ws,
            thickness_mm: self.shape.thickness_mm,
            fold_angle_deg: self.shape.fold_angle_deg,
            inner_curve: self.shape.inner_curve,
            outer_curve: self.shape.outer_curve,
            curl_deg: st * self.shape.curl_deg.min(PITCH_CURL_DEG),
            lift_mm: st * TIP_LIFT_MM,
        };
        let support_nz = if on_top { nrm[2] } else { lying_nz };
        let k = hang_scale(support_nz, self.shape.overhang_deg, &blade, arch_mm);
        blade.arch_deg *= k;
        blade.curl_deg *= k;
        blade.lift_mm *= k;
        arch_mm *= k;
        // Centre the blade on its root: the build anchors the oval's centre,
        // which sits a quarter length down-slope of the blade's middle.
        let (anchor, anchor_n) = self
            .crown
            .project(
                add(base, scale(t_leaf, 0.25 * ls)),
                nrm,
                1.0 + 0.25 * ls + 1.0,
            )
            .unwrap_or((base, nrm));
        let built = if flush {
            self.drape_turning(base, nrm, t_leaf, &blade, drape_embed(standoff))
        } else {
            self.attempt(
                anchor,
                anchor_n,
                t_leaf,
                &blade,
                st * standoff,
                flush,
                0.02 + st * TIP_CEIL_RANGE_MM,
                arch_mm,
            )
        };
        // A pitched blade whose tip is its lowest point and hangs free of
        // the crown below it would start a layer in air: lay it flat
        // instead.
        let built = match built {
            Ok((surf, _, _)) if !on_top && self.hangs(&surf) => {
                self.drape_turning(base, nrm, t_leaf, &blade, drape_embed(standoff))
            }
            b => b,
        };
        let (surf, inner, tangent) = match built {
            Ok(x) => x,
            Err(f) => return Outcome::Failed(f),
        };
        // Skewer cull: a blade vertex inside a branch that is outside the
        // crown pierces visibly; inside both, it is hidden and harmless.
        let mut in_branch = vec![false; surf.len()];
        for b in self.bodies {
            if (0..3).any(|k| base[k] < b.lo[k] - ls || base[k] > b.hi[k] + ls) {
                continue;
            }
            for (f, x) in in_branch.iter_mut().zip(b.rays.contains(&surf)) {
                *f |= x;
            }
        }
        let hits: Vec<V> = surf
            .iter()
            .zip(&in_branch)
            .filter(|(_, f)| **f)
            .map(|(p, _)| *p)
            .collect();
        if !hits.is_empty() && self.crown.rays.contains(&hits).iter().any(|x| !x) {
            return Outcome::Branch;
        }
        Outcome::Placed(leaf::solidify(&surf, &inner, self.tris), tangent)
    }

    /// Whether a built blade's lowest point hangs: the crown is not
    /// within [`HANG_REACH_MM`] under it, so its first layer would be laid
    /// in air.
    pub(super) fn hangs(&self, surf: &[V]) -> bool {
        let low =
            surf.iter().copied().fold(
                [0.0, 0.0, f64::INFINITY],
                |m, v| if v[2] < m[2] { v } else { m },
            );
        let start = add(low, [0.0, 0.0, 0.05]);
        self.crown
            .hit(start, [0.0, 0.0, -1.0])
            .is_none_or(|h| start[2] - h.point[2] > 0.05 + HANG_REACH_MM)
    }

    /// [`Self::drape`] the blade rooted at `base` heading `t`; where the
    /// crown under it is too broken to lay it on (a crease between lobes,
    /// a cut), turn it about the root normal by [`DRAPE_TURNS`] until it
    /// lies. The leaf keeps its root; only a turned leaf's heading changes.
    pub(super) fn drape_turning(
        &self,
        base: V,
        n: V,
        t: V,
        blade: &Blade,
        embed: f64,
    ) -> Result<(Vec<V>, Vec<V>, V), Fail> {
        let mut first = Fail::Drape;
        for (i, turn) in DRAPE_TURNS.iter().enumerate() {
            let tt = leaf::rotate_about(t, safe_norm(n), turn.to_radians());
            let ls = blade.length_mm;
            let (anchor, an) = self
                .crown
                .project(add(base, scale(tt, 0.25 * ls)), n, 1.0 + 0.25 * ls + 1.0)
                .unwrap_or((base, n));
            let r = self.drape(anchor, an, tt, blade, embed);
            if r.is_ok() {
                return r;
            }
            if let (0, Err(f)) = (i, r) {
                first = f;
            }
        }
        Err(first)
    }

    /// A flattened underside blade, laid on the crown: the midrib is
    /// walked along the crown from the seat in twelfth-of-a-leaf steps
    /// (each step re-dropped onto the crown along the local normal, so it
    /// follows the clump round its bevel), then each ring's lateral
    /// vertices are dropped onto the crown across the walked frame and set
    /// their own dome and crease height off it, along the crown's normal
    /// there, less its embed (`drape_embed`). The blade is the crown's surface
    /// raised by at most its 0.24 mm dome and crease, so it cannot hang
    /// past the crown it lies on; its rim and both ends are tucked just
    /// inside. The root oval follows the walked midrib, 0.75 mm in.
    pub(super) fn drape(
        &self,
        anchor: V,
        n: V,
        t0: V,
        blade: &Blade,
        embed: f64,
    ) -> Result<(Vec<V>, Vec<V>, V), Fail> {
        let len = blade.length_mm;
        let t = safe_norm(sub(t0, scale(n, dot(t0, n))));
        let lat = cross(n, t);
        if norm(lat) < 1e-6 {
            return Err(Fail::Error);
        }
        let lat = scale(lat, 1.0 / norm(lat));
        let ds = len / leaf::N_LONG as f64;
        // the anchor is three quarters along the blade (station 9 of 12)
        let fwd = self.crown.walk(anchor, n, t, ds, 3)?;
        let back = self.crown.walk(anchor, n, scale(t, -1.0), ds, 9)?;
        let mut frames: Vec<Frame> = back
            .into_iter()
            .rev()
            .map(|f| Frame {
                t: scale(f.t, -1.0),
                ..f
            })
            .collect();
        frames.push(Frame { p: anchor, n, t });
        frames.extend(fwd);

        // the flat blade's lateral offsets and heights, ring by ring
        let origin = sub(anchor, scale(t, 0.75 * len));
        let flat = Blade {
            arch_deg: 0.0,
            curl_deg: 0.0,
            lift_mm: 0.0,
            ..*blade
        };
        let surf = leaf::surface(origin, t, n, &flat);
        // each ring walks out from its midrib, both ways, a lateral step
        // at a time (a lateral jump would fall off a lobe's edge)
        let mut out = vec![[0.0; 3]; surf.len()];
        let mut up = vec![0.0; surf.len()];
        let mut on = vec![[0.0; 3]; surf.len()];
        let mut nrm = vec![[0.0; 3]; surf.len()];
        let flat_below = -libm::sin(self.shape.overhang_deg.to_radians());
        let full_above = -libm::sin((self.shape.overhang_deg - DRAPE_DOME_FADE_DEG).to_radians());
        let mut lay = |hit: V, ns: V, k: usize| {
            // the dome and crease fade out where the crown is near the
            // printer's limit: their side slopes would tip it past
            let u = ((ns[2] - flat_below) / (full_above - flat_below)).clamp(0.0, 1.0);
            let h = dot(sub(surf[k], origin), n) * u * u * (3.0 - 2.0 * u);
            // the rim is tucked into the crown: no edge stands proud
            let rim = k == BASE || k == TIP || k.is_multiple_of(STRIDE) || k % STRIDE == N_LAT;
            let lift = if rim { DRAPE_RIM_MM } else { embed };
            up[k] = h + lift;
            on[k] = hit;
            nrm[k] = ns;
        };
        for (k, st) in [(BASE, 0), (TIP, leaf::N_LONG)] {
            let f = &frames[st];
            lay(f.p, f.n, k);
        }
        let mid = N_LAT / 2;
        for r in 0..RINGS {
            let f = frames[r + 1];
            let k0 = r * STRIDE + mid;
            lay(f.p, f.n, k0);
            let right: Vec<usize> = (mid + 1..=N_LAT).collect();
            let left: Vec<usize> = (0..mid).rev().collect();
            for side_js in [right, left] {
                let (mut p, mut nn) = (f.p, f.n);
                let mut side = safe_norm(cross(f.n, f.t));
                let mut x_prev = dot(sub(surf[k0], origin), lat);
                for j in side_js {
                    let k = r * STRIDE + j;
                    let x = dot(sub(surf[k], origin), lat);
                    let guess = add(p, scale(side, x - x_prev));
                    let (hit, ns) = self.crown.drop_on(guess, nn).ok_or(Fail::Drape)?;
                    let step = sub(hit.point, p);
                    let sgn = if x - x_prev >= 0.0 { 1.0 } else { -1.0 };
                    let along = sub(step, scale(ns, dot(step, ns)));
                    if norm(along) > 1e-9 {
                        side = scale(safe_norm(along), sgn);
                    }
                    p = hit.point;
                    nn = ns;
                    x_prev = x;
                    lay(p, nn, k);
                }
            }
        }

        // the blade's low end comes out of the crown gradually: within
        // DRAPE_RAMP_MM of its lowest point it rises from the rim's tuck,
        // so it does not start as a lip under the crown it lies on
        let zlow = on.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        for k in 0..out.len() {
            let w = ((on[k][2] - zlow) / DRAPE_RAMP_MM).clamp(0.0, 1.0);
            let lift = DRAPE_RIM_MM + (up[k] - DRAPE_RIM_MM) * w;
            out[k] = add(on[k], scale(nrm[k], lift.min(up[k])));
        }
        // whether a sheet laid ring by ring on the crown's points folds back
        // over itself: a face of it turned away from the crown under it
        let folds = |sheet: &[V]| {
            self.tris.iter().any(|t| {
                let [a, b, c] = t.map(|i| i as usize);
                let flat = cross(sub(surf[b], surf[a]), sub(surf[c], surf[a]));
                let sgn = if dot(flat, n) >= 0.0 { 1.0 } else { -1.0 };
                let crown = add(add(nrm[a], nrm[b]), nrm[c]);
                let f = cross(sub(sheet[b], sheet[a]), sub(sheet[c], sheet[a]));
                sgn * dot(f, crown) < 0.0
            })
        };
        // across a crease between clumps the crown's points fold: the blade
        // over them would bridge the crease in air
        if folds(&on) {
            return Err(Fail::Drape);
        }
        // the slab's floor: the crown's surface under every blade vertex,
        // as deep in as it goes without folding (under a tight lobe a deep
        // floor crosses itself, and the union cuts a film loose there)
        let inner = DRAPE_ROOT_MM
            .iter()
            .map(|d| -> Vec<V> {
                on.iter()
                    .zip(&nrm)
                    .map(|(p, m)| sub(*p, scale(*m, *d)))
                    .collect()
            })
            .find(|f| !folds(f))
            .ok_or(Fail::Drape)?;
        // the blade's lowest point must have the crown going on below it
        // (the crown carries the layers there); over a clump's bottom the
        // blade would hang under it, its lowest layer laid in air
        let low = (0..out.len())
            .min_by(|&a, &b| out[a][2].total_cmp(&out[b][2]))
            .unwrap_or(BASE);
        if !self.crown.goes_below(on[low], nrm[low], out[low][2]) {
            return Err(Fail::Drape);
        }
        // the floor must lie inside the crown, so the slab's walls cross it
        let mid = inner[(RINGS / 2) * STRIDE + N_LAT / 2];
        if !self
            .crown
            .rays
            .contains(&[inner[BASE], mid, inner[TIP]])
            .iter()
            .all(|x| *x)
        {
            return Err(Fail::Oval);
        }
        Ok((out, inner, t))
    }

    /// One attempt at a leaf, as the organic placer makes it (flat-seat
    /// fallback, base and tip tucks; no burial probe under `tuck_tip`). Returns the blade surface, the
    /// root oval (world mm) and the seated tangent.
    #[allow(clippy::too_many_arguments, reason = "the placer's per-leaf knobs")]
    #[allow(
        clippy::too_many_lines,
        reason = "one pipeline, kept in pipeline order"
    )]
    pub(super) fn attempt(
        &self,
        base: V,
        normal: V,
        t0: V,
        blade: &Blade,
        standoff: f64,
        skip_skew: bool,
        tuck_tip_max: f64,
        arch_mm: f64,
    ) -> Result<(Vec<V>, Vec<V>, V), Fail> {
        let len = blade.length_mm;
        let c = &self.crown;
        let tilt = self.seat_tilt(base, normal, t0, len).unwrap_or(0.0);

        // Rigid frame: the oval centred under the anchor, the blade's base
        // 0.75·L up-slope of it, PROTRUSION above.
        let (ca, sa) = (libm::cos(tilt), libm::sin(tilt));
        let tangent = safe_norm(sub(scale(t0, ca), scale(normal, sa)));
        let up = safe_norm(add(scale(normal, ca), scale(t0, sa)));
        let lat = cross(normal, tangent);
        if norm(lat) < 1e-6 {
            return Err(Fail::Error);
        }
        let lat = scale(lat, 1.0 / norm(lat));
        let origin = sub(base, scale(tangent, 0.75 * len));
        let inner: Vec<V> = leaf::oval(normal, tangent, lat, len, blade.width_mm, ROOT_EMBED_MM)
            .into_iter()
            .map(|p| add(p, origin))
            .collect();
        let surf_base = add(origin, scale(normal, PROTRUSION_MM));

        if !c
            .rays
            .contains(&[inner[BASE], inner[TIP]])
            .iter()
            .all(|x| *x)
        {
            return Err(Fail::Oval);
        }

        let mut surf = leaf::surface(surf_base, tangent, up, blade);
        let moved = |surf: &mut Vec<V>, d: V| {
            for p in surf.iter_mut() {
                *p = add(*p, d);
            }
        };

        // Underside arch: 4·s·(1−s)·arch along the seat normal.
        if arch_mm > 0.0 {
            for r in 0..RINGS {
                let s = leaf::station(r);
                let bulge = scale(normal, 4.0 * s * (1.0 - s) * arch_mm);
                for j in 0..STRIDE {
                    surf[r * STRIDE + j] = add(surf[r * STRIDE + j], bulge);
                }
            }
        }

        // Printability skew: slide toward the base until the blade tip
        // clears the oval tip in z.
        let mut skew = 0.0;
        let z_need = if skip_skew {
            0.0
        } else {
            inner[TIP][2] + SKEW_TIP_MARGIN_MM - surf[TIP][2]
        };
        if z_need > 0.0 {
            let tz = tangent[2];
            skew = if tz < -1e-6 {
                z_need / -tz
            } else {
                f64::INFINITY
            };
            if skew > MAX_SKEW_FRAC * len {
                return Err(Fail::Skew);
            }
            moved(&mut surf, scale(tangent, -skew));
        }

        // Belly-dip seat: the tip-half midrib vertex (or the tip) lowest
        // along the leaf normal sits PROTRUSION off the clump.
        let mid = N_LAT / 2;
        let cands = (0..RINGS)
            .filter(|&r| leaf::station(r) > 0.5)
            .map(|r| r * STRIDE + mid)
            .chain([TIP]);
        let mut dip = TIP;
        let mut best = f64::INFINITY;
        for k in cands {
            let d = dot(sub(surf[k], surf[BASE]), up);
            if d < best {
                best = d;
                dip = k;
            }
        }
        let mut drop = 0.0;
        if let Some((cl, _)) = c.clearance(surf[dip], normal) {
            drop = cl - PROTRUSION_MM;
            let nz = normal[2];
            if drop > 0.0 && nz > 1e-6 {
                let slack = surf[TIP][2] - inner[TIP][2] - SKEW_TIP_MARGIN_MM;
                drop = drop.min(slack.max(0.0) / nz);
            }
            moved(&mut surf, scale(normal, -drop));
        }

        // Shingle standoff, after the seat.
        if standoff > 0.0 {
            moved(&mut surf, scale(normal, standoff));
        }

        // Neck gate.
        let net = (standoff - drop).max(0.0);
        if libm::hypot(skew, net) > MAX_NECK_MM {
            return Err(Fail::Neck);
        }

        // Tuck the base slightly in (about the belly dip, ≤ 30°), then cap
        // the tip's clearance (about the base, ≤ 35°).
        let pivot = surf[dip];
        self.tuck(&mut surf, BASE, pivot, normal, tangent, -0.2, 30.0);
        let pivot = surf[BASE];
        self.tuck(&mut surf, TIP, pivot, normal, tangent, tuck_tip_max, 35.0);

        Ok((surf, inner, tangent))
    }

    /// Pitch the blade about `pivot` (axis normal × tangent) so vertex
    /// `idx` comes down toward `target` clearance along −normal.
    #[allow(clippy::too_many_arguments, reason = "the two tucks share it")]
    pub(super) fn tuck(
        &self,
        surf: &mut [V],
        idx: usize,
        pivot: V,
        normal: V,
        tangent: V,
        target: f64,
        max_deg: f64,
    ) {
        let v = surf[idx];
        let Some((cl, _)) = self.crown.clearance(v, normal) else {
            return;
        };
        let r = norm(sub(v, pivot));
        if !(cl > target && r > 1e-6) {
            return;
        }
        let theta = libm::asin(((cl - target) / r).min(1.0)).min(max_deg.to_radians());
        let lat = safe_norm(cross(normal, tangent));
        let v0 = sub(v, pivot);
        let mut sgn = 1.0;
        for s in [1.0, -1.0] {
            sgn = s;
            if dot(
                sub(rotate_about(v0, lat, s * theta), v0),
                scale(normal, -1.0),
            ) > 0.0
            {
                break;
            }
        }
        for p in surf.iter_mut() {
            *p = add(pivot, rotate_about(sub(*p, pivot), lat, sgn * theta));
        }
    }

    /// Pitch the oval about its centre until both
    /// ends sit equally deep (≤ 3 Newton steps, ≤ 60°); `None` to fall back
    /// to a flat seat.
    pub(super) fn seat_tilt(&self, p0: V, n0: V, t0: V, len: f64) -> Option<f64> {
        let c0 = sub(p0, scale(n0, ROOT_EMBED_MM));
        let h = 0.25 * len;
        let span = 0.5 * len;
        let mut theta = 0.0_f64;
        for _ in 0..3 {
            let t = sub(scale(t0, libm::cos(theta)), scale(n0, libm::sin(theta)));
            let ends = [sub(c0, scale(t, h)), add(c0, scale(t, h))];
            let inside = self.crown.rays.contains(&ends);
            let dirs = [0, 1].map(|k| if inside[k] { n0 } else { scale(n0, -1.0) });
            let hits = self.crown.rays.first_hits(&ends, &dirs);
            let mut d = [0.0; 2];
            for k in 0..2 {
                let hp = hits[k]?.point;
                let dist = norm(sub(hp, ends[k]));
                d[k] = if inside[k] { dist } else { -dist };
            }
            let imbalance = d[0] - d[1];
            if imbalance.abs() <= 0.05 {
                break;
            }
            theta += libm::asin((imbalance / span).clamp(-1.0, 1.0));
            if theta.abs() > 60.0_f64.to_radians() {
                return None;
            }
        }
        Some(theta)
    }
}
