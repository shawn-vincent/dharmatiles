//! The TypeScript of the types a page reads: the one gatherer both generated contracts use (the preview page's
//! `web/preview/js/api.d.ts`, made by the program's tests, and the site's `docs/assets/live/engine-wire.d.ts`, made by
//! dt-web's).  A contract is a set of Rust types with serde and `ts_rs::TS`, collected with everything they name; this
//! is the one place that turns them into declarations, so the two files cannot say a type in two ways.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};

use ts_rs::TS;

/// The types of a contract, gathered with everything they name.
pub struct Contract {
    /// The TypeScript settings every declaration is made with.
    pub cfg: ts_rs::Config,
    seen: BTreeSet<TypeId>,
    decls: BTreeMap<String, String>,
    /// The Rust type behind each TypeScript name: two types with one name would leave the page one of them, unsaid.
    named: BTreeMap<String, &'static str>,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            // JSON numbers are numbers: the page has no bigint
            cfg: ts_rs::Config::default().with_large_int("number"),
            seen: BTreeSet::new(),
            decls: BTreeMap::new(),
            named: BTreeMap::new(),
        }
    }
}

impl Contract {
    /// Add `T` and every type it names.
    ///
    /// # Panics
    /// Two Rust types that say different things are one TypeScript name: the page would see one of them.
    pub fn add<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        // a builtin (a number, a string, an array) has no declaration of its own
        if T::output_path().is_some() {
            let name = T::ident(&self.cfg);
            let decl = T::decl(&self.cfg);
            let rust = std::any::type_name::<T>();
            // two Rust types may share a name only when they say the same thing to the page (ts-rs's own JSON
            // value and serde_json's; a generic type's instances)
            if let Some(was) = self.decls.get(&name) {
                assert_eq!(
                    was, &decl,
                    "two types are `{name}` in the page contract ({} and {rust}), and the page would see one of them: rename one",
                    self.named[&name]
                );
            }
            self.named.insert(name.clone(), rust);
            self.decls.insert(name, decl);
        }
        struct V<'a>(&'a mut Contract);
        impl ts_rs::TypeVisitor for V<'_> {
            fn visit<U: TS + 'static + ?Sized>(&mut self) {
                self.0.add::<U>();
            }
        }
        T::visit_dependencies(&mut V(self));
    }

    /// Every declaration, by name, each followed by a blank line.
    #[must_use]
    pub fn declarations(&self) -> String {
        let mut out = String::new();
        for d in self.decls.values() {
            out.push_str(d);
            out.push_str("\n\n");
        }
        out
    }
}
