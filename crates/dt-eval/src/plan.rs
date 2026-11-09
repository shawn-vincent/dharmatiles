//! The plan-then-build scheduler: one dependency graph, one sort.
//!
//! Every stack entry first says what it writes and reads ([`Plan`]). Each
//! is a node of the graph, and so is the lift, which reads every surface
//! and every fill and writes the ground solid:
//!
//! - a surface reads nothing (surfaces are painted before any solid);
//! - a solid laid with the land (a floor) waits for the surfaces and the
//!   lift waits for it, so it stands on the land and the lift sees it;
//! - a fill (water) reads the surfaces, and the solids it names;
//! - a read of a kind waits for every entry that writes one (the ground's
//!   writer is the lift, when a surface paints); a read of every solid
//!   (the connector's) waits for everything else.
//!
//! The lift is a step of every plan: a fill alone paints what it lifts,
//! and with nothing painted it lifts nothing.
//!
//! The order is one stable topological sort: at each step the first node
//! ready, surfaces first in stack order, then what the lift needs, the
//! lift, and the rest in stack order. So stack order is free for solids;
//! it only decides which surface paints over which.
//!
//! A read nothing in the tile writes, a cycle, or a surface that reads a
//! solid is refused here, before anything is built.

use dt_elements::{Kind, Plan, Read, Write};

/// One step of the build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Build stack entry `i`.
    Entry(usize),
    /// Lift the painted surface.
    Lift,
}

/// Why a plan cannot be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Entry `entry` reads solids of `kind`, and nothing writes one.
    NoWriter {
        /// The reader.
        entry: usize,
        /// What it reads.
        kind: Kind,
    },
    /// Entry `entry` is a surface that reads a solid.
    SurfaceReadsSolid {
        /// The reader.
        entry: usize,
    },
    /// These entries read each other's solids.
    Cycle {
        /// The entries left unordered.
        entries: Vec<usize>,
    },
}

/// The build order for `plans` (one per stack entry, in stack order).
///
/// # Errors
/// [`Refusal`] for a read that cannot be satisfied.
pub fn schedule(plans: &[Plan]) -> Result<Vec<Step>, Refusal> {
    let n = plans.len();
    let lift = n; // the lift's node
    let is = |i: usize, w: Write| plans[i].writes == w;
    // a solid of `kind`, however it is laid
    let is_solid = |i: usize, kind: Kind| is(i, Write::Solid(kind)) || is(i, Write::Laid(kind));
    let laid: Vec<usize> = (0..n)
        .filter(|&i| matches!(plans[i].writes, Write::Laid(_)))
        .collect();
    let surfaces: Vec<usize> = (0..n).filter(|&i| is(i, Write::Surface)).collect();
    if let Some(&i) = surfaces.iter().find(|&&i| !plans[i].reads.is_empty()) {
        return Err(Refusal::SurfaceReadsSolid { entry: i });
    }
    let lifted = !surfaces.is_empty();
    // what each kind of solid is written by
    let writers = |kind: Kind, me: usize| -> Vec<usize> {
        let mut w: Vec<usize> = (0..n).filter(|&j| j != me && is_solid(j, kind)).collect();
        if kind == Kind::Terrain && lifted {
            w.push(lift);
        }
        w
    };
    // waits[v]: the nodes v waits for
    let mut waits: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for i in 0..n {
        if is(i, Write::Fill) || matches!(plans[i].writes, Write::Laid(_)) {
            waits[i].extend(&surfaces);
        }
        for r in &plans[i].reads {
            match *r {
                Read::Solids(kind) | Read::IfAny(kind) => {
                    let w = writers(kind, i);
                    if w.is_empty() && matches!(r, Read::Solids(_)) {
                        return Err(Refusal::NoWriter { entry: i, kind });
                    }
                    waits[i].extend(w);
                }
                Read::AllSolids => {
                    waits[i].extend((0..n).filter(|&j| {
                        j != i
                            && !is(j, Write::Surface)
                            && !plans[j].reads.contains(&Read::AllSolids)
                    }));
                    waits[i].push(lift);
                }
            }
        }
    }
    // the lift is always a step (a fill alone paints the surface it
    // lifts); it is the ground's writer only when a surface paints
    waits[lift] = (0..n)
        .filter(|&i| is(i, Write::Surface) || is(i, Write::Fill))
        .chain(laid)
        .collect();
    // what the lift needs, all the way down
    let mut needed = vec![false; n + 1];
    let mut todo = waits[lift].clone();
    while let Some(v) = todo.pop() {
        if !needed[v] {
            needed[v] = true;
            todo.extend(&waits[v]);
        }
    }
    // surfaces, what the lift needs, the lift, the rest; each in stack order
    let rank = |v: usize| -> (u8, usize) {
        let class = if v == lift {
            2
        } else if is(v, Write::Surface) {
            0
        } else if needed[v] {
            1
        } else {
            3
        };
        (class, v)
    };
    let mut left: Vec<usize> = (0..=n).collect();
    left.sort_by_key(|&v| rank(v));
    let mut done = vec![false; n + 1];
    let mut steps = Vec::with_capacity(left.len());
    while !left.is_empty() {
        let Some(k) = left.iter().position(|&v| waits[v].iter().all(|&w| done[w])) else {
            return Err(Refusal::Cycle {
                entries: left.into_iter().filter(|&v| v != lift).collect(),
            });
        };
        let v = left.remove(k);
        done[v] = true;
        steps.push(if v == lift {
            Step::Lift
        } else {
            Step::Entry(v)
        });
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(writes: Write, reads: &[Read]) -> Plan {
        Plan {
            writes,
            reads: reads.to_vec(),
            cell_mm: None,
        }
    }

    #[test]
    fn surfaces_then_lift_then_solids_then_the_connector() {
        let plans = [
            p(Write::Solid(Kind::Connector), &[Read::AllSolids]),
            p(Write::Solid(Kind::Grass), &[Read::Solids(Kind::Rock)]),
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Rock), &[Read::Solids(Kind::Terrain)]),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(2),
                Step::Lift,
                Step::Entry(3),
                Step::Entry(1),
                Step::Entry(0)
            ])
        );
    }

    #[test]
    fn a_floor_is_laid_before_the_lift_and_a_wall_stands_on_it() {
        let plans = [
            p(Write::Solid(Kind::Wall), &[Read::IfAny(Kind::Floor)]),
            p(Write::Surface, &[]),
            p(Write::Laid(Kind::Floor), &[]),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(1),
                Step::Entry(2),
                Step::Lift,
                Step::Entry(0)
            ])
        );
        // laid before the lift, it cannot read the lifted ground
        let cycle = [
            p(Write::Surface, &[]),
            p(Write::Laid(Kind::Floor), &[Read::Solids(Kind::Terrain)]),
        ];
        assert!(matches!(schedule(&cycle), Err(Refusal::Cycle { .. })));
    }

    #[test]
    fn refuses_what_it_cannot_satisfy() {
        let none = [
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Grass), &[Read::Solids(Kind::Cliff)]),
        ];
        assert_eq!(
            schedule(&none),
            Err(Refusal::NoWriter {
                entry: 1,
                kind: Kind::Cliff
            })
        );
        let cycle = [
            p(Write::Solid(Kind::Rock), &[Read::Solids(Kind::Tree)]),
            p(Write::Solid(Kind::Tree), &[Read::Solids(Kind::Rock)]),
        ];
        assert_eq!(
            schedule(&cycle),
            Err(Refusal::Cycle {
                entries: vec![0, 1]
            })
        );
        let early = [p(Write::Surface, &[Read::Solids(Kind::Rock)])];
        assert_eq!(
            schedule(&early),
            Err(Refusal::SurfaceReadsSolid { entry: 0 })
        );
        let optional = [
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Wall), &[Read::IfAny(Kind::Floor)]),
        ];
        assert!(schedule(&optional).is_ok());
        let no_ground = [p(Write::Solid(Kind::Rock), &[Read::Solids(Kind::Terrain)])];
        assert!(schedule(&no_ground).is_err());
        let fill_on_ground = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::Solids(Kind::Terrain)]),
        ];
        assert!(schedule(&fill_on_ground).is_err());
        // a fill's prerequisite cannot read the ground either
        let rock_on_ground = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::IfAny(Kind::Rock)]),
            p(Write::Solid(Kind::Rock), &[Read::Solids(Kind::Terrain)]),
        ];
        assert_eq!(
            schedule(&rock_on_ground),
            Err(Refusal::Cycle {
                entries: vec![1, 2]
            })
        );
    }

    #[test]
    fn a_fill_and_what_it_reads_come_before_the_lift() {
        let plans = [
            p(Write::Solid(Kind::Connector), &[Read::AllSolids]),
            p(Write::Fill, &[Read::IfAny(Kind::Rock)]),
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Tree), &[Read::Solids(Kind::Terrain)]),
            p(Write::Solid(Kind::Rock), &[]),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(2),
                Step::Entry(4),
                Step::Entry(1),
                Step::Lift,
                Step::Entry(3),
                Step::Entry(0)
            ])
        );
        // water alone: it fills, then the lift lifts it
        assert_eq!(
            schedule(&[p(Write::Fill, &[Read::IfAny(Kind::Rock)])]),
            Ok(vec![Step::Entry(0), Step::Lift])
        );
        // no rocks: the water waits for nothing
        let bare = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::IfAny(Kind::Rock)]),
        ];
        assert_eq!(
            schedule(&bare),
            Ok(vec![Step::Entry(0), Step::Entry(1), Step::Lift])
        );
    }
}
