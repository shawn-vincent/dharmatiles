//! A map in the editor: the scene a `.dmap` with placed pieces draws.  Reading a map is [`dt_service::scene_read`], written
//! once on a host the program's [`Shared`] and a web worker's `Service` both are.  What is here is what only the program
//! does with it: a crown over a taller neighbour is a note, found from the pieces' real shapes (their low builds, on the
//! tier lane), never a refusal.

use std::collections::BTreeMap;

use dt_eval::Options;
use dt_store::Tier;

use super::Shared;

pub use dt_service::scene::*;
pub use dt_service::scene_read::{PieceInfo, Scene, read};

/// What a built piece's shape says: its highest point, and the part of it
/// that stands out past its square (a crown) with the lowest point there.
struct Shape {
    top: f64,
    crown: Option<([f64; 4], f64)>,
}

fn shape_of(mesh: &dt_kit::TriMesh, w: f64, d: f64) -> Shape {
    let mut top = f64::MIN;
    let mut crown: Option<([f64; 4], f64)> = None;
    let slack = 0.5;
    for p in &mesh.positions {
        let [x, y, z] = p.map(f64::from);
        top = top.max(z);
        if x < -slack || x > w + slack || y < -slack || y > d + slack {
            let c = crown.get_or_insert(([x, y, x, y], z));
            c.0 = [c.0[0].min(x), c.0[1].min(y), c.0[2].max(x), c.0[3].max(y)];
            c.1 = c.1.min(z);
        }
    }
    Shape { top, crown }
}

/// A crown over a taller neighbour: found from each piece's low build (kept
/// beside the full one), said as a note for each pair.  Never a refusal.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where a piece whose build failed has no shape to crowd; the failure is shown on that piece by its own build"
)]
pub(super) fn crowns(shared: &Shared, scene: &Scene) -> Vec<Note> {
    let base = shared.options();
    let mut shapes: BTreeMap<(String, u64, dt_lang::System), Option<Shape>> = BTreeMap::new();
    let placed: Vec<&PieceInfo> = scene
        .pieces
        .iter()
        .filter(|p| p.at.is_some() && p.error.is_none())
        .collect();
    for p in &placed {
        let key = (p.path.clone(), p.seed_used, p.system);
        if shapes.contains_key(&key) {
            continue;
        }
        let Some(at) = p.at else { continue };
        let mut opts = Options {
            system: p.system,
            seed: p.seed,
            ..base.clone()
        };
        if opts
            .use_printer(&scene.printer, shared.home.as_deref())
            .is_err()
        {
            shapes.insert(key, None);
            continue;
        }
        // a piece's low build is a job on the tier lane, like the page's own look at it
        let path = p.path.clone();
        let jobkey = format!("tier|crown|{}|{:?}|{}", p.path, opts.seed, opts.system);
        let built = shared.on_lane(
            super::queue::Lane::Tier,
            jobkey,
            shared.rev_of(&path),
            "",
            move |reply| super::queue::SideJob::Crown {
                path,
                opts: Options {
                    tier: Tier::Low,
                    ..opts
                },
                reply,
            },
        );
        let built = match built {
            Ok(b) => b,
            // a newer reading of the map is on its way and will look again
            Err(super::queue::Interrupted::Superseded) => {
                shapes.insert(key, None);
                continue;
            }
            Err(super::queue::Interrupted::Panicked(pn)) => std::panic::resume_unwind(pn),
        };
        shapes.insert(
            key,
            built.ok().map(|b| shape_of(&b.export.mesh, at.w, at.d)),
        );
    }
    let mut notes = Vec::new();
    for a in &placed {
        let (Some(pa), Some(Some(sa))) =
            (a.at, shapes.get(&(a.path.clone(), a.seed_used, a.system)))
        else {
            continue;
        };
        let Some((cbox, clow)) = sa.crown else {
            continue;
        };
        let reach = pa.box_on_map(cbox);
        let crown_z = pa.z + clow;
        for b in &placed {
            if b.index == a.index {
                continue;
            }
            let (Some(pb), Some(Some(sb))) =
                (b.at, shapes.get(&(b.path.clone(), b.seed_used, b.system)))
            else {
                continue;
            };
            let f = pb.footprint();
            let (ox, oy) = (
                reach[2].min(f[2]) - reach[0].max(f[0]),
                reach[3].min(f[3]) - reach[1].max(f[1]),
            );
            let top = pb.z + sb.top;
            if ox > 0.5 && oy > 0.5 && top > crown_z + 0.5 {
                let who = |p: &PieceInfo| {
                    p.id.as_ref()
                        .map_or_else(|| p.name.clone(), |i| format!("{} ({i})", p.name))
                };
                notes.push(Note {
                    text: format!(
                        "The crown of {} reaches over {}, which stands {:.0} mm up into it",
                        who(a),
                        who(b),
                        top - crown_z
                    ),
                    code: dt_lang::codes::CROWN,
                    help: Some(
                        "Move one of them, or raise the crown's piece; nothing is refused, the real shapes just meet here"
                            .to_owned(),
                    ),
                });
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 2], b: [f64; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
            "{a:?} {b:?}"
        );
    }

    fn at(x: f64, y: f64, w: f64, d: f64, turns: u8) -> Placement {
        Placement {
            x,
            y,
            w,
            d,
            turns,
            z: 0.0,
        }
    }

    /// The page's `matrixOf` (js/map-model.js) says the same: a tile's south-west corner is the
    /// footprint's south-west, a quarter turn clockwise sends the north edge east.
    #[test]
    fn a_piece_stands_where_the_page_draws_it() {
        let p = at(70.0, 35.0, 70.0, 35.0, 0);
        assert_eq!(p.footprint(), [70.0, 35.0, 140.0, 70.0]);
        near(p.on_map(0.0, 0.0), [70.0, 70.0]);
        near(p.on_map(70.0, 35.0), [140.0, 35.0]);
        let q = at(0.0, 0.0, 70.0, 35.0, 1);
        assert_eq!(q.footprint(), [0.0, 0.0, 35.0, 70.0]);
        // the tile's north-west corner goes to the footprint's north-east
        near(q.on_map(0.0, 35.0), [35.0, 0.0]);
        near(q.on_map(0.0, 0.0), [0.0, 0.0]);
        near(q.on_map(70.0, 0.0), [0.0, 70.0]);
        let r = at(0.0, 0.0, 35.0, 35.0, 2);
        near(r.on_map(0.0, 35.0), [35.0, 35.0]);
        // a crown's box turns with the piece: five mm west of a 70 x 35 tile, turned, lies north of it
        let b = q.box_on_map([-5.0, 0.0, 0.0, 35.0]);
        near([b[0], b[1]], [0.0, -5.0]);
        near([b[2], b[3]], [35.0, 0.0]);
    }
}
