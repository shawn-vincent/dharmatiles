//! `scatter { rock …; flower …; tree … }`: one scatter, mixed things.
//!
//! Each placed item picks its child by `weight=` from the scatter's keyed
//! seed (`item`, its index), draws its own size from the same stream and
//! brings its footprint: a rock's radius, a flower's reach, a tree's crown.
//! One `gap` keeps every pair of things apart, whatever their kinds.
//!
//! Placement runs in three passes over one list of discs, largest things
//! first: **trees** (laid out together, largest first, with the relayout
//! retry and a loud error when they do not fit), then **rocks** (clear of
//! the crowns and of rocks already standing, bedded as a rock scatter's
//! are), then **flowers** (dry ground, clear of the crowns and rocks).
//! The big things claim their room and the small ones fill what is left,
//! so a tree is never crowded out by pebbles; each kind is built as it is
//! in a scatter of its own, in this order, so slots and source ids are
//! fixed by index whatever the threads.
//!
//! A scatter of one kind never comes here: it builds as before.

use dt_kit::{Kind, Rng, Scene, Seed};

use crate::flowers::{self, Plant};
use crate::read::Settings;
use crate::rock::{self, Item, Tally, Thing};
use crate::tree::{self, Tree};
use crate::{Child, Control, Ctx, Draw, Element, LookError, Plan, Read, Write};

/// How far apart things keep when a mixed scatter writes no `gap`, mm at
/// the 35 mm square.
const GAP_MM: f64 = 1.0;

/// How many kinds of thing the children hold (rocks, flowers, trees).
#[must_use]
pub fn kinds(children: &[Child]) -> usize {
    let has = |f: &dyn Fn(&str) -> bool| children.iter().any(|c| f(&c.name));
    usize::from(has(&rock::is_thing))
        + usize::from(has(&|n| n == "flower"))
        + usize::from(has(&|n| n == "tree"))
}

/// A child of a mixed scatter.
enum Part {
    Rock(Thing),
    Flower(Plant),
    Tree(Box<Tree>, f64),
}

impl Part {
    fn weight(&self) -> f64 {
        match self {
            Self::Rock(t) => t.weight,
            Self::Flower(p) => p.weight(),
            Self::Tree(_, w) => *w,
        }
    }
}

/// Things of more than one kind, by count or per square.
pub struct Scatter {
    count: Option<Draw>,
    per_square: Draw,
    gap: f64,
    reach: f64,
    parts: Vec<Part>,
}

impl Scatter {
    /// From its controls and its children (each a thing and its weight).
    ///
    /// # Errors
    /// [`LookError`] for a child that is not a rock, flower or tree, or bad
    /// controls.
    pub fn new(controls: &[Control], children: &[Child]) -> Result<Self, LookError> {
        let set = Settings::new(controls);
        let mut parts = Vec::with_capacity(children.len());
        for ch in children {
            parts.push(match ch.name.as_str() {
                n if rock::is_thing(n) => Part::Rock(Thing::new(n, &ch.controls, ch.weight)?),
                "flower" => Part::Flower(Plant::new(&ch.controls, ch.weight)?),
                "tree" => Part::Tree(Box::new(Tree::new(&ch.controls)?), ch.weight),
                other => {
                    return Err(LookError::general(
                        format!("`{other}` can't be scattered"),
                        "a scatter holds rocks (rock, stone, boulder, pebble), flowers and trees",
                    ));
                }
            });
        }
        Ok(Self {
            count: set.dist("count")?,
            per_square: set.dist_or("perSquare", rock::PER_SQUARE)?,
            gap: set.number_or("gap", GAP_MM)?,
            reach: set.number_or("reach", crate::LINE_REACH_MM)?,
            parts,
        })
    }
}

impl Element for Scatter {
    fn plan(&self) -> Plan {
        let has = |f: fn(&Part) -> bool| self.parts.iter().any(f);
        let (r, f, t) = (
            has(|p| matches!(p, Part::Rock(_))),
            has(|p| matches!(p, Part::Flower(_))),
            has(|p| matches!(p, Part::Tree(..))),
        );
        let writes: &'static [Kind] = match (r, f, t) {
            (true, true, true) => &[Kind::Rock, Kind::Flowers, Kind::Tree],
            (true, true, false) => &[Kind::Rock, Kind::Flowers],
            (true, false, true) => &[Kind::Rock, Kind::Tree],
            (false, true, true) => &[Kind::Flowers, Kind::Tree],
            (true, false, false) => &[Kind::Rock],
            (false, true, false) => &[Kind::Flowers],
            _ => &[Kind::Tree],
        };
        Plan {
            writes: Write::Solids(writes),
            reads: vec![Read::Solids(Kind::Terrain)],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        if ctx.against.is_some() {
            let mut e = LookError::general(
                "a mix of things is not gathered at a foot",
                "scatter rocks alone `at=foot(…)`, or scatter the mix `at=` a region or a line",
            );
            e.code = Some("dt::scatter");
            return Err(e);
        }
        let frame = *scene.frame();
        let s = frame.scale();
        let mut rng = Rng::new(ctx.seed.key("scatter"));
        let area = ctx.spread_area(&frame, self.reach);
        let squares = ctx.spread_squares(&frame, &area);
        // a whole number: the ends round and every one is as likely as any
        let n = match &self.count {
            Some(r) => r.int_by(1.0, &mut rng),
            None => self.per_square.int_by(squares, &mut rng),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = n.clamp(0, 10_000) as usize;
        let total: f64 = self.parts.iter().map(Part::weight).sum();

        // each item picks its child and draws its size from its own stream
        let mut rocks: Vec<Item> = Vec::new();
        let mut flowers: Vec<(&Plant, f64)> = Vec::new();
        let mut trees: Vec<(&Tree, Seed)> = Vec::new();
        let mut tree_radii: Vec<f64> = Vec::new();
        for i in 0..n {
            let item = ctx.seed.key("item").index(i as u64);
            let mut r = Rng::new(item);
            let mut u = r.random() * total;
            let part = self
                .parts
                .iter()
                .find(|p| {
                    let hit = u < p.weight();
                    u -= p.weight();
                    hit
                })
                .unwrap_or(&self.parts[self.parts.len() - 1]);
            match part {
                Part::Rock(thing) => {
                    let d = thing.draw(&mut r);
                    let mut it =
                        rock::make_item(&mut r, d, thing.mix, true, thing.buried.hi() > 0.0)?;
                    it.bury = thing.buried.clone();
                    it.top.clone_from(&thing.top);
                    rocks.push(it);
                }
                Part::Flower(plant) => flowers.push((plant, plant.reach(&mut r))),
                Part::Tree(tree, _) => {
                    tree_radii.push(tree.footprint(scene, item)?);
                    trees.push((tree, item));
                }
            }
        }

        // the discs placed so far (x, y, r): what the next kind keeps clear of
        let mut discs: Vec<[f64; 3]> = Vec::new();

        // trees first: the largest things claim their room together
        if !trees.is_empty() {
            let placed = tree::lay_out(scene, &area, &mut rng, &tree_radii, self.gap * s)?;
            discs.extend(placed.iter().map(|p| [p.at[0], p.at[1], p.r]));
            tree::grow_all(scene, ctx, &trees, &placed)?;
        }

        // then rocks, clear of the crowns
        if !rocks.is_empty() {
            let radii: Vec<f64> = rocks.iter().map(|it| it.r * s).collect();
            let placed = rock::place(
                scene,
                ctx,
                &mut rng,
                &radii,
                self.gap * s,
                self.reach,
                &discs,
            );
            discs.extend(placed.iter().map(|p| [p.at[0], p.at[1], p.r]));
            let posed: Vec<(&Item, [f64; 2], f64)> = placed
                .iter()
                .map(|p| (&rocks[p.index], p.at, rng.uniform(0.0, 360.0)))
                .collect();
            let mut t = Tally::default();
            rock::add_items(scene, ctx.path, ctx.seed, &posed, ctx.crowns, &mut t)?;
            rock::note(scene, &t);
        }

        // last the flowers, on dry ground clear of both
        if !flowers.is_empty() {
            let reaches: Vec<f64> = flowers.iter().map(|f| f.1).collect();
            let placed = flowers::place(scene, &area, &mut rng, &reaches, self.gap, &discs)?;
            flowers::stand(scene, ctx, &mut rng, &flowers, &placed)?;
        }
        Ok(())
    }
}
