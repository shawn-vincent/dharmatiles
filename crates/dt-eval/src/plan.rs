//! The plan-then-build scheduler.
//!
//! Every stack entry first says what it writes and reads ([`Plan`]). The
//! order follows from that, not from where things are listed (R6 D3:
//! stack order is free for solids; it only decides which surface paints
//! over which):
//!
//! 1. surfaces, painted in stack order;
//! 2. the lift of the painted surface into the `ground` solid;
//! 3. solids, each after every writer of what it reads (stable in stack
//!    order otherwise); a read of every solid (the connector's) comes after
//!    all the others.
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
    for (i, p) in plans.iter().enumerate() {
        match p.writes {
            Write::Surface if !p.reads.is_empty() => {
                return Err(Refusal::SurfaceReadsSolid { entry: i });
            }
            Write::Surface => steps.push(Step::Entry(i)),
            Write::Solid(_) => solids.push(i),
        }
    }
    steps.push(Step::Lift);
    // must[i]: the solid entries i waits for
    let mut must: Vec<Vec<usize>> = vec![Vec::new(); plans.len()];
    for &i in &solids {
        for r in &plans[i].reads {
            match *r {
                Read::Solids(kind) => {
                    let writers: Vec<usize> = solids
                        .iter()
                        .copied()
                        .filter(|&j| j != i && plans[j].writes == Write::Solid(kind))
                        .collect();
                    // the lifted ground is a solid every tile with a surface has
                    let ground = kind == "ground" && steps.len() > 1;
                    if writers.is_empty() && !ground {
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
    let mut done = vec![false; plans.len()];
    let mut left = solids;
    while !left.is_empty() {
        let Some(k) = left.iter().position(|&i| must[i].iter().all(|&j| done[j])) else {
            return Err(Refusal::Cycle { entries: left });
        };
        let i = left.remove(k);
        done[i] = true;
        steps.push(Step::Entry(i));
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
    }
}
