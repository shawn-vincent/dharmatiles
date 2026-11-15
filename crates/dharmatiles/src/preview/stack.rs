//! The preview's stack line: each entry named by what it places, with a
//! link to that thing's page on the documentation site, and where it is
//! written (`file`, byte `span`, 1-based `lines`) so the editor can light
//! its text when its geometry is picked.  An entry is a [`StackItem`]: one
//! type for the check's answer, the plan and every reader (its TypeScript
//! type is generated from it).

pub use dt_service::stack::{
    StackItem, docs, items,
};

#[cfg(test)]
mod tests {

    use dt_eval::{Files, Options};

    fn labels(path: &str, system: &str) -> Vec<String> {
        let system: dt_lang::System = system.parse().unwrap();
        let c = dt_eval::check(&Files, path, &Options::default()).unwrap();
        super::items(c.tile.as_ref().unwrap(), c.solved.as_ref(), system)
            .unwrap()
            .iter()
            .map(|v| v.label.clone())
            .collect()
    }

    #[test]
    fn the_cliff_names_what_each_entry_places() {
        assert_eq!(
            labels("lib:tiles/cliffs/2x1-cliff.dtile", "db"),
            [
                "dirt",
                "pebbles in dirt",
                "cliff in crag",
                "dirt in crag",
                "stones at the cliff",
                "boulders at the cliff",
                "DungeonBlocks base",
            ]
        );
    }

    #[test]
    fn an_entry_that_pins_its_seed_says_so_and_the_others_say_null() {
        let c = dt_eval::check(
            &Files,
            "lib:tests/walls/1x1-wall-stone+grass.dtile",
            &Options::default(),
        )
        .unwrap();
        let items = super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Db,
        )
        .unwrap();
        let seed = |what: &str| items.iter().find(|v| v.what == what).unwrap().seed;
        assert_eq!(seed("wall"), Some(11));
        assert_eq!(seed("dirt"), None);
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
    )]
    fn every_library_and_test_tile_describes_its_stack_without_a_bad_span() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
        let mut paths = Vec::new();
        for group in ["tiles", "tests"] {
            let mut dirs = vec![root.join(group)];
            while let Some(d) = dirs.pop() {
                for e in std::fs::read_dir(d).unwrap() {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        dirs.push(p);
                    } else if p.extension().is_some_and(|x| x == "dtile") {
                        let rel = p
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        paths.push(format!("lib:{rel}"));
                    }
                }
            }
        }
        assert!(paths.len() > 50, "found the library");
        for path in paths {
            // a tile with a mistake has no stack to describe
            if let Ok(c) = dt_eval::check(&Files, &path, &Options::default())
                && let Some(t) = c.tile.as_ref()
            {
                super::items(t, None, dt_lang::System::Db)
                    .unwrap_or_else(|e| panic!("{path}: {e}"));
            }
        }
    }

    /// The text each cloud of each entry of `text` covers, by entry key.
    fn clouds_of(text: &str) -> Vec<(String, Vec<(String, String)>)> {
        let m = dt_eval::Memory([("t.dtile".to_owned(), text.to_owned())].into());
        let c = dt_eval::check(&m, "t.dtile", &Options::default()).unwrap();
        super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Db,
        )
        .unwrap()
        .iter()
        .filter(|v| v.key != "tile/connector")
        .map(|v| {
            let cs = v
                .clouds
                .iter()
                .map(|c| {
                    let [o, l] = c.span;
                    let kind = match c.pin {
                        Some(k) => format!("pin {k}"),
                        None => c.kind.clone(),
                    };
                    (kind, text[o..o + l].to_owned())
                })
                .collect();
            (v.key.clone(), cs)
        })
        .collect()
    }

    #[test]
    fn a_cloud_is_what_the_parser_read_not_what_the_text_looks_like() {
        // a `top=` in a comment, a block comment, a raw string and a hidden pin are no clouds
        let text = "dharmatiles version=\"1.0.0\"\n\
            // dirt top=\"$groundHeight\"\n\
            tile size=\"1x1\" seed=3 {\n\
            about #\"top=\"1dmm\" and pin top=\"2dmm\" in a raw string\"#\n\
            stack {\n base\n\
            dirt top=\"$groundHeight + 1dmm\" {\n\
            /- pin top=\"$groundHeight + 9dmm\" at=center\n\
            pin top=\"$groundHeight + 4dmm\" at=center\n\
            /* pin top=\"$groundHeight\" at=center */\n\
            pin top=\"$groundHeight + 2dmm\" at=southCenter\n\
            }\n\
            scatter count=2 { rock size=\"9dmm\" top=\"$bottom + high(3dmm .. 9dmm)\" }\n\
            grass\n }\n}\n";
        let got = clouds_of(text);
        let s = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(got[0].0, "tile/dirt/0");
        assert_eq!(
            got[0].1,
            [
                s("top", "top=\"$groundHeight + 1dmm\""),
                s("pin 0", "pin top=\"$groundHeight + 4dmm\" at=center"),
                s("pin 1", "pin top=\"$groundHeight + 2dmm\" at=southCenter"),
            ]
        );
        assert_eq!(got[1].0, "tile/scatter/0");
        assert_eq!(got[1].1, [s("top", "top=\"$bottom + high(3dmm .. 9dmm)\"")]);
        assert!(got[2].1.is_empty(), "a grass has no cloud");
    }

    #[test]
    fn a_top_a_call_gives_is_the_trees_cloud_where_the_call_writes_it() {
        let m = dt_eval::Memory(
            [(
                "t.dtile".to_owned(),
                "dharmatiles version=\"1.0.0\"\n\
                 define grove { stack { tree } }\n\
                 tile size=\"1x1\" seed=3 { stack { base\n dirt\n grove at=\"(0.5sq, 0.5sq)\" top = \"$bottom + 30dmm\" } }\n"
                    .to_owned(),
            )]
            .into(),
        );
        let c = dt_eval::check(&m, "t.dtile", &Options::default()).unwrap();
        let items = super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Db,
        )
        .unwrap();
        let tree = items.iter().find(|v| v.what == "tree").unwrap();
        let cloud = &tree.clouds[0];
        let text = c.file.src.inner();
        let [o, l] = cloud.span;
        assert_eq!(&text[o..o + l], "top = \"$bottom + 30dmm\"");
    }

    #[test]
    fn a_bodys_top_is_a_control_the_sheet_can_show() {
        let m = dt_eval::Memory(
            [(
                "t.dtile".to_owned(),
                "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=3 { stack { base\n dirt top=\"$groundHeight + 1dmm\" } }\n"
                    .to_owned(),
            )]
            .into(),
        );
        let c = dt_eval::check(&m, "t.dtile", &Options::default()).unwrap();
        let items = super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Db,
        )
        .unwrap();
        let top = items[0].controls.iter().find(|c| c.name == "top").unwrap();
        assert_eq!(top.text, "top=\"$groundHeight + 1dmm\"");
    }

    /// A word written stands for numbers (`height full` for the wall top),
    /// which are no controls of their own: the sheet lists what is written
    /// and nothing it derives.  What is not written comes with the value a
    /// build uses and where it is from.
    #[test]
    fn a_written_word_gives_one_row_and_the_rest_come_with_what_a_build_uses() {
        let m = dt_eval::Memory(
            [(
                "t.dtile".to_owned(),
                "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=3 { stack { base\n dirt\n wall at=\"north\" style=brick { height full } } }\n"
                    .to_owned(),
            )]
            .into(),
        );
        let c = dt_eval::check(&m, "t.dtile", &Options::default()).unwrap();
        let items = super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Db,
        )
        .unwrap();
        let wall = items.iter().find(|v| v.what == "wall").unwrap();
        let written: Vec<&str> = wall.controls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(written, ["style", "height"], "no `height.height`");
        let of = |item: &super::StackItem, control: &str| -> (String, String) {
            let d = item
                .defaults
                .iter()
                .find(|d| d.control == control)
                .unwrap_or_else(|| panic!("no default for {control}"));
            (d.text.clone(), d.from.clone())
        };
        assert_eq!(
            of(wall, "stones/height"),
            ("2.5dmm .. 3dmm".to_owned(), "style brick".to_owned())
        );
        assert_eq!(
            of(wall, "thickness"),
            ("7dmm".to_owned(), "default".to_owned())
        );
        // a body's top is the band the solver used, from the library's edges
        let dirt = items.iter().find(|v| v.what == "dirt").unwrap();
        let (top, from) = of(dirt, "top");
        assert!(top.contains("$groundHeight"), "{top}");
        assert_eq!(from, "parts/edges");
        // what is written is not in the list
        assert!(wall.defaults.iter().all(|d| d.control != "style"));
    }

    #[test]
    fn every_link_is_on_the_docs_site() {
        let c = dt_eval::check(
            &Files,
            "lib:tiles/cliffs/2x1-cliff.dtile",
            &Options::default(),
        )
        .unwrap();
        for v in super::items(
            c.tile.as_ref().unwrap(),
            c.solved.as_ref(),
            dt_lang::System::Ol,
        )
        .unwrap()
        {
            assert!(v.href.starts_with(&super::docs()));
        }
    }
}
