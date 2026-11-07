//! Tracing a branching graph from a root to its targets by space
//! colonisation, invariants S1–S4 held by construction.

use std::collections::VecDeque;

use super::*;

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
