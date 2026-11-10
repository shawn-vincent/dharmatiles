//! The plan: stack order is build order.
//!
//! Every stack entry is built in the order the stack lists it, and each
//! stands on the terrain as laid so far: the surfaces (ground, dirt, floor,
//! water) painted above it in the stack, and the tops the loose solids
//! listed above it laid. A dirt listed after a rock, a tree or a flower is
//! built after it, so it lies on the rock's top, banks against the trunk and
//! covers what it covers; a thing listed after a dirt stands on it.
//!
//! Only two things are placed by what they are, not where they are listed:
//!
//! - the lift (the one solid the surfaces make together) comes right after
//!   the last entry that lays a layer (a surface, a fill, a laid floor),
//!   because nothing is laid once the ground is solid; with no layer at all
//!   it comes first;
//! - an entry that reads every solid in the tile (the connector) is built
//!   last: it reads the finished tile.
//!
//! The plan refuses what the listed order cannot satisfy, before anything
//! is built, and never reorders to cure it: a read of the terrain or of a
//! kind of solid that nothing listed above writes; a fill or a floor listed
//! above a surface it would lie on; a surface that reads a solid.

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
    /// Entry `entry` needs what only entry `later` writes, and that is
    /// listed after it.
    Early {
        /// The reader.
        entry: usize,
        /// The first entry that writes it.
        later: usize,
        /// What it reads: `None` for the ground a fill or a floor lies on.
        kind: Option<Kind>,
    },
}

/// The build order for `plans` (one per stack entry, in stack order).
///
/// # Errors
/// [`Refusal`] for a read the listed order cannot satisfy.
pub fn schedule(plans: &[Plan]) -> Result<Vec<Step>, Refusal> {
    let n = plans.len();
    let is = |i: usize, w: Write| plans[i].writes == w;
    let layer = |i: usize| {
        matches!(
            plans[i].writes,
            Write::Surface | Write::Fill | Write::Laid(_)
        )
    };
    // a solid of `kind`, however it is laid
    let is_solid = |i: usize, kind: Kind| {
        is(i, Write::Solid(kind))
            || is(i, Write::Laid(kind))
            || matches!(plans[i].writes, Write::Solids(ks) if ks.contains(&kind))
    };
    if let Some(i) = (0..n).find(|&i| is(i, Write::Surface) && !plans[i].reads.is_empty()) {
        return Err(Refusal::SurfaceReadsSolid { entry: i });
    }
    // the order: stack order, the lift after the last layer, what reads
    // everything at the end
    let last = (0..n).rfind(|&i| layer(i));
    let finale = |i: usize| plans[i].reads.contains(&Read::AllSolids);
    let mut steps = Vec::with_capacity(n + 1);
    if last.is_none() {
        steps.push(Step::Lift);
    }
    for i in (0..n).filter(|&i| !finale(i)) {
        steps.push(Step::Entry(i));
        if last == Some(i) {
            steps.push(Step::Lift);
        }
    }
    // a layer is never a finale; the lift is placed even if the last layer
    // were one
    for i in (0..n).filter(|&i| finale(i)) {
        steps.push(Step::Entry(i));
    }
    let pos = |i: usize| {
        steps
            .iter()
            .position(|s| *s == Step::Entry(i))
            .unwrap_or(usize::MAX)
    };
    let ground: Vec<usize> = (0..n).filter(|&j| is(j, Write::Surface)).collect();
    #[allow(clippy::needless_range_loop)] // `i` also names the entry in `pos`
    for i in 0..n {
        // a fill or a floor lies on the ground painted before it
        if matches!(plans[i].writes, Write::Fill | Write::Laid(_))
            && let Some(&later) = ground.iter().find(|&&j| pos(j) > pos(i))
        {
            return Err(Refusal::Early {
                entry: i,
                later,
                kind: None,
            });
        }
        for r in &plans[i].reads {
            let Read::Solids(kind) = *r else { continue };
            // the terrain is what the surfaces make; the rest, what writes it
            let writers: Vec<usize> = (0..n)
                .filter(|&j| {
                    j != i
                        && if kind == Kind::Terrain {
                            is(j, Write::Surface)
                        } else {
                            is_solid(j, kind)
                        }
                })
                .collect();
            let Some(&first) = writers.iter().min_by_key(|&&j| pos(j)) else {
                return Err(Refusal::NoWriter { entry: i, kind });
            };
            if pos(first) > pos(i) {
                return Err(Refusal::Early {
                    entry: i,
                    later: first,
                    kind: Some(kind),
                });
            }
        }
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

    fn on_ground(writes: Write) -> Plan {
        p(writes, &[Read::Solids(Kind::Terrain)])
    }

    #[test]
    fn the_stack_is_built_in_the_order_it_is_listed() {
        let rock = || p(Write::Solid(Kind::Rock), &[]);
        let dirt = || p(Write::Surface, &[]);
        let tree = || on_ground(Write::Solid(Kind::Tree));
        // surfaces, then the lift, then things
        assert_eq!(
            schedule(&[dirt(), rock()]),
            Ok(vec![Step::Entry(0), Step::Lift, Step::Entry(1)])
        );
        // a rock before the dirt: the dirt is built after it
        assert_eq!(
            schedule(&[rock(), dirt()]),
            Ok(vec![Step::Entry(0), Step::Entry(1), Step::Lift])
        );
        // a dirt after a tree is built after it, and the lift after both
        assert_eq!(
            schedule(&[dirt(), tree(), dirt(), rock()]),
            Ok(vec![
                Step::Entry(0),
                Step::Entry(1),
                Step::Entry(2),
                Step::Lift,
                Step::Entry(3)
            ])
        );
        // a mix of rocks and trees listed after the dirt, before another
        let mix = p(
            Write::Solids(&[Kind::Rock, Kind::Tree]),
            &[Read::Solids(Kind::Terrain)],
        );
        assert_eq!(
            schedule(&[dirt(), mix, dirt()]),
            Ok(vec![
                Step::Entry(0),
                Step::Entry(1),
                Step::Entry(2),
                Step::Lift
            ])
        );
    }

    #[test]
    fn the_connector_reads_the_finished_tile_and_is_built_last() {
        let plans = [
            p(Write::Solid(Kind::Connector), &[Read::AllSolids]),
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Rock), &[]),
            p(Write::Solid(Kind::Grass), &[Read::Solids(Kind::Rock)]),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(1),
                Step::Lift,
                Step::Entry(2),
                Step::Entry(3),
                Step::Entry(0)
            ])
        );
    }

    #[test]
    fn a_floor_is_laid_before_the_lift_and_a_wall_stands_on_it() {
        let plans = [
            p(Write::Surface, &[]),
            p(Write::Laid(Kind::Floor), &[]),
            p(
                Write::Solid(Kind::Wall),
                &[Read::Solids(Kind::Terrain), Read::IfAny(Kind::Floor)],
            ),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(0),
                Step::Entry(1),
                Step::Lift,
                Step::Entry(2)
            ])
        );
        // a floor cannot read the lifted ground: it is laid before the lift
        // and Terrain is what the surfaces made, so it may read that
        let early = [p(Write::Laid(Kind::Floor), &[]), p(Write::Surface, &[])];
        assert_eq!(
            schedule(&early),
            Err(Refusal::Early {
                entry: 0,
                later: 1,
                kind: None
            })
        );
    }

    #[test]
    fn refuses_what_the_listed_order_cannot_satisfy() {
        // nothing makes a cliff
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
        // a tree above the ground it stands on
        let tree_first = [on_ground(Write::Solid(Kind::Tree)), p(Write::Surface, &[])];
        assert_eq!(
            schedule(&tree_first),
            Err(Refusal::Early {
                entry: 0,
                later: 1,
                kind: Some(Kind::Terrain)
            })
        );
        // grass above the rocks it reads
        let grass_first = [
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Grass), &[Read::Solids(Kind::Rock)]),
            p(Write::Solid(Kind::Rock), &[]),
        ];
        assert_eq!(
            schedule(&grass_first),
            Err(Refusal::Early {
                entry: 1,
                later: 2,
                kind: Some(Kind::Rock)
            })
        );
        // a read nothing may satisfy: no surface at all
        let no_ground = [on_ground(Write::Solid(Kind::Rock))];
        assert_eq!(
            schedule(&no_ground),
            Err(Refusal::NoWriter {
                entry: 0,
                kind: Kind::Terrain
            })
        );
        let early = [p(Write::Surface, &[Read::Solids(Kind::Rock)])];
        assert_eq!(
            schedule(&early),
            Err(Refusal::SurfaceReadsSolid { entry: 0 })
        );
        // an optional read is not refused, listed above or below
        for plans in [
            [
                p(Write::Surface, &[]),
                p(Write::Solid(Kind::Wall), &[Read::IfAny(Kind::Floor)]),
                p(Write::Laid(Kind::Floor), &[]),
            ],
            [
                p(Write::Surface, &[]),
                p(Write::Laid(Kind::Floor), &[]),
                p(Write::Solid(Kind::Wall), &[Read::IfAny(Kind::Floor)]),
            ],
        ] {
            assert!(schedule(&plans).is_ok());
        }
    }

    #[test]
    fn a_fill_comes_before_the_lift_and_after_what_it_reads_above_it() {
        let plans = [
            p(Write::Surface, &[]),
            p(Write::Solid(Kind::Rock), &[]),
            p(Write::Fill, &[Read::IfAny(Kind::Rock)]),
            on_ground(Write::Solid(Kind::Tree)),
            p(Write::Solid(Kind::Connector), &[Read::AllSolids]),
        ];
        assert_eq!(
            schedule(&plans),
            Ok(vec![
                Step::Entry(0),
                Step::Entry(1),
                Step::Entry(2),
                Step::Lift,
                Step::Entry(3),
                Step::Entry(4)
            ])
        );
        // water alone: it fills, then the lift lifts it
        assert_eq!(
            schedule(&[p(Write::Fill, &[Read::IfAny(Kind::Rock)])]),
            Ok(vec![Step::Entry(0), Step::Lift])
        );
        // a tree listed above the water is built on the ground without it
        let tree_above = [
            p(Write::Surface, &[]),
            on_ground(Write::Solid(Kind::Tree)),
            p(Write::Fill, &[]),
        ];
        assert_eq!(
            schedule(&tree_above),
            Ok(vec![
                Step::Entry(0),
                Step::Entry(1),
                Step::Entry(2),
                Step::Lift
            ])
        );
    }
}
