//! A thing in its own frame, and the one place it is built once.
//!
//! A **thing** (a rock, a flower: `dt-elements`' `Thing`) is made from its
//! controls and a seed and reads nothing of the tile, so what it is can be
//! built once and copied.  Its built form is a [`Body`]: meshes in the
//! thing's own frame (the origin where it meets what holds it, +z away from
//! that surface), no slots, nothing of the scene.  Every thing is set down
//! the same way: [`Body::set_down`] gives each part a slot, turns it about
//! z, stretches it up and moves it into place, and that is the only path
//! from a thing to a solid in the scene.  A body built a second time for the
//! same [`ThingId`] is the same bytes, so a copy is never different from a
//! build.
//!
//! **Its own key.**  A [`ThingId`] is the canonical form (`dt_value::Canon`,
//! the writer every key is made with) of everything the body is made from:
//! the kind of thing and each number its builder reads.  The store gives it
//! a key by the same maker as a tile's, with the build target in it
//! (`dt_store::make_thing_key`), so a key that matches is the right body.
//!
//! **Where bodies are kept.**  [`Bodies`] is one build's: it asks for a body
//! by id and a way to make it, and answers from, in order, the build's own
//! memory (a variant, [`Bodies::body`]), the shelf ([`Shelf`], which the
//! store implements: the disk across builds, the browser's memory) and the
//! builder.  The engine knows nothing of the store; it is handed a shelf.
//! What it did is counted ([`ThingUse`]) and said in the build record.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dt_geom::TriMesh;
use dt_value::{Canon, LockExt as _};

use crate::{KitError, Material, Slots, Solid, form};

/// One part of a [`Body`]: a closed mesh of one material.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyPart {
    /// What it is made of: the slots it is set down in must be of it.
    pub material: Material,
    /// Its surface, in the thing's own frame.
    pub mesh: TriMesh,
}

/// A thing built, in its own frame: the origin where it meets what holds it,
/// +z away from that surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    /// Its parts, in the order they are given slots.
    pub parts: Vec<BodyPart>,
}

impl Body {
    /// The triangles in all its parts.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.triangles.len()).sum()
    }

    /// Set it down: part `i` is the solid of `slots.get(first + i)`, turned
    /// `turn_deg` about z through the origin, stretched `tall` times along z
    /// (from the origin, where it meets what holds it), and moved to `at`.
    /// Source ids are the slots', so what is made of it is the same by index
    /// whatever builds it.
    ///
    /// # Errors
    /// [`KitError::Shape`] when a slot is of another material than its part;
    /// the kernel's refusal of a mesh.
    pub fn set_down(
        &self,
        slots: &Slots,
        first: u32,
        turn_deg: f64,
        tall: f64,
        at: [f64; 3],
    ) -> Result<Vec<Solid>, KitError> {
        let mut out = Vec::with_capacity(self.parts.len());
        for (i, part) in self.parts.iter().enumerate() {
            let slot = slots.get(first + dt_value::idx(i));
            if slot.material() != part.material {
                return Err(KitError::Shape(format!(
                    "a {:?} part was given a {:?} slot",
                    part.material,
                    slot.material()
                )));
            }
            out.push(
                form::solid(slot, &part.mesh)?
                    .turn(turn_deg, tall)
                    .translate(at),
            );
        }
        Ok(out)
    }
}

/// What a body is made from, in the canonical form: the kind of thing
/// (`rock`, `flower`) and the bytes of each number its builder reads.  Two
/// ids are equal exactly when the bodies are the same.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThingId {
    kind: &'static str,
    canon: Vec<u8>,
}

impl ThingId {
    /// The id of a `kind` of thing whose builder reads what `write` puts in
    /// the canonical form.
    #[must_use]
    pub fn new(kind: &'static str, write: impl FnOnce(&mut Canon)) -> Self {
        let mut c = Canon::new();
        write(&mut c);
        Self {
            kind,
            canon: c.bytes().to_vec(),
        }
    }

    /// The kind of thing.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// The canonical bytes of what its builder reads.
    #[must_use]
    pub fn canon(&self) -> &[u8] {
        &self.canon
    }
}

/// Where bodies are kept between builds: what the engine is given, and the
/// store (`dt-store`) implements, so the engine knows nothing of disks.
pub trait Shelf: Sync {
    /// The body kept for `id`; `Ok(None)` when none is.
    ///
    /// # Errors
    /// A damaged or unreadable entry, in words (it is dropped, so the next
    /// build makes the body again).
    fn read(&self, id: &ThingId) -> Result<Option<Body>, String>;

    /// Keep `body` for `id`.
    ///
    /// # Errors
    /// The body cannot be kept, in words.
    fn keep(&self, id: &ThingId, body: &Body) -> Result<(), String>;

    /// The build is over: bring the shelf under its limit once (a shelf
    /// that checks its size at each [`Shelf::keep`] would scan its whole
    /// folder for each of a cover's thousands).
    ///
    /// # Errors
    /// The shelf cannot be brought under its limit, in words.
    fn settle(&self) -> Result<(), String>;
}

/// What a build did for its things: how many bodies it made, took from its
/// own memory, and read from the shelf.  Every thing set down is one of the
/// three, so `built + shared + kept` is how many bodies were asked for.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
pub struct ThingUse {
    /// Bodies made.
    pub built: u32,
    /// Bodies a variant had already made in this build, copied.
    pub shared: u32,
    /// Bodies read from the shelf (made by an earlier build).
    pub kept: u32,
}

/// One build's bodies.  Ask [`Bodies::body`] for a body by its [`ThingId`].
pub struct Bodies<'a> {
    shelf: Option<&'a dyn Shelf>,
    /// The bodies a variant asked for, kept in memory for the rest of the
    /// build: a variant is copied many times, anything else once.
    memo: Mutex<HashMap<ThingId, Arc<Body>>>,
    used: Mutex<Used>,
}

#[derive(Default)]
struct Used {
    counts: ThingUse,
    problems: Vec<String>,
    /// Whether anything was put on the shelf: a build that kept nothing
    /// leaves the shelf as it found it.
    wrote: bool,
}

impl<'a> Bodies<'a> {
    /// A build's bodies, kept on `shelf` between builds when there is one.
    #[must_use]
    pub fn new(shelf: Option<&'a dyn Shelf>) -> Self {
        Self {
            shelf,
            memo: Mutex::new(HashMap::new()),
            used: Mutex::new(Used::default()),
        }
    }

    /// The body for `id`: from the build's memory when a variant made it
    /// already, else from the shelf, else made by `make` (and put on the
    /// shelf).  A `variant` body is held for the rest of the build, since it
    /// is asked for again; any other is handed over and forgotten, so a
    /// cover of thousands holds one body at a time.  Reading or keeping a
    /// body that fails is told ([`Bodies::problems`]) and the body is made:
    /// the shelf never changes what a build makes.
    ///
    /// # Errors
    /// What `make` fails with.
    pub fn body<E>(
        &self,
        id: &ThingId,
        variant: bool,
        make: impl FnOnce() -> Result<Body, E>,
    ) -> Result<Arc<Body>, E> {
        if variant && let Some(b) = self.memo.lock_unpoisoned().get(id) {
            self.used.lock_unpoisoned().counts.shared += 1;
            return Ok(Arc::clone(b));
        }
        let mut problems = Vec::new();
        let mut wrote = false;
        let mut kept = None;
        if let Some(shelf) = self.shelf {
            match shelf.read(id) {
                Ok(found) => kept = found,
                Err(e) => problems.push(e),
            }
        }
        let was_kept = kept.is_some();
        let body = match kept {
            Some(b) => b,
            None => {
                let b = make()?;
                if let Some(shelf) = self.shelf {
                    wrote = true;
                    if let Err(e) = shelf.keep(id, &b) {
                        problems.push(e);
                    }
                }
                b
            }
        };
        let body = Arc::new(body);
        if variant {
            self.memo
                .lock_unpoisoned()
                .insert(id.clone(), Arc::clone(&body));
        }
        let mut used = self.used.lock_unpoisoned();
        if was_kept {
            used.counts.kept += 1;
        } else {
            used.counts.built += 1;
        }
        used.problems.append(&mut problems);
        used.wrote |= wrote;
        Ok(body)
    }

    /// What was made, copied and read so far.
    #[must_use]
    pub fn used(&self) -> ThingUse {
        self.used.lock_unpoisoned().counts
    }

    /// The build is over: settle the shelf, and say what went wrong with it
    /// during the build (a damaged entry dropped, a body that could not be
    /// kept), in words.  The build is real either way.
    #[must_use]
    pub fn finish(&self) -> Vec<String> {
        let (mut problems, wrote) = {
            let mut used = self.used.lock_unpoisoned();
            (std::mem::take(&mut used.problems), used.wrote)
        };
        if wrote
            && let Some(shelf) = self.shelf
            && let Err(e) = shelf.settle()
        {
            problems.push(e);
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Frame, Scene, Seed};

    fn tetra() -> TriMesh {
        TriMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]],
            ..TriMesh::default()
        }
    }

    fn body() -> Body {
        Body {
            parts: vec![BodyPart {
                material: Material::Rock,
                mesh: tetra(),
            }],
        }
    }

    fn id(n: f64) -> ThingId {
        ThingId::new("test", |c| {
            c.put(&n);
        })
    }

    #[test]
    fn a_body_is_set_down_turned_stretched_and_moved() {
        let mut scene = Scene::new(Frame::dungeonblocks(1, 1), Seed::new(1));
        let slots = scene.slots(2, Material::Rock).unwrap();
        let b = body();
        let a = b.set_down(&slots, 0, 0.0, 1.0, [5.0, 6.0, 7.0]).unwrap();
        let c = b.set_down(&slots, 1, 90.0, 2.0, [0.0, 0.0, 0.0]).unwrap();
        let (lo, hi) = a[0].bounds().unwrap();
        assert!((lo[0] - 5.0).abs() < 1e-9 && (hi[2] - 8.0).abs() < 1e-9);
        // a quarter turn takes +x to +y; twice as tall is two high
        let (lo, hi) = c[0].bounds().unwrap();
        assert!((hi[1] - 1.0).abs() < 1e-9 && (lo[0] + 1.0).abs() < 1e-9);
        assert!((hi[2] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_part_in_a_slot_of_another_material_is_refused() {
        let mut scene = Scene::new(Frame::dungeonblocks(1, 1), Seed::new(1));
        let slots = scene.slots(1, Material::Flower).unwrap();
        assert!(body().set_down(&slots, 0, 0.0, 1.0, [0.0; 3]).is_err());
    }

    #[test]
    fn the_same_id_is_the_same_thing_and_a_number_changes_it() {
        assert_eq!(id(1.0), id(1.0));
        assert_ne!(id(1.0), id(1.5));
        let other = ThingId::new("other", |c| {
            c.put(&1.0_f64);
        });
        assert_ne!(id(1.0), other);
    }

    /// A shelf in a map, counting what it is asked.
    #[derive(Default)]
    struct Map(Mutex<HashMap<ThingId, Body>>);

    impl Shelf for Map {
        fn read(&self, id: &ThingId) -> Result<Option<Body>, String> {
            Ok(self.0.lock_unpoisoned().get(id).cloned())
        }
        fn keep(&self, id: &ThingId, body: &Body) -> Result<(), String> {
            self.0.lock_unpoisoned().insert(id.clone(), body.clone());
            Ok(())
        }
        fn settle(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn make(n: &mut u32) -> Result<Body, String> {
        *n += 1;
        Ok(body())
    }

    #[test]
    fn a_variant_is_built_once_and_copied() {
        let bodies = Bodies::new(None);
        let mut made = 0;
        for _ in 0..5 {
            bodies.body(&id(1.0), true, || make(&mut made)).unwrap();
        }
        assert_eq!(made, 1);
        assert_eq!(
            bodies.used(),
            ThingUse {
                built: 1,
                shared: 4,
                kept: 0
            }
        );
    }

    #[test]
    fn a_body_that_is_not_a_variant_is_not_held() {
        let bodies = Bodies::new(None);
        let mut made = 0;
        for _ in 0..3 {
            bodies.body(&id(1.0), false, || make(&mut made)).unwrap();
        }
        // nothing holds it, so asking again makes it again
        assert_eq!(made, 3);
        assert_eq!(bodies.used().built, 3);
    }

    #[test]
    fn a_body_kept_by_one_build_is_read_by_the_next() {
        let shelf = Map::default();
        let mut made = 0;
        let first = Bodies::new(Some(&shelf));
        first.body(&id(2.0), false, || make(&mut made)).unwrap();
        assert!(first.finish().is_empty());
        let second = Bodies::new(Some(&shelf));
        let b = second.body(&id(2.0), false, || make(&mut made)).unwrap();
        assert_eq!(made, 1);
        assert_eq!(*b, body());
        assert_eq!(
            second.used(),
            ThingUse {
                built: 0,
                shared: 0,
                kept: 1
            }
        );
    }
}
