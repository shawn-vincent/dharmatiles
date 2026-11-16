//! Stack entries: elements, definition calls and scatters, with their place
//! words and controls in either form (on the line, or child lines).

use dt_lang::words;
use std::sync::Arc;

use kdl::{KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use super::value::describe;
use super::{Ctx, Decoder, Fill, Wanted, spanned_name, value_span};
use crate::elements::renamed;
use crate::error::{DocError, Src};
use crate::known::{DefInfo, Known};
use crate::model::{
    Base, Callee, Definition, Entry, Form, GeneratorParam, Late, Place, Setting, Spanned, Value,
    Written,
};
use crate::place::{Along, Side};
use crate::schema::{self, Control, Kind, PlaceWord, Role, did_you_mean};
use crate::units::{self, KindError, fits, misfit, misfit_text, wanted_help};

/// Where an entry sits, which limits what it may be.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Parent {
    /// Directly in a `stack`.
    Stack,
    /// A child of a `scatter`: a thing.
    Scatter,
    /// Inside a surface (`dirt { scatter … }`): a scatter.
    Surface,
}

/// Controls asked of a thing that are a scatter's job.
const SCATTER_ONLY: [&str; 6] = ["count", "perSquare", "spread", "reach", "clumps", "gap"];

/// What a name on an entry resolved to.
enum Target {
    Control(&'static Control),
    Param(Option<&'static Control>),
    /// A parameter that holds a thing or a layer: its value is the name of
    /// one, and what it must fit is its kind.
    Generator(units::Kind),
    None,
}

/// A `let` or `override` node.
pub(super) fn is_const_node(n: &KdlNode) -> bool {
    matches!(n.name().value(), words::LET | words::OVERRIDE)
}

impl Decoder<'_> {
    /// The entries of a `stack` node.
    pub(crate) fn entries(&mut self, stack: &KdlNode, ctx: &Ctx, path: &[String]) -> Vec<Entry> {
        if let Some(e) = stack.entries().first() {
            self.err(
                dt_lang::codes::STACK,
                e.span(),
                "`stack` takes no values",
                "unexpected",
            )
            .help = Some("entries go inside: `stack { ground; tree … }`".to_owned());
        }
        let Some(kids) = stack.children() else {
            return Vec::new();
        };
        let steps = crate::nodeid::steps(kids.nodes());
        let mut base: Option<Base> = None;
        let mut out: Vec<Entry> = Vec::new();
        // format 1: the block's own `let`s and `override`s, for what is in it
        let mut block_ctx: Option<Ctx> = None;
        self.stack_consts.clear();
        for n in kids.nodes().iter().filter(|n| is_const_node(n)) {
            let c = block_ctx.get_or_insert_with(|| ctx.clone());
            if let Some(k) = self.const_node(n, c, &[]) {
                self.stack_consts.push(k);
            }
        }
        let ctx = block_ctx.as_ref().unwrap_or(ctx);
        for (n, step) in kids.nodes().iter().zip(steps) {
            if is_const_node(n) {
                continue;
            }
            if n.name().value() == "base" {
                match self.base_node(n, out.len()) {
                    Some(b) if base.is_some() => {
                        self.err(
                            dt_lang::codes::BASE_ORDER,
                            b.span,
                            "a stack has one base",
                            "a second base",
                        );
                    }
                    Some(b) => base = Some(b),
                    None => {}
                }
            } else if let Some(e) = self.entry(n, ctx, Parent::Stack, path, step) {
                out.push(e);
            }
        }
        if ctx.in_tile {
            self.check_stack_order(base.as_ref());
            self.stack_base = base;
        }
        // `id=` names an entry among its same-named siblings (its seed
        // path): two alike would share a seed.
        for (k, e) in out.iter().enumerate() {
            let Some(id) = &e.place.id else { continue };
            if out[..k].iter().any(|o| {
                o.what.value == e.what.value
                    && o.place.id.as_ref().is_some_and(|i| i.value == id.value)
            }) {
                self.err(
                    dt_lang::codes::ID,
                    id.span,
                    format!(
                        "another `{}` here already has `id={}`",
                        e.what.value, id.value
                    ),
                    "a second time",
                )
                .help = Some(
                    "give each its own id; an id names one entry among same-named siblings"
                        .to_owned(),
                );
            }
        }
        out
    }

    /// What a name is as a generator, in the one kind system: an element's
    /// kind, a definition's (a thing only when its stack is exactly one
    /// unplaced thing, else a layer; not known while it is still being
    /// decoded in a loop), `None` for no element or definition at all.
    pub(crate) fn name_kind(&self, name: &str) -> Option<units::Kind> {
        if let Some(l) = schema::element(name) {
            return Some(l.generator());
        }
        self.is_definition(name).then(|| {
            self.def_info(name)
                .map_or(units::Kind::Any, |i| i.generator)
        })
    }

    /// What an entry's callee is as a generator: its name's kind, or for a
    /// parameter what it says it holds (`Any` until the parameter line is
    /// read, which checks every use against it).
    pub(crate) fn callee_kind(&self, c: &Callee) -> units::Kind {
        match c {
            Callee::Element(l) => l.generator(),
            Callee::Definition(d) => self.def_info(d).map_or(units::Kind::Any, |i| i.generator),
            Callee::Param(p) => self
                .declared
                .get(&p.name)
                .map_or(units::Kind::Any, |d| d.kind),
        }
    }

    /// The error of a value of the wrong kind, as `misfit` words it, at
    /// `span`; `because` says how the kind was fixed, when it was not
    /// written there.
    pub(crate) fn err_kind(&mut self, span: SourceSpan, ke: KindError, because: &str) {
        let d = self.err(ke.code, span, ke.message, "wrong kind");
        d.help = match (ke.help, because.is_empty()) {
            (Some(h), true) => Some(h),
            (Some(h), false) => Some(format!("{h} ({because})")),
            (None, _) => (!because.is_empty()).then(|| because.to_owned()),
        };
    }

    /// The value of a thing or layer parameter, written `value` (as a
    /// default or an argument): the name of a generator that fits `want`, or
    /// a parameter of the enclosing definition that holds one.  `None` after
    /// the mistake is reported where it is written.
    pub(crate) fn generator_value(
        &mut self,
        e: &KdlEntry,
        owner: &str,
        name: &str,
        want: units::Kind,
        ctx: &Ctx,
    ) -> Option<Spanned<Value>> {
        let span = value_span(e);
        let who = format!("`{owner} {name}`");
        let Some(s) = e.value().as_string() else {
            let written = e.value().to_string();
            let ke = match e.value() {
                KdlValue::Integer(_) | KdlValue::Float(_) => {
                    misfit(&written, units::Kind::Plain, &who, want)
                }
                _ => misfit_text(&written, describe_kdl(e), &who, want, wanted_help(want)),
            };
            self.err_kind(span, ke, "");
            return None;
        };
        if ctx.params.iter().any(|p| p == s) {
            self.wants.entry(s.to_owned()).or_default().push(Wanted {
                who,
                kind: want,
                span,
            });
            return Some(Spanned {
                value: Value::Param(s.to_owned()),
                span,
            });
        }
        let what = Spanned {
            value: s.to_owned(),
            span,
        };
        match self.name_kind(s) {
            Some(have) if fits(have, want) => Some(Spanned {
                value: Value::Generator(have, s.to_owned()),
                span,
            }),
            Some(have) => {
                self.err_kind(span, misfit(s, have, &who, want), "");
                None
            }
            None => {
                self.callee(&what, ctx);
                None
            }
        }
    }

    fn entry(
        &mut self,
        node: &KdlNode,
        ctx: &Ctx,
        parent: Parent,
        path: &[String],
        step: String,
    ) -> Option<Entry> {
        let what = spanned_name(node);
        let callee = self.callee(&what, ctx)?;
        let mut path = path.to_vec();
        path.push(step);
        let mut entry = Entry {
            what,
            callee,
            kind: Kind::Surface,
            place: Place::default(),
            controls: Vec::new(),
            children: Vec::new(),
            consts: Vec::new(),
            top: None,
            at_place: None,
            pins: Vec::new(),
            slope: None,
            label: None,
            path,
            span: node.span(),
        };
        let owner = entry.what.value.clone();
        let element = entry.element();

        if let Callee::Param(p) = &entry.callee {
            // a parameter written as an entry is wanted as a thing in a
            // scatter, as a layer (which a thing fits) in a stack
            let (who, kind) = match parent {
                Parent::Scatter => ("a scatter", units::Kind::Thing),
                Parent::Stack | Parent::Surface => ("the stack", units::Kind::Layer),
            };
            self.wants.entry(p.name.clone()).or_default().push(Wanted {
                who: who.to_owned(),
                kind,
                span: entry.what.span,
            });
        }
        if parent == Parent::Scatter {
            let have = self.callee_kind(&entry.callee);
            if !fits(have, units::Kind::Thing) {
                let ke = misfit(&owner, have, "a scatter", units::Kind::Thing);
                let help = if element.is_some_and(|l| l.role == Role::Scatter) {
                    Some(
                        "scatters don't nest; give the outer one several children instead"
                            .to_owned(),
                    )
                } else {
                    ke.help
                };
                self.err(
                    dt_lang::codes::SCATTER,
                    entry.what.span,
                    ke.message,
                    "not a thing",
                )
                .help = help;
                return None;
            }
        }

        for e in node.entries() {
            let Some(key) = e.name() else {
                if self.heights_label(&mut entry, e) {
                    continue;
                }
                let help = match (element, e.value().as_string()) {
                    (Some(l), Some(s)) => l
                        .controls
                        .iter()
                        .find(|k| k.words.iter().any(|w| w.word == s))
                        .map(|k| format!("write `{}={s}`", k.name)),
                    _ => None,
                }
                .or_else(|| Some("write `control=value`".to_owned()));
                self.err(
                    dt_lang::codes::POSITIONAL,
                    e.span(),
                    format!("`{owner}` takes name=value controls"),
                    "no name",
                )
                .help = help;
                continue;
            };
            let key = Spanned {
                value: key.value().to_owned(),
                span: key.span(),
            };
            if self.heights_property(&mut entry, &key, e, ctx, parent) {
                continue;
            }
            if self.place_word(&mut entry, &key, e, ctx, parent) {
                continue;
            }
            self.control(&mut entry, key, Form::Property, Some(e), &[], e.span(), ctx);
        }

        let kids: &[KdlNode] = node.children().map_or(&[], |c| c.nodes());
        let steps = crate::nodeid::steps(kids);
        // format 1: the block's own `let`s and `override`s, for what is in it
        let mut block_ctx: Option<Ctx> = None;
        for c in kids.iter().filter(|c| is_const_node(c)) {
            let b = block_ctx.get_or_insert_with(|| ctx.clone());
            if let Some(k) = self.const_node(c, b, &[]) {
                entry.consts.push(k);
            }
        }
        let ctx = block_ctx.as_ref().unwrap_or(ctx);
        for (c, step) in kids.iter().zip(steps) {
            if is_const_node(c) {
                continue;
            }
            if self.heights_child(&mut entry, c, ctx) {
                continue;
            }
            let key = spanned_name(c);
            let target = self.target(&entry, &key.value);
            let is_scatter = key.value == "scatter";
            let holds = element.is_some_and(|l| l.holds_scatter);
            // on a thing parameter's entry every child line is a control, to
            // be typed at the call (the place words are the entry's own)
            let written = matches!(entry.callee, Callee::Param(_))
                && PlaceWord::parse(&key.value).is_none()
                && !matches!(key.value.as_str(), words::SEED | words::ID | words::WEIGHT);
            if written || !matches!(target, Target::None) {
                let args: Vec<&KdlEntry> =
                    c.entries().iter().filter(|e| e.name().is_none()).collect();
                let props: Vec<&KdlEntry> =
                    c.entries().iter().filter(|e| e.name().is_some()).collect();
                if let Some(kids) = c.children() {
                    self.err(
                        dt_lang::codes::CONTROL_NODE,
                        kids.span(),
                        format!("`{}` has no block", key.value),
                        "here",
                    )
                    .help = Some(format!(
                        "write its value on its line: `{} value name=value …`",
                        key.value
                    ));
                }
                if args.len() > 1 {
                    self.err(
                        dt_lang::codes::CONTROL_NODE,
                        args[1].span(),
                        format!("`{}` takes one value", key.value),
                        "a second value",
                    )
                    .help = Some(
                        "ranges are quoted: \"1dmm .. 6dmm\"; points too: \"(1sq, 1sq)\""
                            .to_owned(),
                    );
                }
                let props: Vec<KdlEntry> = props.into_iter().cloned().collect();
                self.control(
                    &mut entry,
                    key,
                    Form::Child,
                    args.first().copied(),
                    &props,
                    c.span(),
                    ctx,
                );
            } else if PlaceWord::parse(&key.value).is_some()
                || matches!(key.value.as_str(), words::SEED | words::ID | words::WEIGHT)
            {
                match c.entries() {
                    [e] if e.name().is_none() && c.children().is_none() => {
                        self.place_word(&mut entry, &key, e, ctx, parent);
                    }
                    _ => {
                        self.err(
                            dt_lang::codes::CONTROL_NODE,
                            c.span(),
                            format!("`{}` takes one value", key.value),
                            "here",
                        )
                        .help = Some(format!("write `{}=…` on the `{owner}` line", key.value));
                    }
                }
            } else if element.is_some_and(|l| l.role == Role::Scatter) {
                if let Some(child) = self.entry(c, ctx, Parent::Scatter, &entry.path, step) {
                    entry.children.push(child);
                }
            } else if is_scatter && holds {
                if let Some(child) = self.entry(c, ctx, Parent::Surface, &entry.path, step) {
                    entry.children.push(child);
                }
            } else if schema::element(&key.value).is_some() || self.is_definition(&key.value) {
                let help = if holds {
                    format!(
                        "things on a surface go in a scatter: `{owner} {{ scatter count=… {{ {} }} }}`",
                        key.value
                    )
                } else {
                    format!("list `{}` in the stack beside `{owner}`", key.value)
                };
                self.err(
                    dt_lang::codes::NESTING,
                    key.span,
                    format!("`{}` can't go inside `{owner}`", key.value),
                    "not a control here",
                )
                .help = Some(help);
            } else {
                self.unknown_control(&entry, &key);
            }
        }

        self.check_entry(&mut entry, ctx);
        Some(entry)
    }

    /// Resolve an entry's name to an element or a visible definition.
    fn callee(&mut self, what: &Spanned<String>, ctx: &Ctx) -> Option<Callee> {
        if let Some(l) = schema::element(&what.value) {
            return Some(Callee::Element(l));
        }
        if self.is_definition(&what.value) {
            return Some(Callee::Definition(what.value.clone()));
        }
        // a parameter written as an entry holds a thing or a layer
        if ctx.params.contains(&what.value) {
            return Some(Callee::Param(GeneratorParam {
                name: what.value.clone(),
                written: Vec::new(),
                ctx: Arc::new(ctx.clone()),
            }));
        }
        let name = what.value.as_str();
        let first = if let Some(h) = renamed(name) {
            Some(h.to_owned())
        } else {
            self.known.file_defining(name).map(|file| {
                match self.imported.values().any(|(_, f)| f.as_str() == file) {
                    true => format!("add it to the use line: `use \"{file}\" … {name}`"),
                    false => format!("it is in `{file}`: write `use \"{file}\" {name}` at the top"),
                }
            })
        };
        let mut names: Vec<&str> = crate::elements::ELEMENTS.iter().map(|l| l.name).collect();
        names.extend(self.local_names.iter().map(String::as_str));
        names.extend(self.imported.keys().map(String::as_str));
        let near = did_you_mean(name, names.iter().copied()).map(str::to_owned);
        self.err(
            dt_lang::codes::UNKNOWN_ELEMENT,
            what.span,
            format!("no element or definition is called `{name}`"),
            "unknown",
        )
        .advise(what.span, first, near.as_deref(), || {
            Some("a definition from another file needs `use \"…\" name` at the top".to_owned())
        });
        None
    }

    fn target(&self, entry: &Entry, name: &str) -> Target {
        match &entry.callee {
            Callee::Element(l) => l.control(name).map_or(Target::None, Target::Control),
            // what the parameter holds is known at the call: `control`
            // keeps what is written, and `bind_generator` types it then
            Callee::Param(_) => Target::None,
            Callee::Definition(d) => match self.def_info(d) {
                Some(info) => {
                    info.params
                        .iter()
                        .find(|p| p.name == name)
                        .map_or(Target::None, |p| {
                            if p.kind.is_generator() {
                                Target::Generator(p.kind)
                            } else {
                                Target::Param(p.control)
                            }
                        })
                }
                // A definition in a loop: accept its arguments untyped.
                None => Target::Param(None),
            },
        }
    }

    /// A place word on an entry; false when `key` is not one.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
    )]
    fn place_word(
        &mut self,
        entry: &mut Entry,
        key: &Spanned<String>,
        e: &KdlEntry,
        ctx: &Ctx,
        parent: Parent,
    ) -> bool {
        let owner = entry.what.value.clone();
        let span = value_span(e);
        let word = key.value.as_str();
        let dup = |this: &mut Self| {
            this.err(
                dt_lang::codes::DUPLICATE,
                key.span,
                format!("`{word}` is set twice"),
                "again here",
            );
        };
        match word {
            words::SEED => {
                if entry.place.seed.is_some() {
                    dup(self);
                } else {
                    match super::seed_value(e) {
                        Ok(i) => entry.place.seed = Some(Spanned { value: i, span }),
                        Err((msg, label)) => {
                            self.err(dt_lang::codes::SEED, span, msg, label);
                        }
                    }
                }
                return true;
            }
            words::ID => {
                match e.value().as_string() {
                    Some(s)
                        if s.is_empty()
                            || s.contains(['/', '\\'])
                            || s.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        self.err(
                            dt_lang::codes::ID,
                            span,
                            format!("`id` is a plain name, not `{s}`"),
                            "not a plain name",
                        )
                        .help = Some(
                            "write a name without slashes that is not just a number, such as `id=old-oak`: ids become parts of seed paths, beside entry counts"
                                .to_owned(),
                        );
                    }
                    Some(s) => {
                        entry.place.id = Some(Spanned {
                            value: s.to_owned(),
                            span,
                        })
                    }
                    None => {
                        self.err(dt_lang::codes::VALUE, span, "`id` is a name", "not a name");
                    }
                }
                return true;
            }
            words::WEIGHT => {
                if parent != Parent::Scatter {
                    self.err(
                        dt_lang::codes::WEIGHT,
                        key.span,
                        "`weight=` is a scatter child's share of the mix",
                        "not in a scatter",
                    )
                    .help = Some("write `scatter count=… { rock weight=2; pebble }`".to_owned());
                    return true;
                }
                match e.value() {
                    KdlValue::Integer(_) | KdlValue::Float(_) => {
                        let v = e.value().as_float().unwrap_or_else(|| {
                            e.value()
                                .as_integer()
                                .map_or(f64::NAN, |i| i.to_string().parse().unwrap_or(f64::NAN))
                        });
                        if v > 0.0 {
                            entry.place.weight = Some(Spanned { value: v, span });
                        } else {
                            self.err(
                                dt_lang::codes::WEIGHT,
                                span,
                                "a weight is above zero",
                                "not positive",
                            );
                        }
                    }
                    _ => {
                        self.err(
                            dt_lang::codes::WEIGHT,
                            span,
                            "a weight is a plain number",
                            "not a number",
                        )
                        .help = Some(
                            "write `weight=2`: twice as many as a child of weight 1".to_owned(),
                        );
                    }
                }
                return true;
            }
            _ => {}
        }
        let Some(pw) = PlaceWord::parse(word) else {
            return false;
        };
        // `at` is the word for every kind of place; an element takes the
        // kinds its schema lists
        let allowed = match entry.element() {
            Some(l) => match pw {
                PlaceWord::Side => l.place.contains(&PlaceWord::Side),
                _ => l.place.iter().any(|w| *w != PlaceWord::Side),
            },
            None => true,
        };
        if !allowed {
            let help = match pw {
                PlaceWord::Side => "side= is for things built along a drawn line".to_owned(),
                _ => format!("`{owner}` doesn't take `{word}=`"),
            };
            self.err(
                dt_lang::codes::PLACE,
                key.span,
                format!("`{owner}` doesn't take `{word}=`"),
                "not a place word for this",
            )
            .help = Some(help);
            return true;
        }
        if parent == Parent::Scatter && pw == PlaceWord::At {
            self.err(
                dt_lang::codes::PLACE,
                key.span,
                format!("a scattered {owner} is placed by its scatter"),
                "inside a scatter",
            )
            .help = Some(format!("put `{word}=` on the `scatter` line"));
            return true;
        }
        match pw {
            PlaceWord::Side => {
                if entry.place.side.is_some() {
                    dup(self);
                    return true;
                }
                match arg_value_str(e).and_then(Side::parse) {
                    Some(s) => entry.place.side = Some(Spanned { value: s, span }),
                    None => {
                        let s = arg_value_str(e).unwrap_or("");
                        self.err(
                            dt_lang::codes::CHOICE,
                            span,
                            format!("`side` can't be `{s}`"),
                            "not a side",
                        )
                        .suggest_or(
                            span,
                            did_you_mean(s, Side::WORDS),
                            || "`side` is left, right or centre, walking the line".to_owned(),
                        );
                    }
                }
            }
            PlaceWord::At => self.at_word(entry, key, e, ctx),
            PlaceWord::In | PlaceWord::Along | PlaceWord::Against => {}
        }
        true
    }

    /// One control, from a property (`height=35`) or a child line
    /// (`height 35`, `leaf length=4.5`, `ledges 1 height="40%"`).
    #[expect(
        clippy::too_many_arguments,
        reason = "the flat inputs of one maths step, named at the call"
    )]
    fn control(
        &mut self,
        entry: &mut Entry,
        key: Spanned<String>,
        form: Form,
        value: Option<&KdlEntry>,
        subs: &[KdlEntry],
        span: SourceSpan,
        ctx: &Ctx,
    ) {
        // a thing parameter's entry: which thing it is, is known at the call,
        // so keep what is written, for `bind_generator` to give to this function
        // again
        if let Callee::Param(p) = &mut entry.callee {
            p.written.push(Written {
                name: key,
                form,
                value: value.cloned(),
                subs: subs.to_vec(),
                span,
                ctx: Arc::new(ctx.clone()),
            });
            return;
        }
        let owner = entry.what.value.clone();
        if entry.controls.iter().any(|s| s.name.value == key.value) {
            self.err(
                dt_lang::codes::DUPLICATE,
                key.span,
                format!("`{}` is set twice", key.value),
                "again here",
            )
            .help = Some("a control is written once: on the line or on its own line".to_owned());
            return;
        }
        let (k, is_param) = match self.target(entry, &key.value) {
            Target::Control(k) => (Some(k), false),
            Target::Param(k) => (k, true),
            Target::Generator(want) => {
                if let Some(first) = subs.first() {
                    self.err(
                        dt_lang::codes::CONTROL_NODE,
                        first.span(),
                        format!("`{}` takes one value", key.value),
                        "here",
                    )
                    .help = Some(format!(
                        "a {} is passed by name: `{owner} {}={}`",
                        want.word(),
                        key.value,
                        example(want)
                    ));
                }
                let Some(e) = value else {
                    self.err(
                        dt_lang::codes::TYPE,
                        key.span,
                        format!(
                            "`{owner} {}` is {} and has no value",
                            key.value,
                            want.describe()
                        ),
                        format!("no {}", want.word()),
                    )
                    .help = Some(format!(
                        "name {}: `{owner} {}={}`",
                        want.describe(),
                        key.value,
                        example(want)
                    ));
                    return;
                };
                if let Some(v) = self.generator_value(e, &owner, &key.value, want, ctx) {
                    entry.controls.push(Setting {
                        name: key,
                        control: None,
                        value: Some(v),
                        subs: Vec::new(),
                        form,
                        span,
                        late: None,
                    });
                }
                return;
            }
            Target::None => {
                self.unknown_control(entry, &key);
                return;
            }
        };
        if let Some(k) = k
            && k.expert
            && ctx.in_tile
            && !is_param
        {
            self.err(
                dt_lang::codes::EXPERT,
                key.span,
                format!("`{owner} {}` is an expert control", k.name),
                "not in a tile",
            )
            .help = Some(format!(
                "a tile sets it with `crown branching=open|normal|dense`; `{}` is accepted in parts files, inside a `define`",
                k.name
            ));
            return;
        }
        if form == Form::Property && k.is_some_and(Control::is_group) {
            self.err(
                dt_lang::codes::GROUP,
                key.span,
                format!("`{}` is a group of controls", key.value),
                "a group",
            )
            .help = Some(format!(
                "write it on its own line inside the block: `{} {}=…`",
                key.value,
                k.and_then(|k| k.subs.first()).map_or("", |s| s.name)
            ));
            return;
        }
        let mut setting = Setting {
            name: key.clone(),
            control: k,
            value: None,
            subs: Vec::new(),
            form,
            span,
            late: None,
        };
        // The value.
        match (value, k) {
            (Some(e), Some(k)) if k.is_group() => {
                self.err(
                    dt_lang::codes::GROUP,
                    value_span(e),
                    format!("`{}` has no value of its own", k.name),
                    "a value",
                )
                .help = Some(format!(
                    "its controls are name=value on its own line: `{} {}=…`",
                    k.name,
                    k.subs.first().map_or("", |s| s.name)
                ));
                return;
            }
            (Some(e), Some(k)) => {
                setting.value = self.value(e, k, &owner, &key.value, ctx);
                if setting.value.is_none() {
                    return;
                }
            }
            (Some(e), None) => {
                // no control to type it by here: a name of the definition's
                // own parameters is passed on as that parameter; any other
                // value is kept as written too, for a thing parameter's
                // entry to give it a control at the call
                setting.value = self.untyped(e);
                match &setting.value {
                    Some(Spanned {
                        value: Value::Text(s),
                        span,
                    }) if ctx.params.iter().any(|p| p == s) => {
                        setting.value = Some(Spanned {
                            value: Value::Param(s.clone()),
                            span: *span,
                        });
                    }
                    Some(_) => setting.late = Some(Late::of(e, ctx)),
                    None => {}
                }
            }
            (None, Some(k)) if k.is_group() && form == Form::Property => {
                // unreachable: a property always has a value
            }
            (None, _) => {}
        }
        // Sub-controls.
        for e in subs {
            let Some(name) = e.name() else { continue };
            let sk = Spanned {
                value: name.value().to_owned(),
                span: name.span(),
            };
            let Some(sub) = k.and_then(|k| k.subs.iter().find(|s| s.name == sk.value)) else {
                let first = renamed(&sk.value).map(str::to_owned);
                let names: Vec<&str> =
                    k.map_or_else(Vec::new, |k| k.subs.iter().map(|s| s.name).collect());
                let near = did_you_mean(&sk.value, names.iter().copied());
                self.err(
                    dt_lang::codes::UNKNOWN_CONTROL,
                    sk.span,
                    format!("`{owner} {}` has no `{}`", key.value, sk.value),
                    "unknown",
                )
                .advise_old(sk.span, &sk.value, first, near, || match k {
                    Some(k) if !k.subs.is_empty() => {
                        Some(format!("`{}` takes: {}", k.name, names.join(", ")))
                    }
                    _ => Some(format!(
                        "`{}` takes one value: `{} value`",
                        key.value, key.value
                    )),
                });
                continue;
            };
            if setting.subs.iter().any(|s| s.name.value == sk.value) {
                self.err(
                    dt_lang::codes::DUPLICATE,
                    sk.span,
                    format!("`{}` is set twice", sk.value),
                    "again here",
                );
                continue;
            }
            let label = format!("{owner} {}", key.value);
            if let Some(v) = self.value(e, sub, &label, &sk.value, ctx) {
                setting.subs.push(Setting {
                    name: sk,
                    control: Some(sub),
                    value: Some(v),
                    subs: Vec::new(),
                    form: Form::Property,
                    span: e.span(),
                    late: None,
                });
            }
        }
        if setting.value.is_none() && setting.subs.is_empty() && subs.is_empty() {
            let help = k.map_or_else(
                || format!("write `{} value`", key.value),
                |k| {
                    if k.is_group() {
                        format!(
                            "write `{} {}=…`",
                            k.name,
                            k.subs.first().map_or("", |s| s.name)
                        )
                    } else {
                        describe(k)
                    }
                },
            );
            self.err(
                dt_lang::codes::CONTROL_NODE,
                key.span,
                format!("`{}` needs a value", key.value),
                "empty",
            )
            .help = Some(help);
            return;
        }
        if let Some(k) = k {
            for r in k.subs.iter().filter(|s| s.required) {
                if !setting.subs.iter().any(|s| s.name.value == r.name) {
                    self.err(
                        dt_lang::codes::MISSING_CONTROL,
                        key.span,
                        format!("`{} {}` needs `{}`", owner, k.name, r.name),
                        format!("no `{}`", r.name),
                    )
                    .help = Some(format!("add `{}=…`: {}", r.name, r.about));
                }
            }
        }
        entry.controls.push(setting);
    }

    fn unknown_control(&mut self, entry: &Entry, key: &Spanned<String>) {
        let owner = entry.what.value.as_str();
        let name = key.value.as_str();
        let thing = fits(self.callee_kind(&entry.callee), units::Kind::Thing);
        let (controls, params): (&[Control], Vec<&str>) = match &entry.callee {
            Callee::Element(l) => (l.controls, Vec::new()),
            Callee::Param(_) => (&[], Vec::new()),
            Callee::Definition(d) => (
                &[],
                self.def_info(d)
                    .map(|i| i.params.iter().map(|p| p.name.as_str()).collect())
                    .unwrap_or_default(),
            ),
        };
        let mut near: Option<String> = None;
        let mut takes: Option<String> = None;
        let help = if thing && SCATTER_ONLY.contains(&name) {
            Some(format!(
                "a {owner} is one {owner}; how many and how spread is a scatter's job: `scatter {name}=… {{ {owner} }}`"
            ))
        } else if owner == "tree" && (name == words::SIZE || name == "height") {
            Some("a tree's height is its `top`, mm above zero like every top, and `bottom` is the ground at its foot: `tree size=40` is `tree top=\"bottom + 40\"`".to_owned())
        } else if owner == "flower" && name == "radius" {
            Some("a flower's measure is its `size`, the width across the petals (twice the old radius): write `flower size=…`".to_owned())
        } else if name == "debug" {
            Some("debug switches are not tile controls; `dharmatiles build --show attractors` shows them".to_owned())
        } else if let Some(h) = renamed(name) {
            Some(h.to_owned())
        } else if let Some(g) = controls
            .iter()
            .find(|k| k.subs.iter().any(|s| s.name == name))
        {
            Some(format!(
                "`{name}` belongs on the `{}` line: `{} {name}=…`",
                g.name, g.name
            ))
        } else {
            let mut names: Vec<&str> = controls.iter().map(|k| k.name).collect();
            names.extend(params.iter().copied());
            if let Some(l) = entry.element() {
                for p in l.place.iter().map(|p| p.as_str()) {
                    if !names.contains(&p) {
                        names.push(p);
                    }
                }
            }
            names.push(words::SEED);
            near = did_you_mean(name, names.iter().copied()).map(str::to_owned);
            takes = Some(format!("`{owner}` takes: {}", names.join(", ")));
            None
        };
        let what = if matches!(entry.callee, Callee::Definition(_)) {
            "parameter"
        } else {
            "control"
        };
        self.err(
            dt_lang::codes::UNKNOWN_CONTROL,
            key.span,
            format!("`{owner}` has no {what} `{name}`"),
            format!("unknown {what}"),
        )
        .advise_old(key.span, name, help, near.as_deref(), || takes);
    }

    /// Checks across one entry's place words, controls and children.
    fn check_entry(&mut self, entry: &mut Entry, ctx: &Ctx) {
        let owner = entry.what.value.clone();
        let span = entry.what.span;
        let element = entry.element();
        if let Some(l) = element {
            // inside a definition a place word may come from the call
            // (`ruin-corner at=north`): the evaluator passes it down,
            // and the element refuses at build if it never arrives
            let needs = if ctx.in_tile { l.needs } else { &[] };
            for need in needs {
                let has = match need {
                    PlaceWord::Along => entry.place.along.is_some(),
                    PlaceWord::At => entry.place.at.is_some(),
                    PlaceWord::In => entry.place.region.is_some(),
                    PlaceWord::Against => entry.place.against.is_some(),
                    PlaceWord::Side => entry.place.side.is_some(),
                };
                if !has {
                    self.err(
                        dt_lang::codes::MISSING_CONTROL,
                        span,
                        format!("`{owner}` needs `{}=`", need.as_str()),
                        format!("this {owner} has no `{}=`", need.as_str()),
                    )
                    .help = Some(format!(
                        "write `{owner} at=north …`: an edge, an edge path such as \"north west\", or a drawn line"
                    ));
                }
            }
            for k in l.controls.iter().filter(|k| k.required) {
                if entry.control(k.name).is_none() {
                    self.err(
                        dt_lang::codes::MISSING_CONTROL,
                        span,
                        format!("`{owner}` needs `{}`", k.name),
                        format!("no `{}`", k.name),
                    )
                    .help = Some(format!("add `{}=…`: {}", k.name, k.about));
                }
            }
        }
        if let (Some(side), Some(along)) = (&entry.place.side, &entry.place.along)
            && matches!(along.value, Along::Edges(_))
        {
            self.err(
                dt_lang::codes::PLACE,
                side.span,
                "`side=` is for drawn lines",
                "along a tile edge",
            )
            .help = Some(format!(
                "on a tile edge a {owner} always stands inside the tile, its outer face flush with the edge"
            ));
        }
        if entry.place.side.is_some() && entry.place.along.is_none() {
            let s = entry.place.side.as_ref().map_or(span, |s| s.span);
            self.err(
                dt_lang::codes::PLACE,
                s,
                "`side=` needs `at=` a line",
                "no line",
            )
            .help = Some("write `at=trail side=left`".to_owned());
        }
        if let Callee::Definition(d) = &entry.callee
            && let Some(info) = self.def_info(d).cloned()
        {
            for p in info.params.iter().filter(|p| !p.has_default) {
                if entry.control(&p.name).is_none() {
                    self.err(
                        dt_lang::codes::MISSING_CONTROL,
                        span,
                        format!("`{owner}` needs `{}`", p.name),
                        format!("no `{}`", p.name),
                    )
                    .help = Some(format!(
                        "the definition has no default: write `{owner} {}=…`",
                        p.name
                    ));
                }
            }
        }
        if element.is_some_and(|l| l.role == Role::Scatter) {
            self.check_scatter(entry);
        }
        entry.kind = match (&entry.callee, element) {
            (_, Some(l)) if l.role == Role::Scatter => {
                Kind::of(entry.children.iter().map(|c| c.kind))
            }
            (_, Some(l)) => {
                if l.role == Role::Surface && entry.children.iter().any(|c| c.kind != Kind::Surface)
                {
                    Kind::Mixed
                } else {
                    l.kind()
                }
            }
            (Callee::Definition(d), None) => self.def_info(d).map_or(Kind::Mixed, |i| i.kind),
            (Callee::Element(l), None) => l.kind(),
            (Callee::Param(_), None) => Kind::Solid,
        };
    }

    // The advice below stays text: "how many" is a choice (`count` or `perSquare`), so no one edit mends it and no fix is made
    // (a fix is only made where the edit is unambiguous: `DocError::advise_old`, `suggest`).
    fn check_scatter(&mut self, entry: &Entry) {
        let span = entry.what.span;
        let count = entry.control("count");
        let per = entry.control("perSquare");
        match (count, per) {
            (None, None) => {
                self.err(
                    dt_lang::codes::SCATTER,
                    span,
                    "a scatter says how many",
                    "no count",
                )
                .help = Some("write `count=5` in total, or `perSquare=6`".to_owned());
            }
            (Some(_), Some(p)) => {
                self.err(
                    dt_lang::codes::SCATTER,
                    p.name.span,
                    "`count` and `perSquare` both say how many",
                    "a second count",
                )
                .help = Some("keep one: `count` is a total, `perSquare` a density".to_owned());
            }
            (Some(c), None) => {
                if let Some(Spanned {
                    value: Value::Num(n),
                    span,
                }) = &c.value
                    && n.expr.bounds(&|_| None) == Ok((1.0, 1.0))
                {
                    self.err(
                        dt_lang::codes::SCATTER,
                        *span,
                        "a scatter is for more than one",
                        "one",
                    )
                    .help = Some(
                        "place one thing with at= (or no at= for anywhere): `rock at=\"(1, 1)\"`"
                            .to_owned(),
                    );
                }
            }
            (None, Some(_)) => {}
        }
        if let Some(s) = entry.control("reach")
            && entry.place.against.is_none()
            && entry.place.along.is_none()
        {
            self.err(
                dt_lang::codes::SCATTER,
                s.name.span,
                "`reach` is how far out from a line or a foot: the scatter is `at=` neither",
                "no line or foot",
            )
            .help = Some(
                "write `scatter at=road reach=4 …`, or `scatter at=\"foot(cliff)\" reach=4 …`"
                    .to_owned(),
            );
        }
        if entry.children.is_empty() {
            self.err(
                dt_lang::codes::SCATTER,
                span,
                "a scatter's children say what it scatters",
                "nothing to scatter",
            )
            .help = Some("write `scatter count=5 { rock }`".to_owned());
        }
    }
}

/// A name that fits the kind, for a help.
fn example(want: units::Kind) -> &'static str {
    if want == units::Kind::Thing {
        "rock"
    } else {
        "dirt"
    }
}

/// A string value, bare or quoted.
fn arg_value_str(e: &KdlEntry) -> Option<&str> {
    e.value().as_string()
}

/// What a written value is, for a message: `a number`, `a yes or no`.
fn describe_kdl(e: &KdlEntry) -> &'static str {
    match e.value() {
        KdlValue::Bool(_) => "a yes or no",
        KdlValue::Integer(_) | KdlValue::Float(_) => "a number",
        KdlValue::Null => "nothing",
        KdlValue::String(_) => "a string",
    }
}

/// What a thing parameter stands for, once the caller has said.
#[derive(Clone, Copy)]
pub enum Callable<'a> {
    /// A built-in thing element.
    Element(&'static schema::Element),
    /// A definition of one thing.
    Definition(&'a Definition),
}

/// A thing parameter's entry as the entry of the thing it stands for: what
/// was written on it (`piece size=5`, `crown width=4`) read by the very
/// function that reads the same text written on that element or definition,
/// so a property is checked the same way wherever it is written: its kind
/// and unit, its range, its words, a group's sub-controls, a missing
/// required one, with the same code and words, at the text written.
///
/// `src` is the text the entry is in, and `known` what that file saw when it
/// was decoded.  Besides the entry it gives the controls the entry fills with
/// the definition's own parameters (`piece size=s` fills `size` with `s`), for
/// the caller to type the arguments of those parameters by, with
/// [`type_late`].
///
/// # Errors
/// Every mistake in what was written, as the decoder gives it.
pub fn bind_generator(
    entry: &Entry,
    param: &GeneratorParam,
    thing: Callable<'_>,
    src: &Src,
    known: &Known,
) -> Result<Bound, Vec<DocError>> {
    let mut d = Decoder::new(Arc::clone(src), known);
    let (callee, name) = match thing {
        Callable::Element(l) => (Callee::Element(l), l.name.to_owned()),
        Callable::Definition(def) => {
            d.local.insert(def.name.value.clone(), DefInfo::from(def));
            (
                Callee::Definition(def.name.value.clone()),
                def.name.value.clone(),
            )
        }
    };
    let mut bound = Entry {
        what: Spanned {
            value: name,
            span: entry.what.span,
        },
        callee,
        controls: Vec::new(),
        ..entry.clone()
    };
    for w in &param.written {
        d.control(
            &mut bound,
            w.name.clone(),
            w.form,
            w.value.as_ref(),
            &w.subs,
            w.span,
            &w.ctx,
        );
    }
    d.check_entry(&mut bound, &param.ctx);
    if d.errors.is_empty() {
        Ok(Bound {
            entry: bound,
            fills: d.param_controls.all().cloned().collect(),
        })
    } else {
        Err(d.errors)
    }
}

/// A thing parameter's entry bound to the thing it stands for.
#[derive(Debug)]
pub struct Bound {
    /// The entry, as written on the thing.
    pub entry: Entry,
    /// The controls it fills with the definition's own parameters.
    pub fills: Vec<Fill>,
}

/// A value written where its control was not known (a call's argument, a
/// parameter's default), typed now that a control is: by the very function
/// that types it written on the element, so it gets the same checks with the
/// same code, words and help, pointing at the text written.  `owner` and
/// `name` are what the control is of and as what it is written
/// ([`Fill::owner`], [`Fill::name`]); `src` is the text the value is in and
/// `known` what that file saw when it was decoded.
///
/// # Errors
/// Every mistake in the value, as the decoder gives it.
pub fn type_late(
    late: &Late,
    fill: &Fill,
    src: &Src,
    known: &Known,
) -> Result<Spanned<Value>, Vec<DocError>> {
    let mut d = Decoder::new(Arc::clone(src), known);
    let typed = d.value(
        &late.entry,
        fill.control,
        &fill.owner,
        &fill.name,
        &late.ctx,
    );
    match typed {
        Some(v) if d.errors.is_empty() => Ok(v),
        _ => Err(d.errors),
    }
}
