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

use dt_kit::{Field, Kind, Mask, Placed, Rng, Scene, Seed, Solid, Spread, scatter};

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
}

/// One dart-throw of `sites` into `area`: largest first, each `gap` clear
/// of the others and of the discs to avoid.
fn one_layout(
    scene: &Scene,
    area: &Mask,
    rng: &mut Rng,
    sites: &Sites<'_>,
) -> Result<Vec<Placed>, LookError> {
    let Sites {
        radii,
        gap,
        meets,
        plural,
        avoid,
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
            blocker: scene.standing(&[Kind::Rock, Kind::Tree, Kind::Wall], 0.45),
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
    let density = |x: f64, y: f64| -> f64 {
        if let Some(d) = &dry {
            // the whole disc must be dry, in its area, off what stands there
            let ok = (0..8).all(|k| {
                let a = std::f64::consts::TAU * f64::from(k) / 8.0;
                let (px, py) = (x + d.widest * libm::cos(a), y + d.widest * libm::sin(a));
                let (i, j) = d.ground.node(px.clamp(0.0, w), py.clamp(0.0, h));
                let q = j * nx + i;
                area.bits[q] && !d.wet.bits[q] && !d.blocker.bits[q]
            });
            let (i, j) = d.ground.node(x, y);
            return if ok && area.bits[j * nx + i] {
                1.0
            } else {
                0.0
            };
        }
        let (i, j) = (
            (libm::round(x / step).max(0.0) as usize).min(nx - 1),
            (libm::round(y / step).max(0.0) as usize).min(ny - 1),
        );
        if !area.bits[j * nx + i] {
            return 0.0;
        }
        if let (Some(c), Some(sup)) = (clear, support) {
            for jj in j.saturating_sub(rr)..=(j + rr).min(ny - 1) {
                for ii in i.saturating_sub(rr)..=(i + rr).min(nx - 1) {
                    let d =
                        libm::hypot((ii as f64 - i as f64) * step, (jj as f64 - j as f64) * step);
                    if d <= c && sup.z[jj * nx + ii].is_finite() {
                        return 0.0;
                    }
                }
            }
        }
        1.0
    };
    let spread = Spread {
        tries: meets.tries,
        inset: [meets.rim_mm * s, 1.0],
        confine: true,
        gap,
        factor: 1.0,
    };
    Ok(scatter(&frame, rng, radii, &spread, &density, avoid))
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
    let mut avoid = if sites.meets.bedded {
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
        let layout = one_layout(scene, &area, rng, &sites)?;
        if layout.len() > placed.len() {
            placed = layout;
        }
        if placed.len() == sites.radii.len() {
            break;
        }
    }
    Ok(placed)
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
    children: Vec<(Box<dyn Thing>, f64)>,
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
        Ok(Self {
            count: set.dist("count")?,
            per_square: set.dist_or("perSquare", usual.per_square)?,
            gap: set.number_or("gap", usual.gap_mm)?,
            reach: set.number("reach")?.or(usual.reach_mm),
            stream,
            children: made,
        })
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
        let frame = *scene.frame();
        let s = frame.scale();
        let mut rng = Rng::new(ctx.seed.key(self.stream));
        let line_reach = self.reach.unwrap_or(crate::LINE_REACH_MM);
        // squares the area covers
        let squares = foot.map_or_else(
            || ctx.spread_squares(&frame, &ctx.spread_area(&frame, line_reach)),
            |f| f.squares,
        );
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
                        },
                    )?;
                    if meets.upright.is_some() && placed.len() < radii.len() {
                        return Err(leader.no_room(placed.len(), radii.len()));
                    }
                    placed
                }
            };
            discs.extend(placed.iter().map(|p| [p.at[0], p.at[1], p.r]));
            let standing: Vec<Standing<'_>> = placed
                .iter()
                .map(|p| Standing {
                    drawn: members[p.index],
                    at: p.at,
                    r: p.r,
                })
                .collect();
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
}
