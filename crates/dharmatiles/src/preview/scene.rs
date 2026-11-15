//! A map in the editor: the scene a `.dmap` with placed pieces draws.  Reading a map ([`dt_service::scene_read`]) and a crown
//! over a taller neighbour ([`dt_service::plates_calls::crowns`]) are written once on a host the program's [`Shared`] and a
//! web worker's `Service` both are.

#[cfg(test)]
pub use dt_service::scene::*;

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
