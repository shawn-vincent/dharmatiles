//! Things, and the one scatter that places them.
//!
//! Two kinds of generator build a tile.  A **layer** ([`Element`]) reads the
//! scene so far and changes it: `dirt`, `water`, `grass`, a wall.  A
//! **thing** ([`Thing`]) is made from its own controls and a seed and reads
//! nothing: `rock`, `stone`, `boulder`, `pebble`, `flower`, `tree`.  A
//! `scatter` is a layer that places things, any of them, mixed.
//!
//! **The frame rule.**  A thing is made in its own frame: the origin is
//! where it meets what holds it, +z points away from that surface, and +x
//! is its main axis ([`Thing::make`]).  Anything that places things moves
//! each into place and turns it.
//!
//! **Extent.**  A thing reports the smallest and the largest box it can
//! draw ([`Extent`]), from the ends of its own ranges, in mm at the 35 mm
//! square.  The scatter places the things with the largest extent first, so
//! the big ones claim their room and the small ones fill what is left.
//!
//! **How a thing meets the ground** ([`Meets`]) is data the scatter reads:
//! a rock is bedded (seated into the land, clear of rocks already there), a
//! flower stands on dry ground only, a tree stands upright (a trunk clear of
//! what is stamped, every one of them laid out or the build says so).  The
//! scatter matches on none of it by name.
//!
//! One scatter holds a list of things with weights.  Each item draws which
//! thing it is, and its size, from its own keyed seed; the things that meet
//! the ground alike are placed together (one dart-throw), the group with the
//! largest extent first, each group clear of the discs the groups before it
//! placed; then each group is stood into the scene in its own batch, so
//! slots and source ids are fixed by index whatever the threads.

#![expect(
    clippy::cast_possible_truncation,
    reason = "lattice nodes (a position over the step, clamped into the area) and a clearance radius in cells"
)]

use std::any::{Any, TypeId};
use std::sync::{Mutex, OnceLock};

use dt_kit::{Falls, Field, Kind, Mask, Placed, Rng, Scene, Scope, Seed, Solid, Spread, scatter};

use crate::flowers::Plant;
use crate::read::Settings;
use crate::rock::Rock;
use crate::tree::Tree;
use crate::{
    Child, Control, Ctx, Draw, Element, LookError, Plan, Read, Shown, Write, draw_text, num_text,
};

/// A box in a thing's frame, mm at the 35 mm square: its length along +x
/// (its main axis), across it along y, and up along +z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    /// Along the main axis.
    pub x: f64,
    /// Across it.
    pub y: f64,
    /// Up from where it meets what holds it.
    pub z: f64,
}

impl Box3 {
    /// A box `x` by `y` by `z`.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// The room a thing takes: the smallest and the largest box it can draw,
/// from its own range ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    /// The smallest box.
    pub least: Box3,
    /// The largest box.
    pub most: Box3,
}

impl Extent {
    /// From the two boxes.
    #[must_use]
    pub const fn new(least: Box3, most: Box3) -> Self {
        Self { least, most }
    }

    /// The farthest the largest box reaches from the origin across the
    /// ground, mm at the 35 mm square: what placement spaces by.
    #[must_use]
    pub fn most_reach(&self) -> f64 {
        0.5 * self.most.x.max(self.most.y)
    }
}

/// A thing that stands upright: its trunk is stamped on the ground, so it
/// keeps clear of what is stamped and every one of them must stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Upright {
    /// How far its middle keeps from what is stamped, mm at the 35 mm square.
    pub clear_mm: f64,
    /// How many layouts are tried before the build says they do not fit.
    pub layouts: u32,
}

/// How a thing meets the ground: what a placer reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meets {
    /// Bedded (or buried): seated into the land by its own weight, clear of
    /// the rocks already standing (rocks).
    pub bedded: bool,
    /// Only on dry ground, off what stands over it (flowers).
    pub dry: bool,
    /// Standing upright on a stamped trunk (trees).
    pub upright: Option<Upright>,
    /// Darts it gets in one layout.
    pub tries: u32,
    /// How far it keeps inside the tile's edge, mm at the 35 mm square.
    pub rim_mm: f64,
}

/// What a scatter of this thing uses where the document writes nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Usual {
    /// How far apart neighbours keep, mm at the 35 mm square.
    pub gap_mm: f64,
    /// How many to a square.
    pub per_square: Draw,
    /// How far either side of a line it spreads, mm at the 35 mm square;
    /// `None` is a line's or a foot's own.
    pub reach_mm: Option<f64>,
    /// The keyed stream a scatter of it draws its count and its places from.
    /// Flowers keep the name their first tiles were drawn under (`flowers`):
    /// it names every flower tile's places, and the printability baseline
    /// holds their repair counts to them, so it moves with a rewrite of
    /// both.  A scatter of things that agree on a stream uses it; a mix
    /// uses [`SCATTER_STREAM`].
    pub stream: &'static str,
}

/// The keyed stream a scatter draws from, unless its things all name another.
pub const SCATTER_STREAM: &str = "scatter";

/// One draw of a thing: the room it asks for and what its own stand reads
/// back.  What is inside is the thing's own; the scatter reads only the
/// radius.
pub struct Drawn {
    radius_mm: f64,
    payload: Box<dyn Any + Send + Sync>,
}

impl Drawn {
    /// A draw that takes a disc of `radius_mm` (world mm), carrying `payload`.
    #[must_use]
    pub fn new<T: Any + Send + Sync>(radius_mm: f64, payload: T) -> Self {
        Self {
            radius_mm,
            payload: Box::new(payload),
        }
    }

    /// The disc's radius, world mm.
    #[must_use]
    pub fn radius_mm(&self) -> f64 {
        self.radius_mm
    }

    /// What the thing carried, as its own type.
    ///
    /// # Errors
    /// [`LookError`] when it is another thing's (a bug: the scatter hands a
    /// batch only to the thing that drew it).
    pub fn payload<T: Any>(&self) -> Result<&T, LookError> {
        self.payload.downcast_ref::<T>().ok_or_else(|| {
            LookError::build("a drawn thing was handed to a thing of another family")
        })
    }

    /// Which family drew it: draws of one family are built in one batch.
    fn family(&self) -> TypeId {
        (*self.payload).type_id()
    }
}

/// A draw set down: where, and the disc it was given.
pub struct Standing<'a> {
    /// The draw.
    pub drawn: &'a Drawn,
    /// Where its origin stands, world mm.
    pub at: [f64; 2],
    /// Its disc's radius, world mm.
    pub r: f64,
    /// How many times taller than drawn it stands (1 where nothing near it
    /// asks for more; only a thing that [`Thing::grows_tall`] is handed
    /// another).
    pub tall: f64,
}

/// A thing: made from its controls and a seed, in its own frame.
pub trait Thing: Send + Sync {
    /// The smallest and largest box it can draw.
    fn extent(&self) -> Extent;

    /// How it meets the ground.
    fn meets(&self) -> Meets;

    /// What a scatter of it uses where nothing is written.
    fn usual(&self) -> Usual;

    /// The kind of solid it adds to the scene.
    fn writes(&self) -> Kind;

    /// What a scatter of it reads before it can build.
    fn reads(&self) -> &'static [Read] {
        &[Read::Solids(Kind::Terrain)]
    }

    /// Its things, plural, as an author says them (for messages).
    fn plural(&self) -> &'static str;

    /// One draw for the item whose own seed is `item` and whose own stream is
    /// `rng`: its size, and the room it asks for.  `at_foot` when the
    /// scatter gathers at a foot.
    ///
    /// # Errors
    /// [`LookError`] when a draw cannot be made.
    fn draw(
        &self,
        scene: &Scene,
        item: Seed,
        rng: &mut Rng,
        at_foot: bool,
    ) -> Result<Drawn, LookError>;

    /// One of it alone, in its own frame: the origin where it meets what
    /// holds it, +z away from that surface.  The solids are not added to the
    /// scene.
    ///
    /// # Errors
    /// [`LookError`] when it cannot be built.
    fn make(
        &self,
        scene: &mut Scene,
        seed: Seed,
        printer: &dt_kit::Printer,
    ) -> Result<Vec<Solid>, LookError>;

    /// Set down every draw of this family at once, in the order given: each
    /// moved into place, seated on the ground and added to the scene.  `rng`
    /// is the scatter's stream, which the thing draws its turns from.
    ///
    /// # Errors
    /// [`LookError`] when one cannot be built.
    fn stand(
        &self,
        scene: &mut Scene,
        ctx: &Ctx<'_>,
        rng: &mut Rng,
        placed: &[Standing<'_>],
    ) -> Result<(), LookError>;

    /// Whether it can be stood taller than it was drawn ([`Standing::tall`]),
    /// as a scatter's `near` asks.  Stretching only up can never add an
    /// overhang.
    fn grows_tall(&self) -> bool {
        false
    }

    /// Whether it can be gathered at a foot (`at=foot(…)`).
    fn gathers(&self) -> bool {
        false
    }

    /// Where draws lie when gathered at `foot`, `reach` mm out if the
    /// scatter says.
    ///
    /// # Errors
    /// [`LookError`] when it cannot be gathered.
    fn gather(
        &self,
        _scene: &Scene,
        _ctx: &Ctx<'_>,
        _rng: &mut Rng,
        _foot: &crate::Foot,
        _reach: Option<f64>,
        _drawn: &[&Drawn],
    ) -> Result<Vec<Placed>, LookError> {
        Err(LookError::build(format!(
            "{} are not gathered at a foot",
            self.plural()
        )))
    }

    /// The refusal when only `fit` of `of` could be laid out.  Only a thing
    /// that must stand upright is asked.
    fn no_room(&self, fit: usize, of: usize) -> LookError {
        LookError::general(
            format!("room for {fit} of the {of} {}", self.plural()),
            "ask for fewer, or make `gap` smaller",
        )
    }
}

type Builder = fn(&str, &[Control]) -> Result<Box<dyn Thing>, LookError>;

/// Every thing a scatter may hold: the word as written, and how it is built.
static THINGS: &[(&str, Builder)] = &[
    ("rock", |n, c| Ok(Box::new(Rock::new(n, c)?))),
    ("stone", |n, c| Ok(Box::new(Rock::new(n, c)?))),
    ("boulder", |n, c| Ok(Box::new(Rock::new(n, c)?))),
    ("pebble", |n, c| Ok(Box::new(Rock::new(n, c)?))),
    ("flower", |_, c| Ok(Box::new(Plant::new(c)?))),
    ("tree", |_, c| Ok(Box::new(Tree::new(c)?))),
];

/// The thing `name` with its controls.
///
/// # Errors
/// [`LookError`] for a word that is no thing, or controls it refuses.
pub fn thing(name: &str, controls: &[Control]) -> Result<Box<dyn Thing>, LookError> {
    match THINGS.iter().find(|(n, _)| *n == name) {
        Some((_, build)) => build(name, controls),
        None => Err(LookError::general(
            format!("`{name}` can't be scattered"),
            "a scatter holds rocks (rock, stone, boulder, pebble), flowers and trees",
        )),
    }
}

/// The entry's area less the core of a cliff's stones: nothing is set down
/// on it (it lies behind the stones, and what stood on it would be loose).
pub(crate) fn open_area(scene: &Scene, ctx: &Ctx<'_>, reach: f64) -> Mask {
    // at= a line: the band either side of it
    let mut area = ctx.spread_area(scene.frame(), reach);
    if let Some(rest) = scene.rest() {
        for (a, c) in area.bits.iter_mut().zip(&rest.core.bits) {
            *a &= !*c;
        }
    }
    area
}

/// Discs (x, y, r) round every rock already in the scene.
pub(crate) fn rocks_in(scene: &Scene) -> Vec<[f64; 3]> {
    scene
        .stands(&[Kind::Rock])
        .iter()
        .map(|s| [s.at[0], s.at[1], s.r])
        .collect()
}

/// What a dry thing keeps off: the rocks, trees and walls already in the scene.
const BLOCKERS: &[Kind] = &[Kind::Rock, Kind::Tree, Kind::Wall];

/// What a dry thing's darts are checked against, read from the scene once.
struct Dry {
    ground: Field,
    blocker: Mask,
    wet: Mask,
    widest: f64,
}

/// Discs to place, and how: the radii (world mm), how their thing meets the
/// ground, `gap` (world mm) between them, and the `avoid` discs (x, y, r)
/// of what is already placed.
pub(crate) struct Sites<'a> {
    /// Each disc's radius, world mm.
    pub radii: &'a [f64],
    /// How far apart neighbours keep, world mm.
    pub gap: f64,
    /// How their thing meets the ground.
    pub meets: Meets,
    /// Their thing, plural, for messages.
    pub plural: &'a str,
    /// Discs kept clear of.
    pub avoid: &'a [[f64; 3]],
    /// Where darts are kept ([`PLAIN`] for one thing placed alone).
    pub shape: &'a Shape,
}

/// One dart-throw of `sites` into `area`: largest first, each `gap` clear
/// of the others and of the discs to avoid.
fn one_layout(
    scene: &Scene,
    area: &Mask,
    rng: &mut Rng,
    sites: &Sites<'_>,
    outline: Option<&Field>,
) -> Result<Vec<Placed>, LookError> {
    let Sites {
        radii,
        gap,
        meets,
        plural,
        avoid,
        shape,
    } = *sites;
    let frame = *scene.frame();
    let s = frame.scale();
    let [w, h] = frame.size_mm();
    let step = frame.step_mm();
    let dry = if meets.dry {
        let Some((surface, _)) = scene.surface() else {
            return Err(LookError::general(
                format!("{plural} have no ground to grow on"),
                "put a `ground` or `dirt` before them in the stack",
            ));
        };
        Some(Dry {
            ground: Field::from(&surface),
            blocker: scene.standing(BLOCKERS, NEAR_ABOVE_MM),
            wet: scene.wet().clone(),
            widest: radii.iter().copied().fold(0.0, f64::max),
        })
    } else {
        None
    };
    // an upright thing keeps clear of what is stamped
    let clear = meets.upright.map(|u| u.clear_mm * s);
    let support = scene.support();
    let rr = clear.map_or(0, |c| libm::ceil(c / step) as usize);
    let (nx, ny) = (area.nx, area.ny);
    // where a thing could stand at all: in its area, dry, clear of what is
    // stamped; a dart elsewhere is no place
    let fits = |x: f64, y: f64| -> bool {
        if let Some(d) = &dry {
            // the whole disc must be dry, in its area, off what stands there
            let ok = (0..8).all(|k| {
                let a = std::f64::consts::TAU * f64::from(k) / 8.0;
                let (px, py) = (x + d.widest * libm::cos(a), y + d.widest * libm::sin(a));
                let (i, j) = d.ground.node(px.clamp(0.0, w), py.clamp(0.0, h));
                let q = j * nx + i;
                area.bits[q] && !d.wet.bits[q] && (outline.is_some() || !d.blocker.bits[q])
            });
            let (i, j) = d.ground.node(x, y);
            return ok && area.bits[j * nx + i];
        }
        let (i, j) = (
            (libm::round(x / step).max(0.0) as usize).min(nx - 1),
            (libm::round(y / step).max(0.0) as usize).min(ny - 1),
        );
        if !area.bits[j * nx + i] {
            return false;
        }
        if let (Some(c), Some(sup)) = (clear, support) {
            for jj in j.saturating_sub(rr)..=(j + rr).min(ny - 1) {
                for ii in i.saturating_sub(rr)..=(i + rr).min(nx - 1) {
                    let d =
                        libm::hypot((ii as f64 - i as f64) * step, (jj as f64 - j as f64) * step);
                    if d <= c && sup.z[jj * nx + ii].is_finite() {
                        return false;
                    }
                }
            }
        }
        true
    };
    // how likely a place is kept: the thinning, which leaves a thing out
    let density = |x: f64, y: f64| shape.weight(x, y);
    let spread = Spread {
        tries: shape.tries(meets.tries),
        inset: [meets.rim_mm * s, 1.0],
        confine: true,
        gap,
        factor: 1.0,
        falls: shape.falls(),
    };
    let scope = Scope {
        density: &density,
        fits: &fits,
        outline,
    };
    Ok(scatter(&frame, rng, radii, &spread, &scope, avoid))
}

/// Dart-throw discs of `radii` (world mm) into the entry's area by how
/// their thing meets the ground, largest first, `gap` (world mm) clear of
/// one another, of the rocks standing when they are bedded, and of the
/// `also` discs (x, y, r) of what is already placed.  A thing that stands
/// upright is laid out again (the next draws of the same stream) until every
/// one has room, up to its layouts; the best layout is returned, and the
/// caller says when it is short.
///
/// # Errors
/// [`LookError`] for a dry thing with no ground to stand on.
pub(crate) fn place_group(
    scene: &Scene,
    ctx: &Ctx<'_>,
    rng: &mut Rng,
    reach: f64,
    sites: &Sites<'_>,
) -> Result<Vec<Placed>, LookError> {
    let area = open_area(scene, ctx, reach);
    // what a thing keeps clear of (a bedded one the rocks, a dry one the rocks,
    // trees and walls): a spaced or clumped pattern keeps the disc of each (its
    // bounds, and a dry thing's ring), a cover the outline of each, the footprint
    // `near` measures from, so a cover lies against them on every side and leaves
    // no halo on a short one
    let cover = sites.shape.pattern == Pattern::Cover;
    let kinds: Vec<Kind> = Kind::ALL
        .iter()
        .copied()
        .filter(|k| {
            (sites.meets.bedded && *k == Kind::Rock) || (sites.meets.dry && BLOCKERS.contains(k))
        })
        .collect();
    let outline = (cover && !kinds.is_empty()).then(|| footprint_distance(scene, &kinds));
    let mut avoid = if sites.meets.bedded && !cover {
        rocks_in(scene)
    } else {
        Vec::new()
    };
    avoid.extend_from_slice(sites.avoid);
    let sites = Sites {
        avoid: &avoid,
        ..*sites
    };
    let layouts = sites.meets.upright.map_or(1, |u| u.layouts);
    let mut placed = Vec::new();
    for _ in 0..layouts {
        let layout = one_layout(scene, &area, rng, &sites, outline.as_ref())?;
        if layout.len() > placed.len() {
            placed = layout;
        }
        if placed.len() == sites.radii.len() {
            break;
        }
    }
    Ok(placed)
}

/// How a scatter's places fall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// Kept apart, the room each takes between them.
    Spaced,
    /// Gathered in clumps, kept apart within each.
    Clumped,
    /// A dense cover: nothing is checked among them.
    Cover,
}

impl Pattern {
    /// The room a thing keeps from what already stands when `gap` is not
    /// written, `thing_mm` being its own: a cover's things lie against what
    /// stands (touching it, never in it), so the rule for how near they come
    /// is one: `gap`, which is 0 under a cover and the thing's own otherwise.
    fn usual_gap(self, thing_mm: f64) -> f64 {
        if self == Self::Cover { 0.0 } else { thing_mm }
    }

    fn from_word(w: &str) -> Result<Self, LookError> {
        match w {
            "spaced" => Ok(Self::Spaced),
            "clumped" => Ok(Self::Clumped),
            "cover" => Ok(Self::Cover),
            other => Err(LookError::build(format!("no pattern `{other}`"))),
        }
    }
}

/// How many more darts a thing gets in a clumped pattern: a dart lands in a
/// clump a fifth of the time or so.
const CLUMP_TRIES: u32 = 8;

/// Where darts are kept: the one place the pattern, the clearings and the
/// cover are decided, read by the one dart-throw ([`one_layout`]) whatever
/// the pattern.  Discs are (x, y, r), world mm.
#[derive(Clone, Debug)]
pub struct Shape {
    /// How the places fall.
    pub pattern: Pattern,
    /// A clumped pattern's clumps, each with the reach it drew.
    pub clumps: Vec<[f64; 3]>,
    /// Round patches left bare.
    pub clearings: Vec<[f64; 3]>,
    /// The share of darts that keep a thing.
    pub cover: f64,
}

/// Spaced, no clearings, all the cover: what a thing placed alone is placed
/// by, and a scatter that writes nothing of this.
pub static PLAIN: Shape = Shape {
    pattern: Pattern::Spaced,
    clumps: Vec::new(),
    clearings: Vec::new(),
    cover: 1.0,
};

impl Shape {
    /// How likely a dart at (x, y) is kept, 0 to 1: none in a clearing, the
    /// cover's share elsewhere, and in a clumped pattern fading from a
    /// clump's middle to nothing at its reach.
    #[must_use]
    pub fn weight(&self, x: f64, y: f64) -> f64 {
        if self
            .clearings
            .iter()
            .any(|c| libm::hypot(c[0] - x, c[1] - y) < c[2])
        {
            return 0.0;
        }
        let clump = if self.pattern == Pattern::Clumped {
            self.clumps
                .iter()
                .map(|c| {
                    let f = libm::hypot(c[0] - x, c[1] - y) / c[2];
                    (1.0 - f * f).max(0.0)
                })
                .fold(0.0, f64::max)
        } else {
            1.0
        };
        clump * self.cover
    }

    /// Darts each thing gets, where its thing's own is `base`.
    #[must_use]
    pub fn tries(&self, base: u32) -> u32 {
        match self.pattern {
            Pattern::Spaced => base,
            Pattern::Clumped => base * CLUMP_TRIES,
            // a dart where nothing could stand (off the area, in a rock) is
            // thrown again; one in a clearing, or where the cover is thin, is
            // a thing that is not there (and is not thrown again)
            Pattern::Cover => base,
        }
    }

    /// How the things fall: a cover's evenly and unchecked among them, any
    /// other pattern's at random and kept apart.
    #[must_use]
    pub fn falls(&self) -> Falls {
        match self.pattern {
            Pattern::Cover => Falls::Even,
            Pattern::Spaced | Pattern::Clumped => Falls::Apart,
        }
    }
}

/// What a scatter's things answer to: a `near` block.
#[derive(Clone, Debug)]
struct Near {
    /// The kinds of solid in the scene (none: the water's edge).
    kinds: Option<&'static [Kind]>,
    /// How far out it changes a thing, drawn for each, reference mm.
    reach: Draw,
    /// How many times taller it stands.
    height: Draw,
    /// The share of things within reach left out.
    thin: f64,
}

/// What stood in the scene before a scatter, as distances from it.
struct Nearness {
    rule: Near,
    dist: Field,
    scale: f64,
}

/// Rocks, walls, trunks and cliffs: `near of=standing`.
const STANDING: &[Kind] = &[Kind::Rock, Kind::Wall, Kind::Tree, Kind::Cliff];

/// How far up a solid its footprint is read, mm (what `standing` reads).
const NEAR_ABOVE_MM: f64 = 0.45;

/// How far (mm) every point is from the **footprint** of the solids of
/// `kinds` already in the scene: the one answer to "where is the rock", read
/// by `near` and by a cover's keep-clear alike.  A solid's footprint is its
/// slice [`NEAR_ABOVE_MM`] over its lowest point (where it meets the ground,
/// not its bounds).
fn footprint_distance(scene: &Scene, kinds: &[Kind]) -> Field {
    let mask = scene.standing(kinds, NEAR_ABOVE_MM);
    dt_kit::distance(scene.frame(), &mask.bits).0
}

impl Nearness {
    fn measure(rule: &Near, scene: &Scene) -> Self {
        let dist = match rule.kinds {
            Some(kinds) => footprint_distance(scene, kinds),
            None => dt_kit::distance(scene.frame(), &scene.wet().bits).0,
        };
        Self {
            rule: rule.clone(),
            dist,
            scale: scene.frame().scale(),
        }
    }

    /// What the item whose own seed is `item` does where it stands at `at`
    /// (world mm): `None` if it is left out, else how many times taller it
    /// stands.  Draws the same turns of the item's `near` stream whether or
    /// not it is near.
    fn answer(&self, item: Seed, at: [f64; 2]) -> Option<f64> {
        let mut r = Rng::new(item.key("near"));
        let reach = self.rule.reach.draw(&mut r) * self.scale;
        let tall = self.rule.height.draw(&mut r);
        let left_out = r.random() < self.rule.thin;
        if self.dist.sample(at[0], at[1]) >= reach {
            return Some(1.0);
        }
        (!left_out).then_some(tall)
    }
}

/// The slice of kinds `kinds` (in [`Kind::ALL`] order) as `Write::Solids`
/// holds it: one slice for each distinct set, made once and kept (a build
/// has a handful of distinct sets in all).
fn kinds_slice(kinds: &[Kind]) -> &'static [Kind] {
    static SETS: OnceLock<Mutex<Vec<&'static [Kind]>>> = OnceLock::new();
    // a poisoned lock holds a list that is still whole: only a push panics
    let mut sets = match SETS.get_or_init(|| Mutex::new(Vec::new())).lock() {
        Ok(sets) => sets,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(have) = sets.iter().find(|s| **s == kinds) {
        return have;
    }
    let made: &'static [Kind] = Box::leak(kinds.to_vec().into_boxed_slice());
    sets.push(made);
    made
}

/// `scatter { … }`: many things, by count or per square, each child with
/// its `weight=`.
pub struct Scatter {
    count: Option<Draw>,
    per_square: Draw,
    gap: f64,
    /// How far either side of a line (or out from a foot) it spreads; `None`
    /// is a line's or a foot's own.
    reach: Option<f64>,
    /// The keyed stream it draws from.
    stream: &'static str,
    /// How the places fall.
    pattern: Pattern,
    /// Clumps to a square, and how far each reaches (reference mm).
    clumps: Draw,
    spread: Draw,
    /// The share of darts that keep a thing, and the clearings: to a
    /// square, and how wide each is (reference mm).
    cover: f64,
    clearings: Draw,
    clearing_width: Draw,
    /// What the things answer to.
    near: Option<Near>,
    children: Vec<(Box<dyn Thing>, f64)>,
}

/// The things a `near of=…` word stands for.
fn near_kinds(word: &str) -> Result<Option<&'static [Kind]>, LookError> {
    Ok(match word {
        "rock" => Some(&[Kind::Rock]),
        "wall" => Some(&[Kind::Wall]),
        "tree" => Some(&[Kind::Tree]),
        "cliff" => Some(&[Kind::Cliff]),
        "standing" => Some(STANDING),
        "water" => None,
        other => {
            return Err(LookError::build(format!("`near` has no kind `{other}`")));
        }
    })
}

impl Scatter {
    /// From its controls and its children (each a thing and its weight).
    /// What it uses where nothing is written is the first child's.
    ///
    /// # Errors
    /// [`LookError`] for no child, a child that is no thing, or bad controls.
    pub fn new(controls: &[Control], children: &[Child]) -> Result<Self, LookError> {
        let mut made: Vec<(Box<dyn Thing>, f64)> = Vec::with_capacity(children.len());
        for ch in children {
            made.push((thing(&ch.name, &ch.controls)?, ch.weight));
        }
        let Some((first, _)) = made.first() else {
            return Err(LookError::general(
                "a scatter needs a child to say what it scatters",
                "write `scatter perSquare=2 { rock }`",
            ));
        };
        let usual = first.usual();
        let stream = if made.iter().all(|c| c.0.usual().stream == usual.stream) {
            usual.stream
        } else {
            SCATTER_STREAM
        };
        let set = Settings::new(controls);
        let pattern = Pattern::from_word(set.word("pattern")?.unwrap_or("spaced"))?;
        let near = match set.word("near/of")? {
            Some(word) => Some(Near {
                kinds: near_kinds(word)?,
                reach: set.dist_or("near/reach", Draw::flat(6.0, 12.0))?,
                height: set.dist_or("near/height", Draw::flat(1.0, 1.0))?,
                thin: set.number_or("near/thin", 0.0)?,
            }),
            None => None,
        };
        if let Some(n) = &near
            && ((n.height.lo() - 1.0).abs() > f64::EPSILON
                || (n.height.hi() - 1.0).abs() > f64::EPSILON)
            && let Some(c) = made.iter().find(|c| !c.0.grows_tall())
        {
            return Err(LookError {
                control: Some("near/height".to_owned()),
                ..LookError::general(
                    format!("{} do not grow taller", c.0.plural()),
                    "leave `height` out of `near`, or thin them with `thin=`",
                )
            });
        }
        Ok(Self {
            pattern,
            clumps: set.dist_or("clumps", Draw::flat(3.0, 3.0))?,
            spread: set.dist_or("spread", Draw::flat(3.0, 8.0))?,
            cover: set.number_or("cover", 1.0)?,
            clearings: set.dist_or("clearings", Draw::flat(0.0, 0.0))?,
            clearing_width: set.dist_or("clearingWidth", Draw::flat(6.0, 14.0))?,
            near,
            count: set.dist("count")?,
            per_square: set.dist_or("perSquare", usual.per_square)?,
            gap: set.number_or("gap", pattern.usual_gap(usual.gap_mm))?,
            reach: set.number("reach")?.or(usual.reach_mm),
            stream,
            children: made,
        })
    }

    /// Whether it asks for anything the dart-throw or the stand does not do
    /// at a foot.
    fn shaped(&self) -> bool {
        self.pattern != Pattern::Spaced
            || self.cover < 1.0
            || self.clearings.hi() > 0.0
            || self.near.is_some()
    }

    /// The places' shape, from keyed streams of their own (the scatter's
    /// stream is not touched): clumps and clearings drawn over `area`,
    /// which holds `squares` squares.
    fn shape(&self, scene: &Scene, area: &Mask, squares: f64, seed: Seed) -> Shape {
        let frame = scene.frame();
        let s = frame.scale();
        let [w, h] = frame.size_mm();
        let step = frame.step_mm();
        // a disc's middle somewhere in the area (a try in a hundred that
        // finds none leaves the disc out: nothing could stand there)
        let middle = |rng: &mut Rng| {
            (0..100).find_map(|_| {
                let (x, y) = (rng.uniform(0.0, w), rng.uniform(0.0, h));
                let i = (libm::round(x / step) as usize).min(area.nx - 1);
                let j = (libm::round(y / step) as usize).min(area.ny - 1);
                area.bits[j * area.nx + i].then_some((x, y))
            })
        };
        let discs = |key: &str, n: i64, reach: &Draw, half: f64| {
            let mut rng = Rng::new(seed.key(key));
            (0..n.clamp(0, 1_000))
                .filter_map(|_| {
                    let (x, y) = middle(&mut rng)?;
                    Some([x, y, reach.draw(&mut rng) * half * s])
                })
                .collect::<Vec<_>>()
        };
        let clumps = if self.pattern == Pattern::Clumped {
            let n = self
                .clumps
                .int_by(squares, &mut Rng::new(seed.key("clump count")))
                .max(1);
            discs("clumps", n, &self.spread, 1.0)
        } else {
            Vec::new()
        };
        let n = self
            .clearings
            .int_by(squares, &mut Rng::new(seed.key("clearing count")));
        Shape {
            pattern: self.pattern,
            clumps,
            clearings: discs("clearings", n, &self.clearing_width, 0.5),
            cover: self.cover,
        }
    }

    /// The refusal when something that cannot be gathered at a foot is
    /// scattered there.
    fn not_gathered(&self) -> Option<LookError> {
        let blocked = self.children.iter().find(|c| !c.0.gathers())?;
        let plural = blocked.0.plural();
        let alike = self.children.iter().all(|c| c.0.plural() == plural);
        Some(if alike {
            LookError::general(
                format!("{plural} are not gathered at a foot"),
                format!("scatter {plural} `at=` a region or a line"),
            )
        } else {
            let mut e = LookError::general(
                "a mix of things is not gathered at a foot",
                "scatter rocks alone `at=foot(…)`, or scatter the mix `at=` a region or a line",
            );
            e.code = Some(dt_lang::codes::SCATTER);
            e
        })
    }
}

impl Element for Scatter {
    fn shown(&self) -> Vec<Shown> {
        let mut out = vec![
            Shown::new("perSquare", draw_text(&self.per_square, "", 1.0), "default"),
            Shown::new("gap", format!("{}dmm", num_text(self.gap)), "default"),
        ];
        if let Some(r) = self.reach {
            out.push(Shown::new(
                "reach",
                format!("{}dmm", num_text(r)),
                "default",
            ));
        }
        out
    }

    fn plan(&self) -> Plan {
        let mut kinds: Vec<Kind> = self.children.iter().map(|c| c.0.writes()).collect();
        kinds.sort_by_key(|k| Kind::ALL.iter().position(|a| a == k));
        kinds.dedup();
        let mut reads: Vec<Read> = Vec::new();
        for r in self.children.iter().flat_map(|c| c.0.reads()) {
            if !reads.contains(r) {
                reads.push(*r);
            }
        }
        Plan {
            writes: match kinds.as_slice() {
                [one] => Write::Solid(*one),
                many => Write::Solids(kinds_slice(many)),
            },
            reads,
            cell_mm: None,
        }
    }

    #[expect(
        clippy::cast_sign_loss,
        reason = "a count of things clamped to 0..=10 000 on the line that makes it"
    )]
    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        // gathered against a foot (scree), or spread over the area
        let foot = match ctx.against {
            Some(kind) => {
                if let Some(e) = self.not_gathered() {
                    return Err(e);
                }
                Some(ctx.foot(kind).ok_or_else(|| {
                    LookError::general(
                        format!("nothing in this tile to gather against: no `{kind}`"),
                        format!("add a `{kind}` to the stack, or remove `at=foot(…)`"),
                    )
                })?)
            }
            None => None,
        };
        if foot.is_some() && self.shaped() {
            return Err(LookError::general(
                "a scatter at a foot gathers its own way: it takes no `pattern`, `cover`, `clearings` or `near`",
                "scatter `at=` a region or a line to shape the places",
            ));
        }
        let frame = *scene.frame();
        let s = frame.scale();
        let mut rng = Rng::new(ctx.seed.key(self.stream));
        let line_reach = self.reach.unwrap_or(crate::LINE_REACH_MM);
        // squares the area covers
        let area = ctx.spread_area(&frame, line_reach);
        let squares = foot.map_or_else(|| ctx.spread_squares(&frame, &area), |f| f.squares);
        let shape = self.shape(scene, &area, squares, ctx.seed.key("pattern"));
        let nearness = self.near.as_ref().map(|n| Nearness::measure(n, scene));
        // a whole number: the ends round and every one is as likely as any
        let n = match &self.count {
            Some(r) => r.int_by(1.0, &mut rng),
            None => self.per_square.int_by(squares, &mut rng),
        };
        let n = n.clamp(0, 10_000) as usize;
        let total: f64 = self.children.iter().map(|c| c.1).sum();

        // each item picks its child and draws its size from its own stream
        let mut drawn: Vec<(usize, Drawn)> = Vec::with_capacity(n);
        for i in 0..n {
            let item = ctx.seed.key("item").index(i as u64);
            let mut r = Rng::new(item);
            let mut u = r.random() * total;
            let k = self
                .children
                .iter()
                .position(|c| {
                    let hit = u < c.1;
                    u -= c.1;
                    hit
                })
                .unwrap_or(self.children.len() - 1);
            drawn.push((
                k,
                self.children[k]
                    .0
                    .draw(scene, item, &mut r, foot.is_some())?,
            ));
        }

        // the things that are built together: one family each, the one whose
        // largest box reaches farthest first
        struct Group {
            family: TypeId,
            leader: usize,
            reach: f64,
            items: Vec<usize>,
        }
        let mut groups: Vec<Group> = Vec::new();
        for (i, (k, d)) in drawn.iter().enumerate() {
            let reach = self.children[*k].0.extent().most_reach();
            match groups.iter_mut().find(|g| g.family == d.family()) {
                Some(g) => {
                    g.reach = g.reach.max(reach);
                    g.items.push(i);
                }
                None => groups.push(Group {
                    family: d.family(),
                    leader: *k,
                    reach,
                    items: vec![i],
                }),
            }
        }
        groups.sort_by(|a, b| b.reach.total_cmp(&a.reach));

        // the discs placed so far (x, y, r): what the next group keeps clear of
        let mut discs: Vec<[f64; 3]> = Vec::new();
        for g in &groups {
            let leader = &self.children[g.leader].0;
            let members: Vec<&Drawn> = g.items.iter().map(|&i| &drawn[i].1).collect();
            let placed = match foot {
                Some(foot) => leader.gather(scene, ctx, &mut rng, foot, self.reach, &members)?,
                None => {
                    let meets = leader.meets();
                    let radii: Vec<f64> = members.iter().map(|d| d.radius_mm()).collect();
                    let placed = place_group(
                        scene,
                        ctx,
                        &mut rng,
                        line_reach,
                        &Sites {
                            radii: &radii,
                            gap: self.gap * s,
                            meets,
                            plural: leader.plural(),
                            avoid: &discs,
                            shape: &shape,
                        },
                    )?;
                    if meets.upright.is_some() && placed.len() < radii.len() {
                        return Err(leader.no_room(placed.len(), radii.len()));
                    }
                    placed
                }
            };
            // what is near an item changes it, or leaves it out
            let mut standing: Vec<Standing<'_>> = Vec::with_capacity(placed.len());
            for p in &placed {
                let tall = match &nearness {
                    None => 1.0,
                    Some(near) => {
                        let item = ctx.seed.key("item").index(g.items[p.index] as u64);
                        match near.answer(item, p.at) {
                            Some(tall) => tall,
                            None => continue,
                        }
                    }
                };
                standing.push(Standing {
                    drawn: members[p.index],
                    at: p.at,
                    r: p.r,
                    tall,
                });
            }
            discs.extend(standing.iter().map(|p| [p.at[0], p.at[1], p.r]));
            leader.stand(scene, ctx, &mut rng, &standing)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use dt_kit::Frame;

    fn written(name: &str, v: Value) -> Control {
        Control {
            name: name.into(),
            value: v,
        }
    }

    /// Things with a range at every end the extent reads: a rock, a pebble,
    /// a boulder, a flower, a tree.
    fn things() -> Vec<(&'static str, Box<dyn Thing>)> {
        let size = |lo: f64, hi: f64| written("size", Value::Range(lo, hi));
        vec![
            ("rock", thing("rock", &[]).unwrap()),
            ("pebble", thing("pebble", &[size(2.0, 5.0)]).unwrap()),
            ("boulder", thing("boulder", &[size(12.0, 30.0)]).unwrap()),
            ("flower", thing("flower", &[size(4.0, 7.0)]).unwrap()),
            (
                "tree",
                thing("tree", &[written("crown/width", Value::Range(20.0, 34.0))]).unwrap(),
            ),
        ]
    }

    fn printer() -> dt_kit::Printer {
        dt_kit::Printer::new(0.45, 0.2, 55.0, 0.4).steepest(65.0)
    }

    fn child(n: &str) -> Child {
        Child {
            name: n.into(),
            controls: Vec::new(),
            weight: 1.0,
        }
    }

    /// A thing built alone stands in its own frame: its origin is where it
    /// meets the ground, +z is up, and it lies round the origin.
    #[test]
    fn a_thing_alone_is_in_its_frame() {
        for (name, t) in things() {
            let mut scene = Scene::new(Frame::dungeonblocks(2, 2), Seed::new(1));
            let solids = t.make(&mut scene, Seed::new(3), &printer()).unwrap();
            assert!(!solids.is_empty(), "{name}");
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for sol in &solids {
                let (l, h) = sol.bounds().unwrap();
                for k in 0..3 {
                    lo[k] = lo[k].min(l[k]);
                    hi[k] = hi[k].max(h[k]);
                }
            }
            // the bed is z = 0: nothing sinks more than a trunk's foot, and
            // it rises from there
            assert!(lo[2] <= 1e-9 && lo[2] > -3.0, "{name}: foot at {}", lo[2]);
            assert!(hi[2] > 1.0, "{name}: top at {}", hi[2]);
            // and it lies round the origin, not off to one side
            let e = t.extent();
            for k in 0..2 {
                let mid = 0.5 * (lo[k] + hi[k]);
                assert!(
                    mid.abs() < 0.25 * e.most.x.max(e.most.y),
                    "{name}: its middle is at {mid} on axis {k}"
                );
            }
        }
    }

    /// The extent holds every draw: the room asked for lies between its
    /// boxes, at every seed from 1 to 20.
    #[test]
    fn the_extent_bounds_every_draw() {
        for (name, t) in things() {
            let mut scene = Scene::new(Frame::dungeonblocks(2, 2), Seed::new(1));
            let s = scene.frame().scale();
            let e = t.extent();
            assert!(e.least.x <= e.most.x && e.least.z <= e.most.z, "{name}");
            for seed in 1..=20u64 {
                let item = Seed::new(seed);
                let mut rng = Rng::new(item);
                let d = t.draw(&scene, item, &mut rng, false).unwrap();
                let r = d.radius_mm() / s;
                assert!(
                    r >= 0.5 * e.least.x.min(e.least.y) - 1e-9 && r <= e.most_reach() + 1e-9,
                    "{name} seed {seed}: reach {r} outside {:.3} to {:.3}",
                    0.5 * e.least.x.min(e.least.y),
                    e.most_reach()
                );
            }
            // the solid's own top lies between the least and most height
            if name != "tree" {
                for seed in 1..=20u64 {
                    let solids = t.make(&mut scene, Seed::new(seed), &printer()).unwrap();
                    let top = solids
                        .iter()
                        .map(|sol| sol.bounds().unwrap().1[2])
                        .fold(f64::NEG_INFINITY, f64::max);
                    assert!(
                        top >= e.least.z - 1e-9 && top <= e.most.z + 1e-9,
                        "{name} seed {seed}: top {top} outside {} to {}",
                        e.least.z,
                        e.most.z
                    );
                }
            }
        }
    }

    /// How a thing meets the ground is data the placer reads: rocks are
    /// bedded, flowers dry, trees upright.
    #[test]
    fn things_say_how_they_meet_the_ground() {
        let m = |n: &str| thing(n, &[]).unwrap().meets();
        for n in ["rock", "stone", "boulder", "pebble"] {
            assert!(m(n).bedded && !m(n).dry && m(n).upright.is_none(), "{n}");
        }
        assert!(m("flower").dry && !m("flower").bedded);
        assert!(m("tree").upright.is_some() && !m("tree").dry);
    }

    #[test]
    fn a_child_that_is_no_thing_is_refused_in_the_old_words() {
        let Err(e) = thing("dirt", &[]) else {
            panic!("dirt is no thing")
        };
        assert_eq!(e.message, "`dirt` can't be scattered");
        assert_eq!(
            e.help.as_deref(),
            Some("a scatter holds rocks (rock, stone, boulder, pebble), flowers and trees")
        );
        let Err(e) = Scatter::new(&[], &[child("rock"), child("water")]) else {
            panic!("water is no thing")
        };
        assert_eq!(e.message, "`water` can't be scattered");
        let Err(e) = Scatter::new(&[], &[]) else {
            panic!("a scatter needs a child")
        };
        assert_eq!(e.message, "a scatter needs a child to say what it scatters");
    }

    /// A scatter writes what its children write, and reads what they read.
    #[test]
    fn the_plan_is_what_the_children_write_and_read() {
        let plan = |names: &[&str]| {
            let kids: Vec<Child> = names.iter().map(|n| child(n)).collect();
            Scatter::new(&[], &kids).unwrap().plan()
        };
        assert_eq!(plan(&["rock", "boulder"]).writes, Write::Solid(Kind::Rock));
        assert!(plan(&["rock"]).reads.is_empty());
        assert_eq!(plan(&["tree"]).writes, Write::Solid(Kind::Tree));
        let mix = plan(&["tree", "rock", "flower"]);
        assert_eq!(
            mix.writes,
            Write::Solids(&[Kind::Rock, Kind::Flowers, Kind::Tree])
        );
        assert_eq!(mix.reads, vec![Read::Solids(Kind::Terrain)]);
    }

    fn cover_shape(cover: f64, clearings: Vec<[f64; 3]>) -> Shape {
        Shape {
            pattern: Pattern::Cover,
            clumps: Vec::new(),
            clearings,
            cover,
        }
    }

    /// Places of a cover (one dart each, nothing spaced) at 400 a square
    /// land at that density whatever the seed, less the cover's share, and
    /// none in a clearing.
    #[test]
    fn a_cover_keeps_its_density_and_leaves_its_clearings_bare() {
        let frame = Frame::dungeonblocks(1, 1);
        let radii = vec![0.4; 400];
        let spread = |shape: &Shape| Spread {
            tries: shape.tries(30),
            inset: [0.0, 1.0],
            confine: true,
            gap: 0.0,
            factor: 1.0,
            falls: shape.falls(),
        };
        let clearings = vec![[10.0, 10.0, 4.0], [25.0, 20.0, 5.0]];
        let bare: f64 = clearings
            .iter()
            .map(|c| std::f64::consts::PI * c[2] * c[2])
            .sum();
        let kept_area = 35.0 * 35.0 - bare;
        for (cover, clear) in [(1.0, Vec::new()), (0.85, clearings.clone())] {
            let shape = cover_shape(cover, clear.clone());
            let area = if clear.is_empty() {
                35.0 * 35.0
            } else {
                kept_area
            };
            let expected = 400.0 * cover * area / (35.0 * 35.0);
            for seed in 1..=8u64 {
                let mut rng = Rng::new(Seed::new(seed));
                let placed = scatter(
                    &frame,
                    &mut rng,
                    &radii,
                    &spread(&shape),
                    &Scope::with_density(&|x, y| shape.weight(x, y)),
                    &[],
                );
                let n = placed.len() as f64;
                assert!(
                    (n - expected).abs() < 0.2 * expected,
                    "cover {cover}, seed {seed}: {n} places, expected about {expected:.0}"
                );
                for p in &placed {
                    assert!(
                        clear
                            .iter()
                            .all(|c| libm::hypot(c[0] - p.at[0], c[1] - p.at[1]) >= c[2]),
                        "a place in a clearing: {:?}",
                        p.at
                    );
                }
            }
        }
        // and a cover is dense: nothing is kept apart
        let dense = cover_shape(1.0, Vec::new());
        let mut rng = Rng::new(Seed::new(1));
        let all = scatter(
            &frame,
            &mut rng,
            &radii,
            &spread(&dense),
            &Scope::with_density(&|_, _| 1.0),
            &[],
        );
        assert_eq!(all.len(), 400);
    }

    /// How many of `placed` fall in each cell of a 4 by 4 grid of the tile.
    fn grid_counts(placed: &[Placed]) -> [usize; 16] {
        let mut cells = [0usize; 16];
        for p in placed {
            let (i, j) = ((p.at[0] / 8.75) as usize, (p.at[1] / 8.75) as usize);
            cells[j.min(3) * 4 + i.min(3)] += 1;
        }
        cells
    }

    /// A cover is even: in every cell of a 4 by 4 grid of the tile, whatever
    /// the seed, the places are within a quarter of the cell's share. Darts at
    /// random are not (the same count, thrown at random, heaps some cells and
    /// leaves others bare by far more), which is what made a cover look like
    /// piles with bare ground between.
    #[test]
    fn a_cover_is_even_where_random_darts_are_not() {
        let frame = Frame::dungeonblocks(1, 1);
        let radii = vec![0.4; 300];
        let shape = cover_shape(0.85, Vec::new());
        let spread = |falls| Spread {
            tries: 1,
            inset: [0.0, 1.0],
            confine: true,
            gap: 0.0,
            factor: 1.0,
            falls,
        };
        let worst = |falls, seed: u64| {
            let mut rng = Rng::new(Seed::new(seed));
            let placed = scatter(
                &frame,
                &mut rng,
                &radii,
                &spread(falls),
                &Scope::with_density(&|x, y| shape.weight(x, y)),
                &[],
            );
            let share = placed.len() as f64 / 16.0;
            let cells = grid_counts(&placed);
            cells
                .iter()
                .map(|&c| (c as f64 - share).abs() / share)
                .fold(0.0, f64::max)
        };
        let mut random_worst: f64 = 0.0;
        for seed in 1..=16u64 {
            let even = worst(Falls::Even, seed);
            assert!(
                even < 0.25,
                "seed {seed}: a cell is {even:.2} off its share"
            );
            random_worst = random_worst.max(worst(Falls::Apart, seed));
        }
        assert!(
            random_worst > 0.5,
            "random darts are uneven, or this test shows nothing: {random_worst:.2}"
        );
    }

    /// Under a cover things lie against what stands, touching and never
    /// in it, so a fringe at its foot has something to stand beside: the
    /// nearest items touch the rock's disc, and those within reach stand
    /// taller by the `near` height while those past it do not.
    #[test]
    fn a_covers_fringe_lands_at_the_foot_and_is_taller() {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let (cx, cy, rock) = (17.5, 17.5, 4.0);
        let mask: Vec<bool> = (0..nx * ny)
            .map(|k| {
                let (x, y) = ((k % nx) as f64 * step, (k / nx) as f64 * step);
                libm::hypot(x - cx, y - cy) < rock
            })
            .collect();
        let (dist, _) = dt_kit::distance(&frame, &mask);
        let near = Nearness {
            rule: Near {
                kinds: Some(&[Kind::Rock]),
                reach: Draw::flat(4.0, 9.0),
                height: Draw::flat(1.3, 1.6),
                thin: 0.0,
            },
            dist,
            scale: frame.scale(),
        };
        let gap = Pattern::Cover.usual_gap(1.5);
        assert!(
            gap.abs() < f64::EPSILON,
            "a cover's gap is 0 unless written"
        );
        assert!((Pattern::Spaced.usual_gap(1.5) - 1.5).abs() < f64::EPSILON);
        let radii = vec![0.5; 400];
        let shape = cover_shape(1.0, Vec::new());
        let mut rng = Rng::new(Seed::new(3));
        let placed = scatter(
            &frame,
            &mut rng,
            &radii,
            &Spread {
                tries: 1,
                inset: [0.0, 1.0],
                confine: true,
                gap,
                factor: 1.0,
                falls: shape.falls(),
            },
            &Scope::with_density(&|x, y| shape.weight(x, y)),
            &[[cx, cy, rock]],
        );
        let mut at_the_foot = 0;
        for p in &placed {
            let out = libm::hypot(p.at[0] - cx, p.at[1] - cy) - rock - p.r;
            assert!(out >= 0.0, "in the rock: {:?}", p.at);
            let item = Seed::new(7).index(p.index as u64);
            let tall = near.answer(item, p.at).unwrap();
            if out < 0.6 {
                at_the_foot += 1;
            }
            if out + p.r < 4.0 {
                assert!(
                    (1.3..=1.6).contains(&tall),
                    "{tall} at {out:.2} from the foot"
                );
            } else if out + p.r >= 9.0 {
                assert!((tall - 1.0).abs() < f64::EPSILON, "{tall} past the reach");
            }
        }
        assert!(at_the_foot >= 2, "{at_the_foot} items touch the rock");
    }

    /// A clumped pattern puts its places inside the clumps and nowhere else.
    #[test]
    fn clumps_hold_every_place() {
        let frame = Frame::dungeonblocks(1, 1);
        let shape = Shape {
            pattern: Pattern::Clumped,
            clumps: vec![[8.0, 8.0, 4.0], [26.0, 24.0, 5.0]],
            clearings: Vec::new(),
            cover: 1.0,
        };
        let radii = vec![0.3; 60];
        let spread = Spread {
            tries: shape.tries(30),
            inset: [0.0, 1.0],
            confine: true,
            gap: 0.2,
            factor: 1.0,
            falls: shape.falls(),
        };
        let mut rng = Rng::new(Seed::new(2));
        let placed = scatter(
            &frame,
            &mut rng,
            &radii,
            &spread,
            &Scope::with_density(&|x, y| shape.weight(x, y)),
            &[],
        );
        assert!(placed.len() > 30, "{} places", placed.len());
        for p in &placed {
            assert!(
                shape
                    .clumps
                    .iter()
                    .any(|c| libm::hypot(c[0] - p.at[0], c[1] - p.at[1]) < c[2]),
                "a place outside every clump: {:?}",
                p.at
            );
        }
    }

    /// What is near changes an item only within its reach: always inside the
    /// reach's low end, never past its high end, softly between.
    #[test]
    fn what_is_near_changes_items_only_within_reach() {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        // a solid disc 3 mm across in the middle of the tile
        let mask: Vec<bool> = (0..nx * ny)
            .map(|k| {
                let (x, y) = ((k % nx) as f64 * step, (k / nx) as f64 * step);
                libm::hypot(x - 17.5, y - 17.5) < 1.5
            })
            .collect();
        let (dist, _) = dt_kit::distance(&frame, &mask);
        let near = Nearness {
            rule: Near {
                kinds: Some(&[Kind::Rock]),
                reach: Draw::flat(6.0, 12.0),
                height: Draw::flat(2.0, 2.0),
                thin: 0.0,
            },
            dist,
            scale: frame.scale(),
        };
        let from = |d: f64| [17.5 + 1.5 + d, 17.5];
        for seed in 1..=40u64 {
            let item = Seed::new(seed);
            assert_eq!(
                near.answer(item, from(4.0)),
                Some(2.0),
                "inside the low end"
            );
            assert_eq!(
                near.answer(item, from(13.5)),
                Some(1.0),
                "past the high end"
            );
        }
        let changed = |d: f64| {
            (1..=400u64)
                .filter(|&seed| near.answer(Seed::new(seed), from(d)) == Some(2.0))
                .count()
        };
        let (a, b) = (changed(7.5), changed(10.5));
        assert!(
            a > b && a > 100 && b > 20,
            "a soft edge: {a} at 7.5, {b} at 10.5 of 400"
        );
        // thinning leaves items out, only within reach
        let thin = Nearness {
            rule: Near {
                thin: 1.0,
                ..near.rule.clone()
            },
            dist: near.dist.clone(),
            scale: near.scale,
        };
        assert_eq!(thin.answer(Seed::new(1), from(4.0)), None);
        assert_eq!(thin.answer(Seed::new(1), from(13.5)), Some(1.0));
    }

    /// The pattern, the cover and `near` read as written, and a tree is
    /// refused a height it cannot take.
    #[test]
    fn the_pattern_controls_are_read() {
        let ok = Scatter::new(
            &[
                written("pattern", Value::Word("cover".into())),
                written("cover", Value::Number(0.85)),
                written("near/of", Value::Word("standing".into())),
                written("near/height", Value::Range(1.5, 2.0)),
            ],
            &[child("flower")],
        );
        assert!(ok.is_ok());
        let Err(e) = Scatter::new(
            &[
                written("near/of", Value::Word("rock".into())),
                written("near/height", Value::Range(1.5, 2.0)),
            ],
            &[child("tree")],
        ) else {
            panic!("trees do not grow taller")
        };
        assert_eq!(e.control.as_deref(), Some("near/height"));
    }
}
