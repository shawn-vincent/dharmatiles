//! The command line's vocabulary, held to the registry (`dt_lang::words`): every command and long flag the clap
//! derive makes is a row, and every row is a command or flag the program has.

use std::collections::BTreeSet;

use clap::CommandFactory;
use dt_lang::words::{self, WordKind};

fn walk(c: &clap::Command, commands: &mut BTreeSet<String>, flags: &mut BTreeSet<String>) {
    for a in c.get_arguments() {
        if let Some(l) = a.get_long() {
            flags.insert(l.to_owned());
        }
    }
    for s in c.get_subcommands() {
        commands.insert(s.get_name().to_owned());
        walk(s, commands, flags);
    }
}

fn rows(kind: WordKind) -> BTreeSet<String> {
    words::all()
        .filter(|w| w.kind == kind)
        .map(|w| w.word.to_owned())
        .collect()
}

#[test]
fn every_command_and_flag_is_a_row_and_every_row_one() {
    let (mut commands, mut flags) = (BTreeSet::new(), BTreeSet::new());
    walk(&crate::cli::Cli::command(), &mut commands, &mut flags);
    // clap's own `help` command and `--help`, `--version` are not ours
    commands.remove("help");
    flags.remove("help");
    flags.remove("version");
    for (kind, made) in [(WordKind::Command, commands), (WordKind::Flag, flags)] {
        let rows = rows(kind);
        let new: Vec<_> = made.difference(&rows).collect();
        let unused: Vec<_> = rows.difference(&made).collect();
        assert!(
            new.is_empty(),
            "{kind:?}s the program has and dt-lang/src/words.rs lacks (add a row with its meaning): {new:?}"
        );
        assert!(
            unused.is_empty(),
            "{kind:?}s registered and not in the command line (remove the row, or add it): {unused:?}"
        );
    }
}
