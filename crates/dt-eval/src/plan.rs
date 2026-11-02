//! The plan-then-build scheduler.
//!
//! Every stack entry first says what it writes and reads ([`Plan`]). The
//! order follows from that, not from where things are listed (R6 D3:
//! stack order is free for solids; it only decides which surface paints
//! over which):
//!
//! 1. surfaces, painted in stack order;
//! 2. fills (water), each after the solids it reads, and those solids
//!    (with what they read in turn) before it;
//! 3. the lift of the painted, filled surface into the `ground` solid;
//! 4. the other solids, each after every writer of what it reads (stable
//!    in stack order otherwise); a read of every solid (the connector's)
//!    comes after all the others.
//!
//! A read nothing in the tile writes, a cycle, or a surface that reads a
//! solid (P1 paints surfaces before any solid exists) is refused here,
//! before anything is built (risk R8).

use dt_looks::{Plan, Read, Write};

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
        kind: &'static str,
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
    let mut steps = Vec::with_capacity(plans.len() + 1);
    let mut solids = Vec::new();
    let mut surfaces = 0;
    for (i, p) in plans.iter().enumerate() {
        match p.writes {
            Write::Surface if !p.reads.is_empty() => {
                return Err(Refusal::SurfaceReadsSolid { entry: i });
            }
            Write::Surface => {
                steps.push(Step::Entry(i));
                surfaces += 1;
            }
            Write::Solid(_) | Write::Fill => solids.push(i),
        }
    }
    let lifted = surfaces > 0;
    // must[i]: the entries i waits for
    let mut must: Vec<Vec<usize>> = vec![Vec::new(); plans.len()];
    for &i in &solids {
        for r in &plans[i].reads {
            match *r {
                Read::Solids(kind) | Read::AnySolids(kind) => {
                    let writers: Vec<usize> = solids
                        .iter()
                        .copied()
                        .filter(|&j| j != i && plans[j].writes == Write::Solid(kind))
                        .collect();
                    // the lifted ground is a solid every tile with a surface has
                    let ground = kind == "ground" && lifted;
                    if writers.is_empty() && !ground && matches!(r, Read::Solids(_)) {
                        return Err(Refusal::NoWriter { entry: i, kind });
                    }
                    must[i].extend(writers);
                }
                Read::AllSolids => must[i].extend(
                    solids
                        .iter()
                        .copied()
                        .filter(|&j| j != i && !plans[j].reads.contains(&Read::AllSolids)),
                ),
            }
        }
    }
    // before the lift: the fills and everything they wait for
    let mut early = vec![false; plans.len()];
    let mut todo: Vec<usize> = solids
        .iter()
        .copied()
        .filter(|&i| plans[i].writes == Write::Fill)
        .collect();
    while let Some(i) = todo.pop() {
        if !early[i] {
            early[i] = true;
            todo.extend(&must[i]);
        }
    }
    // a fill and what it waits for come before the lift that makes the
    // ground, so none of them can read the ground
    let reads_ground = |i: usize| {
        plans[i].reads.iter().any(|r| {
            matches!(
                r,
                Read::Solids("ground") | Read::AnySolids("ground") | Read::AllSolids
            )
        })
    };
    let early_ground: Vec<usize> = (0..plans.len())
        .filter(|&i| early[i] && reads_ground(i))
        .collect();
    if lifted && !early_ground.is_empty() {
        return Err(Refusal::Cycle {
            entries: early_ground,
        });
    }
    let mut done = vec![false; plans.len()];
    for pass in [true, false] {
        let mut left: Vec<usize> = solids
            .iter()
            .copied()
            .filter(|&i| early[i] == pass)
            .collect();
        while !left.is_empty() {
            let Some(k) = left.iter().position(|&i| must[i].iter().all(|&j| done[j])) else {
                return Err(Refusal::Cycle { entries: left });
            };
            let i = left.remove(k);
            done[i] = true;
            steps.push(Step::Entry(i));
        }
        if pass {
            steps.push(Step::Lift);
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

    #[test]
    fn surfaces_then_lift_then_solids_then_the_connector() {
        let plans = [
            p(Write::Solid("connector"), &[Read::AllSolids]),
            p(Write::Solid("grass"), &[Read::Solids("rock")]),
            p(Write::Surface, &[]),
            p(Write::Solid("rock"), &[Read::Solids("ground")]),
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
    fn refuses_what_it_cannot_satisfy() {
        let none = [
            p(Write::Surface, &[]),
            p(Write::Solid("grass"), &[Read::Solids("cliff")]),
        ];
        assert_eq!(
            schedule(&none),
            Err(Refusal::NoWriter {
                entry: 1,
                kind: "cliff"
            })
        );
        let cycle = [
            p(Write::Solid("a"), &[Read::Solids("b")]),
            p(Write::Solid("b"), &[Read::Solids("a")]),
        ];
        assert_eq!(
            schedule(&cycle),
            Err(Refusal::Cycle {
                entries: vec![0, 1]
            })
        );
        let early = [p(Write::Surface, &[Read::Solids("rock")])];
        assert_eq!(
            schedule(&early),
            Err(Refusal::SurfaceReadsSolid { entry: 0 })
        );
        let no_ground = [p(Write::Solid("rock"), &[Read::Solids("ground")])];
        assert!(schedule(&no_ground).is_err());
        let fill_on_ground = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::Solids("ground")]),
        ];
        assert!(schedule(&fill_on_ground).is_err());
        // a fill's prerequisite cannot read the ground either
        let rock_on_ground = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::AnySolids("rock")]),
            p(Write::Solid("rock"), &[Read::Solids("ground")]),
        ];
        assert_eq!(
            schedule(&rock_on_ground),
            Err(Refusal::Cycle { entries: vec![2] })
        );
    }

    #[test]
    fn a_fill_and_what_it_reads_come_before_the_lift() {
        let plans = [
            p(Write::Solid("connector"), &[Read::AllSolids]),
            p(Write::Fill, &[Read::AnySolids("rock")]),
            p(Write::Surface, &[]),
            p(Write::Solid("tree"), &[Read::Solids("ground")]),
            p(Write::Solid("rock"), &[]),
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
        // no rocks: the water waits for nothing
        let bare = [
            p(Write::Surface, &[]),
            p(Write::Fill, &[Read::AnySolids("rock")]),
        ];
        assert_eq!(
            schedule(&bare),
            Ok(vec![Step::Entry(0), Step::Entry(1), Step::Lift])
        );
    }
}
