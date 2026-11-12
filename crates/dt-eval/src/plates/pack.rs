//! The packer: rectangles on beds.
//!
//! Every piece is its real footprint, the top-down box of the whole built
//! piece (a tree's crown over the edge included), in millimetres.  Pieces go
//! large to small, each in the first free spot reading from the front-left
//! corner (low y first, then low x) of the first plate that has one, a new
//! plate opening when none does.  A spot is free when the piece, with the
//! gap around it, lies inside the margin and clear of the other pieces and of
//! the bed's excluded zones.  When a plate is full, its pieces are moved
//! together so the group is centred on the bed, both ways; a bed with an
//! excluded zone in the way takes the largest part of that move that keeps
//! every piece clear of it.
//!
//! Multicolour adds the filaments: a plate may not need more filaments than
//! the printer has slots, and by default (`Group::Filament`) pieces that share
//! their filaments are put together, so a plate needs the fewest and the
//! printer changes the least.  Grouping can cost a plate; [`pack`] says so
//! ([`Packing::grouping_cost`]) so the user can choose size-only packing.
//!
//! Nothing here is random and nothing reads the clock: the same pieces give
//! the same plates.

use dt_doc::map::{Group, Mode};
use dt_kit::Material;

/// What the packer needs to know of the bed and the settings.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The printable area: width (x) by depth (y), mm.
    pub bed: [f64; 2],
    /// Zones nothing may stand on, `[x0, y0, x1, y1]` from the bed's
    /// front-left corner, mm.
    pub excluded: Vec<[f64; 4]>,
    /// Between two pieces, mm.
    pub gap: f64,
    /// Between a piece and the bed's edge, mm.
    pub margin: f64,
    /// May a piece turn a quarter to fit.
    pub turn: bool,
    /// Filaments the printer holds at once.
    pub slots: u32,
    /// Single colour or multicolour.
    pub mode: Mode,
    /// How multicolour pieces are grouped.
    pub group: Group,
}

/// One piece to put on a plate.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// What it is (the planner's index of it).
    pub id: usize,
    /// Its footprint: width (x) by depth (y), mm.
    pub size: [f64; 2],
    /// The filaments it prints in, sorted.
    pub filaments: Vec<Material>,
}

/// Where a piece stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its [`Item::id`].
    pub id: usize,
    /// The left edge of its footprint on the bed, mm.
    pub x: f64,
    /// The front edge of its footprint on the bed, mm.
    pub y: f64,
    /// Turned a quarter (counter-clockwise), so its width is the item's
    /// depth.
    pub turned: bool,
    /// Its width on the bed, mm.
    pub w: f64,
    /// Its depth on the bed, mm.
    pub d: f64,
}

/// One bed's worth.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plate {
    /// The pieces, in the order they were put down.
    pub placed: Vec<Placed>,
    /// The filaments the plate needs, sorted.
    pub filaments: Vec<Material>,
}

/// The plates, and what to tell about them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Packing {
    /// The plates, in order.
    pub plates: Vec<Plate>,
    /// Pieces that fit on no bed (by id): bigger than the printable area.
    pub too_big: Vec<usize>,
    /// Pieces that need more filaments than the printer has (by id).
    pub too_colourful: Vec<usize>,
    /// How many more plates grouping by filament took than packing by size
    /// would have, when it took more.
    pub grouping_cost: Option<usize>,
}

const EPS: f64 = 1e-6;

fn overlaps(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[2] - EPS && b[0] < a[2] - EPS && a[1] < b[3] - EPS && b[1] < a[3] - EPS
}

/// The first free spot on `plate` for a `w` by `d` box, reading from the
/// front-left: where it stands, and whether it had to turn to.
fn place(plate: &Plate, spec: &Spec, w: f64, d: f64) -> Option<Placed> {
    let [bw, bd] = spec.bed;
    let m = spec.margin;
    // the boxes in the way, each already grown by the gap
    let mut busy: Vec<[f64; 4]> = plate
        .placed
        .iter()
        .map(|p| {
            [
                p.x - spec.gap,
                p.y - spec.gap,
                p.x + p.w + spec.gap,
                p.y + p.d + spec.gap,
            ]
        })
        .collect();
    busy.extend(spec.excluded.iter().copied());
    // a piece rests against the margin, a neighbour (plus the gap) or a zone
    let mut xs = vec![m];
    let mut ys = vec![m];
    for b in &busy {
        xs.push(b[2]);
        ys.push(b[3]);
    }
    let order = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < EPS);
    };
    order(&mut xs);
    order(&mut ys);
    let orients: &[(f64, f64, bool)] = if spec.turn && (w - d).abs() > EPS {
        &[(w, d, false), (d, w, true)]
    } else {
        &[(w, d, false)]
    };
    for &y in &ys {
        for &x in &xs {
            for &(ow, od, turned) in orients {
                let r = [x, y, x + ow, y + od];
                if r[2] > bw - m + EPS || r[3] > bd - m + EPS {
                    continue;
                }
                if busy.iter().all(|b| !overlaps(r, *b)) {
                    return Some(Placed {
                        id: 0,
                        x,
                        y,
                        turned,
                        w: ow,
                        d: od,
                    });
                }
            }
        }
    }
    None
}

fn union(a: &[Material], b: &[Material]) -> Vec<Material> {
    let mut v: Vec<Material> = a.iter().chain(b).copied().collect();
    v.sort();
    v.dedup();
    v
}

/// Large to small, a total order: bigger footprint first, then the longer
/// side, then the id.
fn by_size(items: &mut [&Item]) {
    items.sort_by(|a, b| {
        let area = |i: &Item| i.size[0] * i.size[1];
        area(b)
            .total_cmp(&area(a))
            .then(
                b.size[0]
                    .max(b.size[1])
                    .total_cmp(&a.size[0].max(a.size[1])),
            )
            .then(a.id.cmp(&b.id))
    });
}

/// First fit over every plate, for the order given.
fn first_fit(items: &[&Item], spec: &Spec, only_last: bool) -> Packing {
    let single = spec.mode == Mode::Single;
    let mut out = Packing::default();
    for it in items {
        if !single && it.filaments.len() > spec.slots as usize {
            out.too_colourful.push(it.id);
            continue;
        }
        let first = if only_last {
            out.plates.len().saturating_sub(1)
        } else {
            0
        };
        let mut done = false;
        for plate in out.plates.iter_mut().skip(first) {
            let need = union(&plate.filaments, &it.filaments);
            if !single && need.len() > spec.slots as usize {
                continue;
            }
            if let Some(mut p) = place(plate, spec, it.size[0], it.size[1]) {
                p.id = it.id;
                plate.placed.push(p);
                if !single {
                    plate.filaments = need;
                }
                done = true;
                break;
            }
        }
        if done {
            continue;
        }
        let mut fresh = Plate::default();
        if let Some(mut p) = place(&fresh, spec, it.size[0], it.size[1]) {
            p.id = it.id;
            fresh.placed.push(p);
            if !single {
                fresh.filaments.clone_from(&it.filaments);
            }
            out.plates.push(fresh);
        } else {
            out.too_big.push(it.id);
        }
    }
    out
}

/// Put `items` on plates.
///
/// Single colour packs by size alone (one filament, no limit).  Multicolour
/// packs by size too when `group` is [`Group::Size`]; by default it orders
/// the pieces by their filament sets, the sets with the most area first and
/// each set large to small, and puts each piece on the plate being filled or
/// opens a new one.  [`Packing::grouping_cost`] says when that took more
/// plates than packing by size.
#[must_use]
pub fn pack(items: &[Item], spec: &Spec) -> Packing {
    let mut sized: Vec<&Item> = items.iter().collect();
    by_size(&mut sized);
    let mut by_area = first_fit(&sized, spec, false);
    if spec.mode == Mode::Single || spec.group == Group::Size {
        centre(&mut by_area, spec);
        return by_area;
    }
    // each filament set together: the set with the most area first
    let mut sets: Vec<(Vec<Material>, f64)> = Vec::new();
    for it in items {
        let a = it.size[0] * it.size[1];
        match sets.iter_mut().find(|(s, _)| *s == it.filaments) {
            Some((_, t)) => *t += a,
            None => sets.push((it.filaments.clone(), a)),
        }
    }
    sets.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| key(&a.0).cmp(&key(&b.0))));
    let mut ordered: Vec<&Item> = Vec::with_capacity(sized.len());
    for (set, _) in &sets {
        ordered.extend(sized.iter().copied().filter(|i| i.filaments == *set));
    }
    let mut grouped = first_fit(&ordered, spec, true);
    if grouped.plates.len() > by_area.plates.len() {
        grouped.grouping_cost = Some(grouped.plates.len() - by_area.plates.len());
    }
    centre(&mut grouped, spec);
    grouped
}

/// Move each plate's pieces together so the group is centred on the bed.
/// The group already lies inside the margin, so the centred group does too;
/// an excluded zone in the way shortens the move (three quarters, half, a
/// quarter of it, then none), so no piece ever stands on one.
fn centre(packing: &mut Packing, spec: &Spec) {
    let [bw, bd] = spec.bed;
    for plate in &mut packing.plates {
        let Some(first) = plate.placed.first() else {
            continue;
        };
        let mut b = [first.x, first.y, first.x + first.w, first.y + first.d];
        for p in &plate.placed {
            b = [
                b[0].min(p.x),
                b[1].min(p.y),
                b[2].max(p.x + p.w),
                b[3].max(p.y + p.d),
            ];
        }
        let dx = (bw - (b[0] + b[2])) / 2.0;
        let dy = (bd - (b[1] + b[3])) / 2.0;
        let clear = |t: f64| {
            plate.placed.iter().all(|p| {
                let r = [
                    p.x + dx * t,
                    p.y + dy * t,
                    p.x + p.w + dx * t,
                    p.y + p.d + dy * t,
                ];
                spec.excluded.iter().all(|z| !overlaps(r, *z))
            })
        };
        if let Some(t) = [1.0, 0.75, 0.5, 0.25].into_iter().find(|&t| clear(t)) {
            for p in &mut plate.placed {
                p.x += dx * t;
                p.y += dy * t;
            }
        }
    }
}

fn key(s: &[Material]) -> Vec<u8> {
    s.iter().map(|m| *m as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(bed: f64) -> Spec {
        Spec {
            bed: [bed, bed],
            excluded: vec![],
            gap: 3.0,
            margin: 5.0,
            turn: true,
            slots: 4,
            mode: Mode::Single,
            group: Group::Filament,
        }
    }

    fn squares(n: usize, side: f64) -> Vec<Item> {
        (0..n)
            .map(|id| Item {
                id,
                size: [side, side],
                filaments: vec![Material::Earth],
            })
            .collect()
    }

    fn per_plate(p: &Packing) -> Vec<usize> {
        p.plates.iter().map(|p| p.placed.len()).collect()
    }

    /// No two boxes of a plate come nearer than the gap, none leaves the
    /// margin, none lies in a zone.
    fn assert_valid(p: &Packing, s: &Spec) {
        for plate in &p.plates {
            for (i, a) in plate.placed.iter().enumerate() {
                assert!(a.x >= s.margin - EPS && a.y >= s.margin - EPS);
                assert!(a.x + a.w <= s.bed[0] - s.margin + EPS);
                assert!(a.y + a.d <= s.bed[1] - s.margin + EPS);
                for z in &s.excluded {
                    assert!(!overlaps([a.x, a.y, a.x + a.w, a.y + a.d], *z));
                }
                for b in &plate.placed[i + 1..] {
                    let grown = [
                        a.x - s.gap,
                        a.y - s.gap,
                        a.x + a.w + s.gap,
                        a.y + a.d + s.gap,
                    ];
                    assert!(
                        !overlaps(grown, [b.x, b.y, b.x + b.w, b.y + b.d]),
                        "{a:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn thirty_six_squares_fit_the_x1c_bed_and_the_next_one_opens_a_plate() {
        let s = spec(256.0);
        let p = pack(&squares(36, 35.0), &s);
        assert_eq!(per_plate(&p), [36]);
        assert_valid(&p, &s);
        // the thirty-seventh
        let p = pack(&squares(37, 35.0), &s);
        assert_eq!(per_plate(&p), [36, 1]);
        assert_valid(&p, &s);
    }

    #[test]
    fn the_sv06s_220_bed_holds_five_by_five_where_the_x1c_holds_six_by_six() {
        // the page's claim, 35 mm squares with 3 mm between and 5 mm to the edge
        let sv = spec(220.0);
        let p = pack(&squares(36, 35.0), &sv);
        assert_eq!(per_plate(&p), [25, 11]);
        assert_valid(&p, &sv);
        assert_eq!(pack(&squares(36, 35.0), &spec(256.0)).plates.len(), 1);
        // with no gap and no margin the page's 49 squares fit 256 (7 x 35 = 245)
        let bare = Spec {
            gap: 0.0,
            margin: 0.0,
            ..spec(256.0)
        };
        assert_eq!(per_plate(&pack(&squares(49, 35.0), &bare)), [49]);
    }

    #[test]
    fn a_crown_over_the_edge_is_packed_by_its_real_footprint() {
        // 1x1-grass-tree: 35 mm plus 4.1 and 5.2 across, plus 9.4 deep
        let tree = |id| Item {
            id,
            size: [44.3, 44.4],
            filaments: vec![Material::Earth],
        };
        let items: Vec<Item> = (0..26).map(tree).collect();
        let s = spec(256.0);
        let p = pack(&items, &s);
        // five a row: 5 x 44.3 + 4 x 3 = 233.5 inside 246
        assert_eq!(per_plate(&p), [25, 1]);
        assert_valid(&p, &s);
        // the same trees on a grid of squares would have been thirty-six
        assert!(pack(&squares(36, 35.0), &s).plates.len() == 1);
    }

    #[test]
    fn large_to_small_and_the_same_every_time() {
        let items = vec![
            Item {
                id: 0,
                size: [35.0, 35.0],
                filaments: vec![Material::Earth],
            },
            Item {
                id: 1,
                size: [70.0, 70.0],
                filaments: vec![Material::Earth],
            },
            Item {
                id: 2,
                size: [70.0, 35.0],
                filaments: vec![Material::Earth],
            },
            Item {
                id: 3,
                size: [35.0, 35.0],
                filaments: vec![Material::Earth],
            },
        ];
        let s = spec(256.0);
        let a = pack(&items, &s);
        let order: Vec<usize> = a.plates[0].placed.iter().map(|p| p.id).collect();
        assert_eq!(
            order,
            [1, 2, 0, 3],
            "70x70, 70x35, then the two squares in id order"
        );
        assert_eq!(a, pack(&items, &s));
        // the biggest stands at the group's front-left corner, and the group
        // is centred on the bed both ways
        let ps = &a.plates[0].placed;
        let lo = |f: fn(&Placed) -> f64| ps.iter().map(f).fold(f64::MAX, f64::min);
        let hi = |f: fn(&Placed) -> f64| ps.iter().map(f).fold(f64::MIN, f64::max);
        assert_eq!((ps[0].x, ps[0].y), (lo(|p| p.x), lo(|p| p.y)));
        assert!(((lo(|p| p.x) + hi(|p| p.x + p.w)) / 2.0 - 128.0).abs() < 1e-9);
        assert!(((lo(|p| p.y) + hi(|p| p.y + p.d)) / 2.0 - 128.0).abs() < 1e-9);
    }

    #[test]
    fn a_plate_is_centred_unless_a_zone_is_in_the_way() {
        let s = spec(256.0);
        let one = pack(&squares(1, 35.0), &s);
        let p = one.plates[0].placed[0];
        assert_eq!((p.x, p.y), (110.5, 110.5), "one square in the middle");
        // a zone over the bed's middle: the square stays clear of it
        let mut z = spec(256.0);
        z.excluded = vec![[100.0, 100.0, 156.0, 156.0]];
        let held = pack(&squares(1, 35.0), &z);
        assert_valid(&held, &z);
        let q = held.plates[0].placed[0];
        assert!(q.x + q.w <= 100.0 + 1e-9 || q.y + q.d <= 100.0 + 1e-9);
    }

    #[test]
    fn a_quarter_turn_lets_a_long_piece_fill_a_gap() {
        // a bed 50 across and 100 deep (margin 5): a 90 x 40 piece fits only turned
        let s = Spec {
            bed: [50.0, 100.0],
            ..spec(100.0)
        };
        let items = [Item {
            id: 0,
            size: [90.0, 40.0],
            filaments: vec![Material::Earth],
        }];
        let p = pack(&items, &s);
        assert_eq!(per_plate(&p), [1]);
        assert!(p.plates[0].placed[0].turned);
        assert_eq!(
            (p.plates[0].placed[0].w, p.plates[0].placed[0].d),
            (40.0, 90.0)
        );
        let off = Spec { turn: false, ..s };
        let p = pack(&items, &off);
        assert_eq!(p.too_big, [0], "without turns it is bigger than the bed");
    }

    #[test]
    fn nothing_stands_in_an_excluded_zone() {
        // the whole front strip of the bed is excluded: 30 mm deep
        let s = Spec {
            excluded: vec![[0.0, 0.0, 256.0, 30.0]],
            ..spec(256.0)
        };
        let p = pack(&squares(36, 35.0), &s);
        assert_valid(&p, &s);
        assert!(p.plates[0].placed.iter().all(|q| q.y >= 30.0 - EPS));
        // six rows no longer fit: 30 + 6 x 35 + 5 x 3 = 255 > 251
        assert_eq!(per_plate(&p), [30, 6]);
    }

    #[test]
    fn a_piece_bigger_than_the_bed_is_reported() {
        let p = pack(&squares(1, 300.0), &spec(256.0));
        assert!(p.plates.is_empty());
        assert_eq!(p.too_big, [0]);
    }

    fn coloured(id: usize, side: f64, f: &[Material]) -> Item {
        Item {
            id,
            size: [side, side],
            filaments: f.to_vec(),
        }
    }

    const EARTH: Material = Material::Earth;
    const GRASS: Material = Material::Grass;
    const WATER: Material = Material::Water;
    const ROCK: Material = Material::Rock;

    #[test]
    fn multicolour_groups_pieces_that_share_filaments() {
        let mut s = spec(100.0);
        s.mode = Mode::Multicolour;
        s.slots = 2;
        // a 100 mm bed holds 2 x 2 squares of 35 (2 x 35 + 3 = 73 inside 90)
        let mut items = Vec::new();
        for i in 0..4 {
            items.push(coloured(i, 35.0, &[EARTH, GRASS]));
        }
        for i in 4..8 {
            items.push(coloured(i, 35.0, &[EARTH, WATER]));
        }
        let p = pack(&items, &s);
        assert_eq!(per_plate(&p), [4, 4]);
        // equal areas: the sets in material order (water before grass)
        assert_eq!(p.plates[0].filaments, [EARTH, WATER]);
        assert_eq!(p.plates[1].filaments, [EARTH, GRASS]);
        assert!(p.grouping_cost.is_none());
        // by size alone the order is by id, the same plates here
        s.group = Group::Size;
        assert_eq!(per_plate(&pack(&items, &s)), [4, 4]);
    }

    #[test]
    fn a_plate_never_needs_more_filaments_than_the_printer_has() {
        let mut s = spec(256.0);
        s.mode = Mode::Multicolour;
        s.slots = 2;
        let items = vec![
            coloured(0, 35.0, &[EARTH, GRASS]),
            coloured(1, 35.0, &[EARTH, WATER]),
            coloured(2, 35.0, &[EARTH, ROCK]),
        ];
        let p = pack(&items, &s);
        // every pair would need three, so three plates though they all fit on one
        assert_eq!(per_plate(&p), [1, 1, 1]);
        assert!(p.plates.iter().all(|p| p.filaments.len() <= 2));
        // single colour needs one filament, so one plate
        s.mode = Mode::Single;
        assert_eq!(per_plate(&pack(&items, &s)), [3]);
    }

    #[test]
    fn a_piece_that_needs_more_filaments_than_the_printer_has_is_refused() {
        let mut s = spec(256.0);
        s.mode = Mode::Multicolour;
        s.slots = 1;
        let p = pack(&[coloured(0, 35.0, &[EARTH, GRASS])], &s);
        assert!(p.plates.is_empty());
        assert_eq!(p.too_colourful, [0]);
    }

    #[test]
    fn grouping_can_cost_a_plate_and_size_only_packing_saves_it() {
        // a bed one row deep and 90 mm inside its margin: a long piece (50) takes
        // a row alone beside at most one short one (25); three long pieces of one
        // filament set and three short of another
        let mut s = spec(100.0);
        s.bed = [100.0, 45.0];
        s.mode = Mode::Multicolour;
        s.slots = 4;
        let long = |id| Item {
            id,
            size: [50.0, 35.0],
            filaments: vec![EARTH, GRASS],
        };
        let short = |id| Item {
            id,
            size: [25.0, 35.0],
            filaments: vec![EARTH, WATER],
        };
        let items = vec![long(0), long(1), long(2), short(3), short(4), short(5)];
        // grouped, the shorts follow the longs and only the last plate is open to them
        let grouped = pack(&items, &s);
        assert_eq!(per_plate(&grouped), [1, 1, 2, 2]);
        assert_eq!(grouped.grouping_cost, Some(1));
        // by size each short stands beside a long one: three plates
        s.group = Group::Size;
        let by_size = pack(&items, &s);
        assert_eq!(per_plate(&by_size), [2, 2, 2]);
        assert_eq!(by_size.grouping_cost, None);
        assert_valid(&by_size, &s);
        assert_valid(&grouped, &s);
    }
}
