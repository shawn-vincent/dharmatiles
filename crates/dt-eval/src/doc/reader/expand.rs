//! Expanding a stack: definitions inlined, parameters bound, scatter
//! children made.

use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::expr::Expr;
use dt_doc::model::{self, Callee, Definition, Spanned};
use dt_doc::place::{Along, Point};
use dt_doc::schema::{PlaceWord, Role};
use dt_kit::{Draw, EntryId, Seed};

use crate::diag::{Diag, Src};

use super::controls::{bind, property_span};
use super::*;

impl Reader<'_> {
    /// Inline `entries` into `out`: built elements become [`Entry`]s, what
    /// is not built yet goes to `pending`.
    pub(super) fn expand(
        &mut self,
        entries: &[model::Entry],
        scope: &Scope,
        out: &mut Vec<Entry>,
        pending: &mut Vec<Diag>,
    ) {
        let src = &scope.file.src;
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for e in entries {
            // an entry's own block of `let`s and `override`s holds for it
            let entry_scope;
            let scope = if e.consts.is_empty() {
                scope
            } else {
                entry_scope = self.scoped(scope, &e.consts);
                &entry_scope
            };
            if scope.depth == 0 {
                self.top = Some((Arc::clone(src), e.span));
            }
            if self.entries >= MAX_ENTRIES {
                if self.entries == MAX_ENTRIES {
                    self.entries += 1;
                    let (at, span) = self.top.clone().unwrap_or((Arc::clone(src), e.span));
                    self.errors.push(
                        Diag::at(
                            &at,
                            span,
                            "dt::too-many",
                            format!("this expands to more than {MAX_ENTRIES} stack entries"),
                            "expands too far",
                        )
                        .help("a definition that calls others several times each multiplies at every level; use `scatter` or fewer calls"),
                    );
                }
                return;
            }
            self.entries += 1;
            let mut area = e.place.region.clone().or_else(|| scope.area.clone());
            let at = e.place.at.clone().or_else(|| scope.at.clone());
            let along = e.place.along.clone().or_else(|| scope.along.clone());
            // this entry's path under an enclosing definition call: its
            // name, then its id or its count among same-named siblings
            // (an id is never all digits, so the two cannot meet)
            let own = match &e.place.id {
                Some(i) => format!("{}/id:{}", e.what.value, i.value),
                None => {
                    let k = seen.entry(e.what.value.clone()).or_default();
                    *k += 1;
                    format!("{}/{}", e.what.value, *k - 1)
                }
            };
            let seed = match &e.place.seed {
                Some(s) => {
                    if let Ok(v) = u64::try_from(s.value) {
                        Some(v)
                    } else {
                        self.errors.push(Diag::at(
                            src,
                            s.span,
                            "dt::seed",
                            "a seed is a whole number from 0",
                            "negative",
                        ));
                        None
                    }
                }
                // inherited from a definition call: one seed per inner
                // entry, keyed by its path under the call
                None => scope.seed.map(|s| Seed::new(s).path(&own).value()),
            };
            let id = match (&e.place.id, &scope.id) {
                (_, Some(base)) => Some(format!("{base}/{own}")),
                (Some(i), None) => Some(i.value.clone()),
                (None, None) => None,
            };
            // who this entry is, in seed paths: by its id, else by how many
            // entries of its element come before it in the whole stack
            let entry_id = EntryId::new(&match &id {
                Some(i) => format!("tile/{}/{i}", e.what.value),
                None => format!(
                    "tile/{}/{}",
                    e.what.value,
                    self.counts.get(&e.what.value).copied().unwrap_or(0)
                ),
            });
            match &e.callee {
                Callee::Element(element) => {
                    if !dt_elements::built().any(|n| n == element.name) {
                        pending.push(not_yet(
                            src,
                            e.what.span,
                            &format!("element `{}`", element.name),
                        ));
                        continue;
                    }
                    let mut ok = true;
                    // inside a definition the decoder leaves a required
                    // place word to the call; it must arrive from there
                    for need in element.needs {
                        let has = match need {
                            PlaceWord::Along => along.is_some(),
                            PlaceWord::At => at.is_some(),
                            _ => true,
                        };
                        if !has {
                            ok = false;
                            let w = need.as_str();
                            let mut d = Diag::at(
                                src,
                                e.what.span,
                                "dt::missing-control",
                                format!(
                                    "`{}` needs `{w}=`, and the definition call gives none",
                                    element.name
                                ),
                                format!("no `{w}=` here or at the call"),
                            );
                            d.help = Some(format!(
                                "write it here, or pass it at the call: `name {w}=…`"
                            ));
                            self.errors.push(d);
                        }
                    }
                    // at= a path of edges and at= a point, for the elements
                    // that place by them; the rest is not built yet
                    let mut path = None;
                    let mut line = None;
                    let mut side = None;
                    let mut point = None;
                    let against = if element.honours(PlaceWord::Against) {
                        e.place.against.as_ref().map(|a| a.value.clone())
                    } else {
                        None
                    };
                    let mut unplaced = vec![("side=", e.place.side.as_ref().map(|s| s.span))];
                    if !element.honours(PlaceWord::Against) {
                        unplaced.push(("at=foot(…)", e.place.against.as_ref().map(|s| s.span)));
                    }
                    match &along {
                        Some(a) if element.honours(PlaceWord::Along) => match &a.value {
                            Along::Edges(edges) => path = Some(edge_path(edges, self.dims)),
                            Along::Line(name) if element.name == "scatter" => {
                                line = Some(name.clone());
                            }
                            Along::Line(name) if element.name == "wall" => {
                                line = Some(name.clone());
                                side = e.place.side.as_ref().map(|s| s.value);
                                unplaced.retain(|(w, _)| *w != "side=");
                            }
                            Along::Line(_) => unplaced.push(("at= a drawn line", Some(a.span))),
                        },
                        // a dirt, grass or cliff follows a line by its strip
                        Some(a) if element.place.contains(&PlaceWord::Along) => {
                            match self.strip(&a.value, element.name) {
                                Ok(name) => {
                                    area = Some(Spanned {
                                        value: name,
                                        span: a.span,
                                    });
                                }
                                Err((msg, help)) => {
                                    ok = false;
                                    self.errors.push(
                                        Diag::at(src, a.span, "dt::type", msg, "here").help(help),
                                    );
                                }
                            }
                        }
                        // inherited from a definition call: for the elements
                        // that place by it only, the others ignore it
                        Some(a) if e.place.along.is_some() => {
                            unplaced.push(("at= a line", Some(a.span)));
                        }
                        _ => {}
                    }
                    match &at {
                        Some(p) if element.honours(PlaceWord::At) => {
                            let fixed = |((x0, x1), (y0, y1)): dt_doc::place::Bounds| {
                                [Draw::flat(x0, x1), Draw::flat(y0, y1)]
                            };
                            let drawn = match &p.value {
                                Point::Named(n) => self
                                    .points
                                    .get(n)
                                    .map(|q| fixed(((q[0], q[0]), (q[1], q[1]))))
                                    .ok_or_else(|| {
                                        ("dt::at", format!("no point is called `{n}`"), None)
                                    }),
                                Point::Pair { x, y } => crate::lower::coordinate(x)
                                    .and_then(|x| Ok([x, crate::lower::coordinate(y)?]))
                                    .map_err(|r| (r.code, r.message, r.help)),
                                // a part's point: its frame is the tile of
                                // the call site, so it is worked out here
                                Point::Framed(f) => self
                                    .in_tile_frame(f, area.is_some())
                                    .and_then(|(x, y)| {
                                        Ok([
                                            crate::lower::coordinate(&x)?,
                                            crate::lower::coordinate(&y)?,
                                        ])
                                    })
                                    .map_err(|r| (r.code, r.message, r.help)),
                                v => v
                                    .bounds(self.dims.0, self.dims.1)
                                    .map(fixed)
                                    .map_err(|msg| ("dt::at", msg, None)),
                            };
                            match drawn {
                                Ok(d) => point = Some(d),
                                Err((code, msg, help)) => self.errors.push(
                                    Diag::at(src, p.span, code, msg, "here").maybe_help(help),
                                ),
                            }
                        }
                        Some(p) if e.place.at.is_some() => unplaced.push(("at=", Some(p.span))),
                        _ => {}
                    }
                    for (word, span) in unplaced {
                        if let Some(span) = span {
                            ok = false;
                            self.errors.push(misplaced(src, span, word, element.name));
                        }
                    }
                    let mut children = Vec::new();
                    // a scatter inside a surface scatters on that surface:
                    // its own entry, in the surface's area, listed right after it
                    let mut hoisted: Vec<model::Entry> = Vec::new();
                    for c in &e.children {
                        if element.role == Role::Surface
                            && matches!(&c.callee, Callee::Element(cl) if cl.name == "scatter")
                        {
                            hoisted.push(c.clone());
                            continue;
                        }
                        if element.name != "scatter" {
                            ok = false;
                            self.errors.push(
                                Diag::at(
                                    src,
                                    c.what.span,
                                    "dt::nesting",
                                    format!(
                                        "`{}` can't go inside `{}`",
                                        c.what.value, element.name
                                    ),
                                    "not a control here",
                                )
                                .help(format!(
                                    "list `{}` in the stack beside `{}`",
                                    c.what.value, element.name
                                )),
                            );
                            continue;
                        }
                        match self.scatter_child(c, scope, pending, &entry_id, children.len()) {
                            Some(child) => children.push(child),
                            None => ok = false,
                        }
                    }
                    let controls = self.controls(e, element, scope, pending, (&entry_id, None));
                    let (mut controls, mut top) = match controls {
                        Some((c, t)) => (Some(c), t),
                        None => (None, None),
                    };
                    let mut pins = Vec::new();
                    let table = scope.lets.table.clone().unwrap_or_default();
                    if element.is_body() {
                        // a body's band: its `top=` and pins, which must be
                        // readable where it is written (format 1)
                        if let Some(t) = &e.top {
                            ok &= self.readable(
                                &table,
                                &t.value,
                                t.span,
                                src,
                                Reads::At(&e.what.value),
                            );
                            let prop = property_span(src, t.span, "top");
                            top = Some(Top {
                                expr: t.value.clone(),
                                span: t.span,
                                prop,
                                src: Arc::clone(src),
                                crown: None,
                            });
                            // written, it is a control like the rest: a row of the
                            // sheet the schema describes (the solver reads `top`)
                            if let Some(cs) = controls.as_mut() {
                                cs.push(Setting {
                                    origin: Origin::Written,
                                    name: "top".to_owned(),
                                    value: dt_elements::Value::Formula(dt_elements::Formula {
                                        owner: entry_id.clone(),
                                        child: None,
                                        key: t.value.to_string(),
                                    }),
                                    span: prop,
                                    src: Arc::clone(src),
                                });
                            }
                        }
                        let from = Provenance {
                            path: scope.file.path.clone(),
                            src: Arc::clone(src),
                            default: false,
                        };
                        for p in &e.pins {
                            ok &= self.readable(
                                &table,
                                &p.top.value,
                                p.top.span,
                                src,
                                Reads::At(&e.what.value),
                            );
                            pins.push(pin_decl(p, &from));
                        }
                    } else if !dt_doc::words::takes_cloud_top(element.name)
                        && let Some(t) = &e.top
                    {
                        // a thing or a coat: its top is not built yet, except
                        // a tree's or a rock's (its height)
                        pending.push(not_yet(
                            src,
                            t.span,
                            &format!("`top=` on `{}` is", e.what.value),
                        ));
                    }
                    // a scatter along a line with a width covers its strip: half
                    // the width either side, unless `reach=` says otherwise
                    if let (Some(cs), Some(name)) = (controls.as_mut(), &line)
                        && let Some(w) = self.widths.get(name)
                        && e.control("reach").is_none()
                    {
                        cs.retain(|c| c.name != "reach");
                        cs.push(Setting {
                            origin: Origin::LineWidth,
                            name: "reach".to_owned(),
                            value: dt_elements::Value::Number(w / 2.0),
                            span: e.what.span,
                            src: Arc::clone(src),
                        });
                    }
                    if ok && let Some(controls) = controls {
                        let inner = Scope {
                            area: area.clone(),
                            ..scope.clone()
                        };
                        *self.counts.entry(e.what.value.clone()).or_default() += 1;
                        out.push(Entry {
                            id: entry_id,
                            what: e.what.clone(),
                            area,
                            seed,
                            label: e.label.as_ref().map(|l| l.value.clone()),
                            controls,
                            top,
                            pins,
                            slope: e.slope.as_ref().map(|s| s.value),
                            table,
                            along: path,
                            at: point,
                            against,
                            line,
                            side,
                            children,
                            src: Arc::clone(src),
                            span: e.span,
                            call: if scope.depth > 0 {
                                self.top.clone()
                            } else {
                                None
                            },
                        });
                        if !hoisted.is_empty() {
                            // listed right after the surface it is written in
                            self.expand(&hoisted, &inner, out, pending);
                        }
                    }
                }
                Callee::Definition(name) => {
                    let Some((file, def)) = self.definition(&scope.file, name) else {
                        self.errors.push(Diag::at(
                            src,
                            e.what.span,
                            "dt::unknown-name",
                            format!("cannot find the definition `{name}`"),
                            "not loaded",
                        ));
                        continue;
                    };
                    if scope.depth > 64 {
                        self.errors.push(Diag::at(
                            src,
                            e.what.span,
                            "dt::loop",
                            "definitions call each other too deeply",
                            "here",
                        ));
                        continue;
                    }
                    let params = Self::params(e, &def, &file, scope);
                    let top = self.call_top(e, &def, scope, pending);
                    let inner = Scope {
                        file: Arc::clone(&file),
                        params,
                        lets: scope.lets.clone(),
                        area,
                        at,
                        along,
                        top,
                        seed,
                        id,
                        depth: scope.depth + 1,
                    };
                    // a definition's own `let`s and `override`s hold for its body
                    let inner = self.scoped(&inner, &def.consts);
                    let inner = self.scoped(&inner, &def.stack_consts);
                    self.expand(&def.stack, &inner, out, pending);
                }
            }
        }
    }

    /// The `top=` a call of `def` passes to the tree inside it: the
    /// outermost call's, else this one's.  A `top=` on a call holding no
    /// tree has nothing to stand on yet.
    pub(super) fn call_top(
        &mut self,
        e: &model::Entry,
        def: &Definition,
        scope: &Scope,
        pending: &mut Vec<Diag>,
    ) -> Option<(Spanned<Expr>, Src)> {
        let src = &scope.file.src;
        if let Some(t) = &e.top {
            let holds_tree = def.stack.iter().any(|x| match &x.callee {
                Callee::Element(el) => dt_doc::words::takes_cloud_top(el.name),
                Callee::Definition(_) => true,
            });
            if !holds_tree {
                pending.push(not_yet(
                    src,
                    t.span,
                    &format!("`top=` on `{}`, which holds no tree, is", e.what.value),
                ));
            }
        }
        scope
            .top
            .clone()
            .or_else(|| e.top.clone().map(|t| (t, Arc::clone(src))))
    }

    /// What a call of `def` (defined in `file`) binds its parameters to: the
    /// call's arguments, else the parameters' defaults.
    pub(super) fn params(
        e: &model::Entry,
        def: &Definition,
        file: &Arc<File>,
        scope: &Scope,
    ) -> BTreeMap<String, Bound> {
        let mut params = BTreeMap::new();
        for p in &def.params {
            let arg = e
                .control(&p.name.value)
                .and_then(|s| s.value.as_ref())
                .map(|v| bind(v, &scope.file.src, scope))
                .or_else(|| {
                    p.default.as_ref().map(|v| Bound {
                        value: v.clone(),
                        src: Arc::clone(&file.src),
                    })
                });
            if let Some(a) = arg {
                params.insert(p.name.value.clone(), a);
            }
        }
        params
    }

    /// One child of a scatter: a thing, or a definition whose stack is one
    /// thing (`scatter count=2 { oak }`), with its controls.  `None` when
    /// it cannot be given yet or is refused (both are reported).
    pub(super) fn scatter_child(
        &mut self,
        c: &model::Entry,
        scope: &Scope,
        pending: &mut Vec<Diag>,
        owner: &EntryId,
        index: usize,
    ) -> Option<Child> {
        // a child's own block of `let`s and `override`s holds for it
        let child_scope;
        let scope = if c.consts.is_empty() {
            scope
        } else {
            child_scope = self.scoped(scope, &c.consts);
            &child_scope
        };
        let src = &scope.file.src;
        let (cl, inner, thing) = match &c.callee {
            Callee::Element(cl) if cl.scattered => (*cl, scope.clone(), c.clone()),
            Callee::Element(cl) => {
                self.errors.push(
                    Diag::at(
                        src,
                        c.what.span,
                        "dt::nesting",
                        format!("`{}` can't be scattered", cl.name),
                        "not a thing",
                    )
                    .help("a scatter holds rocks, flowers and trees"),
                );
                return None;
            }
            Callee::Definition(name) => {
                let Some((file, def)) = self.definition(&scope.file, name) else {
                    self.errors.push(Diag::at(
                        src,
                        c.what.span,
                        "dt::unknown-name",
                        format!("cannot find the definition `{name}`"),
                        "not loaded",
                    ));
                    return None;
                };
                let one = match def.stack.as_slice() {
                    [one] if scope.depth < 64 => one,
                    _ => {
                        self.errors.push(
                            Diag::at(
                                src,
                                c.what.span,
                                "dt::nesting",
                                format!("`{name}` is not one thing, so a scatter cannot hold it"),
                                "not one thing",
                            )
                            .help(
                                "a scatter holds a definition whose stack is one rock, flower or tree",
                            ),
                        );
                        return None;
                    }
                };
                let top = self.call_top(c, &def, scope, pending);
                let inner = Scope {
                    file: Arc::clone(&file),
                    params: Self::params(c, &def, &file, scope),
                    lets: scope.lets.clone(),
                    area: None,
                    at: None,
                    along: None,
                    top,
                    seed: None,
                    id: None,
                    depth: scope.depth + 1,
                };
                let inner = self.scoped(&inner, &def.consts);
                let inner = self.scoped(&inner, &def.stack_consts);
                let mut child = self.scatter_child(one, &inner, pending, owner, index)?;
                // the call's own name is where it is shown
                child.what.span = c.what.span;
                child.weight = c.place.weight.as_ref().map_or(1.0, |w| w.value);
                return Some(child);
            }
        };
        let mut ok = true;
        for (word, span) in [
            ("at= a line", c.place.along.as_ref().map(|s| s.span)),
            ("side=", c.place.side.as_ref().map(|s| s.span)),
            ("at=", c.place.at.as_ref().map(|s| s.span)),
            ("at=foot(…)", c.place.against.as_ref().map(|s| s.span)),
            ("at= a region", c.place.region.as_ref().map(|s| s.span)),
        ] {
            if let Some(span) = span {
                ok = false;
                self.errors.push(misplaced(
                    &inner.file.src,
                    span,
                    word,
                    &format!("a scatter's `{}`", cl.name),
                ));
            }
        }
        if !dt_doc::words::takes_cloud_top(cl.name)
            && let Some(t) = &thing.top
        {
            ok = false;
            pending.push(not_yet(
                &inner.file.src,
                t.span,
                &format!("`top=` on `{}` is", cl.name),
            ));
        }
        let (controls, top) = self.controls(&thing, cl, &inner, pending, (owner, Some(index)))?;
        ok.then(|| Child {
            what: Spanned {
                value: cl.name.to_owned(),
                span: c.what.span,
            },
            controls,
            top,
            table: inner.lets.table.clone().unwrap_or_default(),
            weight: c.place.weight.as_ref().map_or(1.0, |w| w.value),
        })
    }

    /// The strip a surface follows when it is placed `at=` `along`: the
    /// name of the region the line's width makes.
    ///
    /// # Errors
    /// What is wrong and how to write it: an edge has no width, and a
    /// line needs one.
    pub(super) fn strip(&self, along: &Along, element: &str) -> Result<String, (String, String)> {
        match along {
            Along::Line(name) if self.widths.contains_key(name) => Ok(strip_name(name)),
            Along::Line(name) => Err((
                format!("`{element}` follows the strip of a line, and `{name}` has no width"),
                format!("give it one: `line {name} width=9 {{ through … }}`"),
            )),
            Along::Edges(_) => Err((
                format!("`{element}` follows the strip of a line, and a tile edge has no width"),
                "write a region beside the edge and use it: `region rim { within 6 of=northEdge }`, then `at=rim`"
                    .to_owned(),
            )),
        }
    }

    /// The definition `name` as `file` sees it: its own, or one it uses.
    pub(super) fn definition(
        &self,
        file: &Arc<File>,
        name: &str,
    ) -> Option<(Arc<File>, Definition)> {
        if let Some(d) = file.doc.definition(name)
            && d.header().is_none()
        {
            return Some((Arc::clone(file), d.clone()));
        }
        file.doc
            .uses
            .iter()
            .filter(|u| u.names.iter().any(|n| n.value == name))
            .find_map(|u| {
                let f = self.files.get(&use_path(&file.path, u))?;
                let d = f.doc.definition(name)?.clone();
                Some((Arc::clone(f), d))
            })
    }
}
