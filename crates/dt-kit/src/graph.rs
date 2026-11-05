//! Graphs: the P5 operations for branching things (trees; later roots and
//! vines). [`colonise`] traces a branching graph from a root to target
//! points by space colonisation, with the tree invariants held by
//! construction; [`simplify`] keeps its forks and ends; [`pipe_radii`]
//! sizes it bottom-up; [`tubes`] lofts a closed, C1-continuous tube along
//! every edge.
//!
//! The skeleton grows from the root to the leaf targets, is simplified,
//! and gets radii bottom-up; the tube half builds each closed edge solid
//! (with the Bézier and Bishop-frame helpers).
//!
//! Invariants of a traced graph (docs/looks/trees.html, S1–S4), each by
//! construction, not by a check after the fact:
//!
//! - S1: every target is an end (a node with no children), and only
//!   targets are ends;
//! - S2: every branch lands exactly on its target;
//! - S3: forks happen only at synthetic in-between nodes;
//! - S4: no edge rises at less than the floor angle above horizontal. A
//!   step that would is turned up onto the floor cone; a target that
//!   cannot be reached at the floor from the branch that owns it is moved
//!   onto the cone (the nearest point), and counted. [`tubes`] then keeps
//!   every point of every edge's curve at or above the floor, shortening
//!   the C1 handles of an edge where they would dip.

use std::collections::VecDeque;

use dt_geom::TriMesh;

/// How [`colonise`] grows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colonise {
    /// Step length, mm.
    pub step_mm: f64,
    /// A target stays with its branch while it is inside this cone
    /// (cosine of the half-angle; lower splits later).
    pub split_cos: f64,
    /// A target needing a heading below this elevation (its sine) splits
    /// off into its own branch: the angle branches aim to keep.
    pub aim_sin: f64,
    /// The floor: no edge rises less than this many degrees (S4).
    pub floor_deg: f64,
    /// In-between steps (all but the landing on a target) rise at least
    /// this many degrees (at or above the floor): the printable climb.
    pub climb_deg: f64,
    /// At most this many branches leave one node at a step.
    pub max_branches: usize,
    /// Heading inertia: 0 steers straight at the target, 1 keeps going.
    pub alpha: f64,
    /// Steps a branch may take before it is forced to its ends.
    pub max_steps: usize,
    /// Aim point within the owned targets: 0 the lowest, 1 the highest.
    pub aim: f64,
    /// How evenly a fork shares its targets, 0 to 1.
    pub balance: f64,
}

/// A traced graph, before [`simplify`]: every step is a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Traced {
    /// Node positions; node 0 is the root.
    pub nodes: Vec<[f64; 3]>,
    /// Each node's parent (`usize::MAX` for the root).
    pub parents: Vec<usize>,
    /// The direction of the step that arrived at each node.
    pub dirs: Vec<[f64; 3]>,
    /// The node each target ended at (S1, S2: one end per target).
    pub ends: Vec<usize>,
    /// The targets as reached (moved ones on their floor cone).
    pub targets: Vec<[f64; 3]>,
    /// Targets moved onto the floor cone (S4).
    pub moved: usize,
}

/// **colonise**: trace a branching graph from `root` (heading up) to every
/// target. `groups`, if given, labels targets that split off together.
#[must_use]
pub fn colonise(
    root: [f64; 3],
    targets: &[[f64; 3]],
    groups: Option<&[usize]>,
    c: &Colonise,
) -> Traced {
    let mut t = Traced {
        nodes: vec![root],
        parents: vec![usize::MAX],
        dirs: vec![[0.0, 0.0, 1.0]],
        ends: vec![usize::MAX; targets.len()],
        targets: targets.to_vec(),
        moved: 0,
    };
    if targets.is_empty() {
        return t;
    }
    let floor = (c.floor_deg + FLOOR_MARGIN_DEG).to_radians();
    let all: Vec<usize> = (0..targets.len()).collect();
    let mut queue: VecDeque<Branch> = VecDeque::new();
    queue.push_back(Branch {
        tip: 0,
        owned: all,
        labels: groups.map(<[usize]>::to_vec),
        steps: 0,
    });
    while let Some(b) = queue.pop_front() {
        if b.owned.is_empty() {
            continue;
        }
        step(&mut t, &mut queue, b, c, floor);
    }
    t
}

/// A margin above the floor for every constructed direction, so rounding
/// never puts an edge under it.
const FLOOR_MARGIN_DEG: f64 = 0.25;

struct Branch {
    tip: usize,
    owned: Vec<usize>,
    labels: Option<Vec<usize>>,
    steps: usize,
}

#[allow(clippy::too_many_lines)]
fn step(t: &mut Traced, queue: &mut VecDeque<Branch>, b: Branch, c: &Colonise, floor: f64) {
    let pos = t.nodes[b.tip];
    let heading = t.dirs[b.tip];
    let at = |i: usize| t.targets[i];
    // aim: between the lowest and the highest owned target
    let lowest = argbest(&b.owned, |i| -at(i)[2]);
    let highest = argbest(&b.owned, |i| at(i)[2]);
    let (lo, hi) = (at(lowest), at(highest));
    let aim = [0, 1, 2].map(|k| lo[k] + c.aim * (hi[k] - lo[k]));
    let raw = sub(aim, pos);
    let main = match unit(raw) {
        Some(d) => {
            let blend = [0, 1, 2].map(|k| d[k] * (1.0 - c.alpha) + heading[k] * c.alpha);
            unit(blend).unwrap_or(d)
        }
        None => heading,
    };
    let climb = (c.climb_deg.max(c.floor_deg) + FLOOR_MARGIN_DEG).to_radians();
    let main = lift(main, heading, climb);

    let mut primary = b.owned.clone();
    let mut primary_labels = b.labels.clone();
    if b.owned.len() >= 2 && c.max_branches > 1 {
        let next = add(pos, scale(main, c.step_mm));
        let units: Vec<[f64; 3]> = b
            .owned
            .iter()
            .map(|&i| unit(sub(at(i), next)).unwrap_or([0.0, 0.0, 1.0]))
            .collect();
        let cos_a: Vec<f64> = units
            .iter()
            .map(|u| dot(*u, main).clamp(-1.0, 1.0))
            .collect();
        let mut stray: Vec<bool> = (0..b.owned.len())
            .map(|k| cos_a[k] < c.split_cos || units[k][2] < c.aim_sin)
            .collect();
        if let Some(labels) = &b.labels {
            // a group strays together
            let groups: Vec<usize> = (0..labels.len())
                .filter(|&k| stray[k])
                .map(|k| labels[k])
                .collect();
            for k in 0..labels.len() {
                stray[k] = groups.contains(&labels[k]);
            }
        }
        let pick = |want: bool| -> Vec<usize> {
            (0..b.owned.len()).filter(|&k| stray[k] == want).collect()
        };
        let (sk, pk) = (pick(true), pick(false));
        let stray_t: Vec<usize> = sk.iter().map(|&k| b.owned[k]).collect();
        let stray_l: Option<Vec<usize>> = b
            .labels
            .as_ref()
            .map(|l| sk.iter().map(|&k| l[k]).collect());
        primary = pk.iter().map(|&k| b.owned[k]).collect();
        primary_labels = b
            .labels
            .as_ref()
            .map(|l| pk.iter().map(|&k| l[k]).collect());

        if !stray_t.is_empty() && !primary.is_empty() {
            let mut clusters: Vec<Vec<usize>> = match &stray_l {
                Some(l) => by_group(&stray_t, l),
                None => cluster_pca(&t.targets, &stray_t, pos, c.max_branches - 1),
            };
            if c.balance > 1e-6 && !clusters.is_empty() {
                #[allow(clippy::cast_precision_loss)]
                let k_forks = (1 + clusters.len()) as f64;
                match &b.labels {
                    None => {
                        #[allow(clippy::cast_precision_loss)]
                        let (np, no) = (primary.len() as f64, b.owned.len() as f64);
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let n_target = (round_half_even(np + (no / k_forks - np) * c.balance)
                            .max(1.0) as usize)
                            .min(b.owned.len() - 1)
                            .max(1);
                        let mut order: Vec<usize> = (0..b.owned.len()).collect();
                        order.sort_by(|&x, &y| cos_a[y].total_cmp(&cos_a[x]).then(x.cmp(&y)));
                        primary = order[..n_target].iter().map(|&k| b.owned[k]).collect();
                        let rest: Vec<usize> =
                            order[n_target..].iter().map(|&k| b.owned[k]).collect();
                        clusters = cluster_pca(&t.targets, &rest, pos, c.max_branches - 1);
                        primary_labels = None;
                    }
                    Some(labels) => {
                        let mut gids = labels.clone();
                        gids.sort_unstable();
                        gids.dedup();
                        let n_g = gids.len();
                        let n_pg = primary_labels.as_ref().map_or(0, |pl| {
                            let mut u = pl.clone();
                            u.sort_unstable();
                            u.dedup();
                            u.len()
                        });
                        #[allow(clippy::cast_precision_loss)]
                        let (fpg, fg) = (n_pg as f64, n_g as f64);
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let n_gt =
                            (round_half_even(fpg + (fg / k_forks - fpg) * c.balance).max(1.0)
                                as usize)
                                .min(n_g.saturating_sub(1))
                                .max(1);
                        let mean_cos = |g: usize| {
                            let (s, n) = (0..labels.len())
                                .filter(|&k| labels[k] == g)
                                .fold((0.0, 0.0), |(s, n), k| (s + cos_a[k], n + 1.0));
                            s / f64::max(n, 1.0)
                        };
                        let mut sorted = gids.clone();
                        sorted.sort_by(|&x, &y| mean_cos(y).total_cmp(&mean_cos(x)));
                        let keep = &sorted[..n_gt.min(sorted.len())];
                        let members = |g: usize| -> Vec<usize> {
                            (0..labels.len()).filter(|&k| labels[k] == g).collect()
                        };
                        let pk: Vec<usize> = (0..labels.len())
                            .filter(|&k| keep.contains(&labels[k]))
                            .collect();
                        primary = pk.iter().map(|&k| b.owned[k]).collect();
                        primary_labels = Some(pk.iter().map(|&k| labels[k]).collect());
                        clusters = sorted[n_gt.min(sorted.len())..]
                            .iter()
                            .map(|&g| members(g).iter().map(|&k| b.owned[k]).collect())
                            .filter(|v: &Vec<usize>| !v.is_empty())
                            .collect();
                    }
                }
            }
            for cl in clusters {
                queue.push_back(Branch {
                    tip: b.tip,
                    owned: cl,
                    labels: None,
                    steps: 0,
                });
            }
        }
        if primary.is_empty() {
            match &stray_l {
                Some(l) => {
                    let best = largest_group(l);
                    for (g, members) in groups_of(&stray_t, l) {
                        if g == best {
                            primary = members;
                        } else if !members.is_empty() {
                            queue.push_back(Branch {
                                tip: b.tip,
                                owned: members,
                                labels: None,
                                steps: 0,
                            });
                        }
                    }
                    primary_labels = None;
                }
                None => {
                    let mut cl = cluster_pca(&t.targets, &b.owned, pos, 2).into_iter();
                    let Some(first) = cl.next() else {
                        return;
                    };
                    primary = first;
                    primary_labels = None;
                    for extra in cl.filter(|e| !e.is_empty()) {
                        queue.push_back(Branch {
                            tip: b.tip,
                            owned: extra,
                            labels: None,
                            steps: 0,
                        });
                    }
                }
            }
        }
        if let Some(pl) = &primary_labels
            && pl.iter().all(|&g| g == pl[0])
        {
            primary_labels = None;
        }
    }

    if primary.len() == 1 || b.steps >= c.max_steps {
        if primary.len() > 1 {
            if let Some(pl) = &primary_labels {
                let best = largest_group(pl);
                let (keep, rest): (Vec<usize>, Vec<usize>) =
                    (0..primary.len()).partition(|&k| pl[k] == best);
                if !rest.is_empty() {
                    queue.push_back(Branch {
                        tip: b.tip,
                        owned: rest.iter().map(|&k| primary[k]).collect(),
                        labels: None,
                        steps: 0,
                    });
                }
                primary = keep.iter().map(|&k| primary[k]).collect();
            }
            let near = argbest(&primary, |i| -dist(at(i), pos));
            let rest: Vec<usize> = primary.iter().copied().filter(|&i| i != near).collect();
            if !rest.is_empty() {
                queue.push_back(Branch {
                    tip: b.tip,
                    owned: rest,
                    labels: None,
                    steps: 0,
                });
            }
            primary = vec![near];
        }
        land(t, b.tip, primary[0], c, floor);
    } else {
        let new_pos = add(pos, scale(main, c.step_mm));
        let k = push(t, new_pos, b.tip, main);
        queue.push_back(Branch {
            tip: k,
            owned: primary,
            labels: primary_labels,
            steps: b.steps + 1,
        });
    }
}

/// `_grow_to_leaf`: the end edge from `tip` to target `i`, landing exactly
/// on it (S2). The intermediate nodes would lie on the straight line (the
/// heading starts at the target) and simplify away, so the edge is one
/// step. A target below the floor cone is moved onto it first (S4).
fn land(t: &mut Traced, tip: usize, i: usize, c: &Colonise, floor: f64) {
    let pos = t.nodes[tip];
    let mut v = sub(t.targets[i], pos);
    let len = norm(v);
    let need = libm::sin(floor);
    if len < 1e-6 || v[2] < need * len {
        // nearest point on the floor cone's surface, at least half a step out
        let h = unit([v[0], v[1], 0.0])
            .or_else(|| unit([t.dirs[tip][0], t.dirs[tip][1], 0.0]))
            .unwrap_or([1.0, 0.0, 0.0]);
        let d = [
            h[0] * libm::cos(floor),
            h[1] * libm::cos(floor),
            libm::sin(floor),
        ];
        let along = dot(v, d).max(0.5 * c.step_mm);
        v = scale(d, along);
        t.targets[i] = add(pos, v);
        t.moved += 1;
    }
    let dir = unit(v).unwrap_or([0.0, 0.0, 1.0]);
    let k = push(t, t.targets[i], tip, dir);
    t.ends[i] = k;
}

/// Turn `d` up onto the floor cone if it is below it, keeping its compass
/// heading (or `heading`'s, if `d` is vertical).
fn lift(d: [f64; 3], heading: [f64; 3], floor: f64) -> [f64; 3] {
    if d[2] >= libm::sin(floor) {
        return d;
    }
    let h = unit([d[0], d[1], 0.0])
        .or_else(|| unit([heading[0], heading[1], 0.0]))
        .unwrap_or([1.0, 0.0, 0.0]);
    [
        h[0] * libm::cos(floor),
        h[1] * libm::cos(floor),
        libm::sin(floor),
    ]
}

fn push(t: &mut Traced, p: [f64; 3], parent: usize, dir: [f64; 3]) -> usize {
    t.nodes.push(p);
    t.parents.push(parent);
    t.dirs.push(dir);
    t.nodes.len() - 1
}

/// The member of `items` with the largest `key` (first on ties).
fn argbest(items: &[usize], key: impl Fn(usize) -> f64) -> usize {
    let mut best = items[0];
    let mut bv = key(best);
    for &i in &items[1..] {
        let v = key(i);
        if v > bv {
            best = i;
            bv = v;
        }
    }
    best
}

/// Groups in ascending label order, members in order.
fn groups_of(items: &[usize], labels: &[usize]) -> Vec<(usize, Vec<usize>)> {
    let mut gids = labels.to_vec();
    gids.sort_unstable();
    gids.dedup();
    gids.into_iter()
        .map(|g| {
            (
                g,
                (0..items.len())
                    .filter(|&k| labels[k] == g)
                    .map(|k| items[k])
                    .collect(),
            )
        })
        .collect()
}

fn by_group(items: &[usize], labels: &[usize]) -> Vec<Vec<usize>> {
    groups_of(items, labels)
        .into_iter()
        .map(|(_, m)| m)
        .filter(|m| !m.is_empty())
        .collect()
}

/// The label with the most members (the lowest such label on ties).
fn largest_group(labels: &[usize]) -> usize {
    let mut best = (0, usize::MAX);
    for (g, m) in groups_of(labels, labels) {
        if m.len() > best.0 {
            best = (m.len(), g);
        }
    }
    best.1
}

/// `_cluster_pca`: split targets in two by the main axis of their
/// directions from `origin`; at most `max_k` clusters.
fn cluster_pca(
    all: &[[f64; 3]],
    items: &[usize],
    origin: [f64; 3],
    max_k: usize,
) -> Vec<Vec<usize>> {
    if max_k == 0 || items.is_empty() {
        return Vec::new();
    }
    if max_k == 1 || items.len() < 2 {
        return vec![items.to_vec()];
    }
    let units: Vec<[f64; 3]> = items
        .iter()
        .map(|&i| {
            let d = sub(all[i], origin);
            let n = norm(d) + 1e-9;
            scale(d, 1.0 / n)
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let n = units.len() as f64;
    let mean = [0, 1, 2].map(|k| units.iter().map(|u| u[k]).sum::<f64>() / n);
    let centred: Vec<[f64; 3]> = units.iter().map(|u| sub(*u, mean)).collect();
    let mut cov = [[0.0; 3]; 3];
    for c in &centred {
        for a in 0..3 {
            for b in 0..3 {
                cov[a][b] += c[a] * c[b];
            }
        }
    }
    let axis = principal_axis(cov);
    let (mut lo, mut hi) = (Vec::new(), Vec::new());
    for (k, c) in centred.iter().enumerate() {
        if dot(*c, axis) <= 0.0 {
            lo.push(items[k]);
        } else {
            hi.push(items[k]);
        }
    }
    [lo, hi]
        .into_iter()
        .filter(|v| !v.is_empty())
        .take(max_k)
        .collect()
}

/// The unit eigenvector of a symmetric 3 × 3 matrix's largest eigenvalue
/// (cyclic Jacobi; deterministic), signed so its largest component is
/// positive.
fn principal_axis(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..32 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-15 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let tt = theta.signum() / (theta.abs() + libm::sqrt(theta * theta + 1.0));
            let cs = 1.0 / libm::sqrt(tt * tt + 1.0);
            let sn = tt * cs;
            for row in &mut a {
                let (akp, akq) = (row[p], row[q]);
                row[p] = cs * akp - sn * akq;
                row[q] = sn * akp + cs * akq;
            }
            let (rp, rq) = (a[p], a[q]);
            for k in 0..3 {
                a[p][k] = cs * rp[k] - sn * rq[k];
                a[q][k] = sn * rp[k] + cs * rq[k];
            }
            for row in &mut v {
                let (vp, vq) = (row[p], row[q]);
                row[p] = cs * vp - sn * vq;
                row[q] = sn * vp + cs * vq;
            }
        }
    }
    let k = (0..3).fold(0, |b, k| if a[k][k] > a[b][b] { k } else { b });
    let mut e = [v[0][k], v[1][k], v[2][k]];
    let big = (0..3).fold(0, |b, j| if e[j].abs() > e[b].abs() { j } else { b });
    if e[big] < 0.0 {
        e = e.map(|x| -x);
    }
    e
}

/// Rounds halves to even.
#[must_use]
pub fn round_half_even(x: f64) -> f64 {
    let r = libm::round(x);
    if ((x - libm::trunc(x)).abs() - 0.5).abs() < 1e-12 {
        2.0 * libm::round(x / 2.0)
    } else {
        r
    }
}

/// A traced graph with only its root, forks and ends (`_simplify_skeleton`),
/// in depth-first order (every parent before its children).
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    /// Node positions; node 0 is the root.
    pub nodes: Vec<[f64; 3]>,
    /// Each node's parent (`None` for the root).
    pub parents: Vec<Option<usize>>,
    /// The direction arriving at each node (the end tangent of its edge).
    pub in_dirs: Vec<[f64; 3]>,
    /// The direction leaving the parent toward each node.
    pub out_dirs: Vec<[f64; 3]>,
    /// Radii, mm (see [`pipe_radii`]).
    pub radii: Vec<f64>,
    /// For each end node, the target it is (`None` elsewhere).
    pub target: Vec<Option<usize>>,
}

impl Graph {
    /// Each node's children, in order.
    #[must_use]
    pub fn children(&self) -> Vec<Vec<usize>> {
        let mut ch = vec![Vec::new(); self.nodes.len()];
        for (i, p) in self.parents.iter().enumerate() {
            if let Some(p) = p {
                ch[*p].push(i);
            }
        }
        ch
    }

    /// The elevation (degrees above horizontal) of every edge's chord.
    #[must_use]
    pub fn elevations(&self) -> Vec<f64> {
        (0..self.nodes.len())
            .filter_map(|i| {
                let p = self.parents[i]?;
                let e = sub(self.nodes[i], self.nodes[p]);
                let l = norm(e);
                (l > 1e-9).then(|| libm::asin((e[2] / l).clamp(-1.0, 1.0)).to_degrees())
            })
            .collect()
    }
}

/// **simplify**: drop the single-child steps of a traced graph; radii are
/// left at zero for [`pipe_radii`].
#[must_use]
pub fn simplify(t: &Traced) -> Graph {
    let n = t.nodes.len();
    let mut ch = vec![Vec::new(); n];
    for i in 1..n {
        ch[t.parents[i]].push(i);
    }
    let mut end_of = vec![None; n];
    for (target, &node) in t.ends.iter().enumerate() {
        if node < n {
            end_of[node] = Some(target);
        }
    }
    let significant = |i: usize| i == 0 || ch[i].len() != 1;
    let mut g = Graph {
        nodes: Vec::new(),
        parents: Vec::new(),
        in_dirs: Vec::new(),
        out_dirs: Vec::new(),
        radii: Vec::new(),
        target: Vec::new(),
    };
    // depth first, children in order (an explicit stack, reversed pushes)
    let mut stack = vec![(0usize, None::<usize>, t.dirs[0])];
    while let Some((old, parent, out)) = stack.pop() {
        let new = g.nodes.len();
        g.nodes.push(t.nodes[old]);
        g.parents.push(parent);
        g.in_dirs.push(t.dirs[old]);
        g.out_dirs.push(out);
        g.radii.push(0.0);
        g.target.push(end_of[old]);
        for &c in ch[old].iter().rev() {
            let first = t.dirs[c];
            let mut cur = c;
            while !significant(cur) {
                cur = ch[cur][0];
            }
            stack.push((cur, Some(new), first));
        }
    }
    g
}

/// **pipe radii**: ends get `tip_mm`; every other node
/// `(Σ r_child^e)^(1/e)` (the pipe model; the root is derived).
pub fn pipe_radii(g: &mut Graph, exponent: f64, tip_mm: f64) {
    let ch = g.children();
    for i in (0..g.nodes.len()).rev() {
        g.radii[i] = if ch[i].is_empty() {
            tip_mm
        } else {
            let s: f64 = ch[i].iter().map(|&c| libm::pow(g.radii[c], exponent)).sum();
            libm::pow(s, 1.0 / exponent)
        };
    }
}

/// One edge's cubic Bézier: from its parent node to its node, with C1
/// handles along the arriving directions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    /// Control points.
    pub p: [[f64; 3]; 4],
}

impl Curve {
    /// The point at `t` in 0..1.
    #[must_use]
    pub fn at(&self, t: f64) -> [f64; 3] {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        [0, 1, 2].map(|k| (0..4).map(|j| w[j] * self.p[j][k]).sum())
    }

    /// The (unnormalised) tangent at `t`.
    #[must_use]
    pub fn tangent(&self, t: f64) -> [f64; 3] {
        let u = 1.0 - t;
        let p = &self.p;
        [0, 1, 2].map(|k| {
            3.0 * u * u * (p[1][k] - p[0][k])
                + 6.0 * u * t * (p[2][k] - p[1][k])
                + 3.0 * t * t * (p[3][k] - p[2][k])
        })
    }

    /// The lowest elevation of its tangent, degrees, sampled finely.
    #[must_use]
    pub fn min_elevation(&self) -> f64 {
        (0..=64)
            .filter_map(|k| unit(self.tangent(f64::from(k) / 64.0)))
            .map(|d| libm::asin(d[2].clamp(-1.0, 1.0)).to_degrees())
            .fold(90.0, f64::min)
    }
}

/// The C1 curve of every edge (`None` for the root and zero-length
/// edges), handles `handle` × the chord, each edge's handles shortened
/// (halved until it holds) where its curve would dip below `floor_deg`;
/// and how many were shortened.
#[must_use]
pub fn curves(g: &Graph, handle: f64, floor_deg: f64) -> (Vec<Option<Curve>>, usize) {
    let mut shortened = 0;
    let out = (0..g.nodes.len())
        .map(|i| {
            let p = g.parents[i]?;
            let (p0, p3) = (g.nodes[p], g.nodes[i]);
            let len = dist(p0, p3);
            if len < 1e-8 {
                return None;
            }
            let (t0, t3) = (
                unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0]),
                unit(g.in_dirs[i]).unwrap_or([0.0, 0.0, 1.0]),
            );
            let make = |h: f64| Curve {
                p: [p0, add(p0, scale(t0, h)), sub(p3, scale(t3, h)), p3],
            };
            let mut h = handle * len;
            let mut c = make(h);
            let mut tries = 0;
            while c.min_elevation() < floor_deg && tries < 12 {
                h *= 0.5;
                c = make(h);
                tries += 1;
            }
            if tries > 0 {
                shortened += 1;
                if c.min_elevation() < floor_deg {
                    c = make(0.0);
                }
            }
            Some(c)
        })
        .collect();
    (out, shortened)
}

/// Sets each side's radius of a ring: `(ring, angles, radii)`.
pub type Surface<'a> = dyn Fn(&Ring, &[f64], &mut [f64]) + 'a;

/// How [`tubes`] lofts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TubeStyle {
    /// Longest ring spacing, mm.
    pub step_mm: f64,
    /// No ring thinner than this, mm.
    pub min_radius_mm: f64,
    /// Latitude rings of an end's dome.
    pub dome_rings: usize,
}

/// One ring of a tube, as the surface callback of [`tubes`] sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring {
    /// The edge (its child node).
    pub edge: usize,
    /// Arc length from the edge's parent node, mm (negative on the
    /// overlap into the parent).
    pub s: f64,
    /// The edge's arc length, mm.
    pub length: f64,
    /// The ring's centre.
    pub centre: [f64; 3],
    /// The edge's radius here, mm (linear from parent to child).
    pub radius: f64,
}

/// **tubes**: a closed tube along every edge of `g` with a curve: rings
/// on its [`Curve`], frames carried from the parent edge (Bishop frames,
/// so rings do not twist at forks), starting a little inside the parent
/// (a real overlap at every fork) and ending in a dome at an end node.
/// `sides(edge)` gives the ring's side count; `surface(ring, angles,
/// radii)` may set the radius of each side (it starts at the ring's
/// radius). Each tube is outward wound; index `i` is edge `i` (empty for
/// the root).
#[must_use]
pub fn tubes(
    g: &Graph,
    curves: &[Option<Curve>],
    style: &TubeStyle,
    sides: &dyn Fn(usize) -> usize,
    surface: &Surface<'_>,
) -> Vec<TriMesh> {
    let n = g.nodes.len();
    let ch = g.children();
    let mut frames: Vec<Option<([f64; 3], [f64; 3])>> = vec![None; n];
    frames[0] = Some(basis(unit(g.in_dirs[0]).unwrap_or([0.0, 0.0, 1.0])));
    let mut out = vec![TriMesh::default(); n];
    // breadth first: a parent's end frame before its children
    let mut queue: VecDeque<usize> = ch[0].iter().copied().collect();
    while let Some(i) = queue.pop_front() {
        queue.extend(ch[i].iter().copied());
        let Some(p) = g.parents[i] else { continue };
        let pf = frames[p].unwrap_or_else(|| basis(unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0])));
        let Some(curve) = curves[i] else {
            frames[i] = Some(pf);
            continue;
        };
        let (mesh, end) = tube(
            g,
            i,
            p,
            &curve,
            pf,
            ch[i].is_empty(),
            style,
            sides(i),
            surface,
        );
        frames[i] = Some(end);
        out[i] = mesh;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn tube(
    g: &Graph,
    i: usize,
    p: usize,
    curve: &Curve,
    frame: ([f64; 3], [f64; 3]),
    end: bool,
    style: &TubeStyle,
    n_sides: usize,
    surface: &Surface<'_>,
) -> (TriMesh, ([f64; 3], [f64; 3])) {
    let r_start = g.radii[p].max(style.min_radius_mm);
    let r_end = g.radii[i].max(style.min_radius_mm);
    let p0 = curve.p[0];
    let t_in = unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0]);
    // sample the curve by arc length
    let fine: Vec<[f64; 3]> = (0..=64).map(|k| curve.at(f64::from(k) / 64.0)).collect();
    let mut arc = vec![0.0];
    for w in fine.windows(2) {
        arc.push(arc[arc.len() - 1] + dist(w[0], w[1]));
    }
    let length = arc[64];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (libm::ceil(length / style.step_mm) as usize).max(4);
    // the overlap into the parent: straight back along its tangent
    let overlap = if p == 0 {
        0.0
    } else {
        (0.25 * r_start).max(0.15).min(0.35 * length)
    };
    let mut rings: Vec<([f64; 3], [f64; 3], f64, f64)> = Vec::new(); // centre, tangent, s, radius
    if overlap > 0.0 {
        rings.push((sub(p0, scale(t_in, overlap)), t_in, -overlap, r_start));
    }
    #[allow(clippy::cast_precision_loss)]
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let c = curve.at(t);
        let tan = unit(curve.tangent(t)).unwrap_or(t_in);
        let s = interp_arc(&arc, t);
        rings.push((
            c,
            tan,
            s,
            r_start + (r_end - r_start) * (s / length.max(1e-9)).clamp(0.0, 1.0),
        ));
    }
    let angles: Vec<f64> = (0..n_sides)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let a = 2.0 * std::f64::consts::PI * k as f64 / n_sides as f64;
            a
        })
        .collect();
    let (mut u, mut v) = frame;
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut radii = vec![0.0; n_sides];
    for (k, (c, tan, s, r)) in rings.iter().enumerate() {
        if k > 0 {
            (u, v) = transport(u, *tan);
        }
        radii.iter_mut().for_each(|x| *x = *r);
        surface(
            &Ring {
                edge: i,
                s: *s,
                length,
                centre: *c,
                radius: *r,
            },
            &angles,
            &mut radii,
        );
        for (a, rr) in angles.iter().zip(&radii) {
            let (ca, sa) = (libm::cos(*a), libm::sin(*a));
            positions.push([0, 1, 2].map(|d| c[d] + rr * (ca * u[d] + sa * v[d])));
        }
    }
    let nr = rings.len();
    let idx = |r: usize, k: usize| u32::try_from(r * n_sides + k % n_sides).unwrap_or(u32::MAX);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for r in 0..nr - 1 {
        for k in 0..n_sides {
            tris.push([idx(r, k), idx(r, k + 1), idx(r + 1, k + 1)]);
            tris.push([idx(r, k), idx(r + 1, k + 1), idx(r + 1, k)]);
        }
    }
    // start cap
    let cs = u32::try_from(positions.len()).unwrap_or(u32::MAX);
    positions.push(rings[0].0);
    for k in 0..n_sides {
        tris.push([cs, idx(0, k + 1), idx(0, k)]);
    }
    let last = nr - 1;
    let (tip_c, tip_t, _, tip_r) = rings[last];
    if end {
        // a hemisphere dome on the end ring
        let mut prev: Vec<u32> = (0..n_sides).map(|k| idx(last, k)).collect();
        for lat in 1..=style.dome_rings {
            #[allow(clippy::cast_precision_loss)]
            let phi = std::f64::consts::FRAC_PI_2 * lat as f64 / style.dome_rings as f64;
            let centre = add(tip_c, scale(tip_t, tip_r * libm::sin(phi)));
            if lat < style.dome_rings {
                let rr = tip_r * libm::cos(phi);
                let base = positions.len();
                for a in &angles {
                    let (ca, sa) = (libm::cos(*a), libm::sin(*a));
                    positions.push([0, 1, 2].map(|d| centre[d] + rr * (ca * u[d] + sa * v[d])));
                }
                let cur: Vec<u32> = (0..n_sides)
                    .map(|k| u32::try_from(base + k).unwrap_or(u32::MAX))
                    .collect();
                for k in 0..n_sides {
                    let k1 = (k + 1) % n_sides;
                    tris.push([prev[k], prev[k1], cur[k1]]);
                    tris.push([prev[k], cur[k1], cur[k]]);
                }
                prev = cur;
            } else {
                let pole = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                positions.push(centre);
                for k in 0..n_sides {
                    tris.push([pole, prev[k], prev[(k + 1) % n_sides]]);
                }
            }
        }
    } else {
        let ce = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        positions.push(tip_c);
        for k in 0..n_sides {
            tris.push([ce, idx(last, k), idx(last, k + 1)]);
        }
    }
    let mut mesh = TriMesh {
        positions,
        triangles: tris,
        ..TriMesh::default()
    };
    if crate::signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    (mesh, (u, v))
}

fn interp_arc(arc: &[f64], t: f64) -> f64 {
    let x = t * 64.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let k = (x.floor() as usize).min(63);
    #[allow(clippy::cast_precision_loss)]
    let f = x - k as f64;
    arc[k] + f * (arc[k + 1] - arc[k])
}

/// A frame round `w` (`_basis`).
#[must_use]
pub fn basis(w: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let r = if dot(w, [0.0, 0.0, 1.0]).abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = unit(cross(w, r)).unwrap_or([1.0, 0.0, 0.0]);
    let v = unit(cross(w, u)).unwrap_or([0.0, 1.0, 0.0]);
    (u, v)
}

/// Parallel transport of a frame onto a new tangent (`_transport`).
fn transport(u: [f64; 3], t: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let un = sub(u, scale(t, dot(u, t)));
    match unit(un) {
        Some(un) => (un, unit(cross(t, un)).unwrap_or([0.0, 1.0, 0.0])),
        None => basis(t),
    }
}

// --- small vector helpers (public: looks use them for their own frames) ---

/// a − b.
#[must_use]
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
/// a + b.
#[must_use]
pub fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
/// a × s.
#[must_use]
pub fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
/// a · b.
#[must_use]
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// a × b.
#[must_use]
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// |a|.
#[must_use]
pub fn norm(a: [f64; 3]) -> f64 {
    libm::sqrt(dot(a, a))
}
/// |a − b|.
#[must_use]
pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(a, b))
}
/// a / |a|, or `None` for a (near) zero vector.
#[must_use]
pub fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    (n > 1e-12).then(|| scale(a, 1.0 / n))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::needless_range_loop)]
    use super::*;

    fn cloud(seed: u64, n: usize) -> Vec<[f64; 3]> {
        let mut rng = dt_geom::Rng::new(dt_geom::Seed::new(seed));
        (0..n)
            .map(|_| {
                let a = rng.uniform(0.0, std::f64::consts::TAU);
                let r = rng.uniform(4.0, 12.0);
                [r * libm::cos(a), r * libm::sin(a), rng.uniform(24.0, 40.0)]
            })
            .collect()
    }

    fn params() -> Colonise {
        Colonise {
            step_mm: 1.0,
            split_cos: 0.8 * libm::cos(30f64.to_radians()),
            aim_sin: libm::sin(35f64.to_radians()),
            floor_deg: 26.0,
            climb_deg: 26.0,
            max_branches: 3,
            alpha: 0.1,
            max_steps: 160,
            aim: 0.33,
            balance: 1.0,
        }
    }

    #[test]
    fn invariants_hold_over_many_seeds() {
        for seed in 1..=60 {
            let pts = cloud(seed, 40);
            let groups: Vec<usize> = (0..pts.len()).map(|i| i % 4).collect();
            let gr = if seed % 2 == 0 {
                Some(groups.as_slice())
            } else {
                None
            };
            let t = colonise([0.0, 0.0, 0.0], &pts, gr, &params());
            let g = simplify(&t);
            let ch = g.children();
            // S1, S2: every target an end, at its (possibly moved) position
            let ends: Vec<usize> = (0..g.nodes.len()).filter(|&i| ch[i].is_empty()).collect();
            assert_eq!(ends.len(), pts.len(), "seed {seed}");
            for &e in &ends {
                let k = g.target[e].unwrap();
                assert!(dist(g.nodes[e], t.targets[k]) < 1e-12);
            }
            // S3: no target is a fork or in-between node
            for i in 0..g.nodes.len() {
                if !ch[i].is_empty() {
                    assert!(g.target[i].is_none());
                }
            }
            // S4: every chord, and every curve, at or above the floor
            let min = g.elevations().into_iter().fold(90.0, f64::min);
            assert!(min >= 26.0, "seed {seed}: {min}");
            let (cv, _) = curves(&g, 0.45, 26.0);
            for c in cv.iter().flatten() {
                assert!(c.min_elevation() >= 26.0 - 1e-9);
            }
        }
    }

    #[test]
    fn a_target_below_the_floor_is_moved_onto_it() {
        let pts = [[10.0, 0.0, 1.0], [0.0, 0.0, 20.0]];
        let t = colonise([0.0, 0.0, 0.0], &pts, None, &params());
        assert!(t.moved >= 1);
        let g = simplify(&t);
        assert!(g.elevations().into_iter().all(|e| e >= 26.0));
    }

    #[test]
    fn pipe_radii_follow_the_pipe_model() {
        let pts = cloud(3, 20);
        let mut g = simplify(&colonise([0.0; 3], &pts, None, &params()));
        pipe_radii(&mut g, 3.0, 1.0);
        let ch = g.children();
        for i in 0..g.nodes.len() {
            if ch[i].is_empty() {
                assert!((g.radii[i] - 1.0).abs() < 1e-12);
            } else {
                let s: f64 = ch[i].iter().map(|&c| g.radii[c].powi(3)).sum();
                assert!((g.radii[i] - s.cbrt()).abs() < 1e-9);
            }
        }
        // 20 ends of radius 1 under exponent 3: the root is 20^(1/3)
        assert!((g.radii[0] - 20f64.cbrt()).abs() < 1e-9);
    }

    #[test]
    fn tubes_are_closed() {
        let pts = cloud(5, 16);
        let mut g = simplify(&colonise([0.0; 3], &pts, None, &params()));
        pipe_radii(&mut g, 3.0, 1.0);
        let (cv, _) = curves(&g, 0.45, 26.0);
        let style = TubeStyle {
            step_mm: 0.9,
            min_radius_mm: 0.42,
            dome_rings: 4,
        };
        let ts = tubes(&g, &cv, &style, &|_| 12, &|_, _, _| {});
        for m in ts.iter().filter(|m| !m.triangles.is_empty()) {
            assert!(m.is_watertight());
            assert!(crate::signed_volume(m) > 0.0);
        }
    }
}
