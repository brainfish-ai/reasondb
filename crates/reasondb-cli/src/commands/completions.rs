//! Shell completions generation

use clap::Command;
use clap_complete::{generate, Shell};
use std::io;

pub fn run(shell: Shell) {
    let mut cmd = build_cli();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
}

fn build_cli() -> Command {
    Command::new("reasondb")
        .subcommand(Command::new("serve"))
        .subcommand(Command::new("query"))
        .subcommand(
            Command::new("tables")
                .subcommand(Command::new("list"))
                .subcommand(Command::new("create"))
                .subcommand(Command::new("get"))
                .subcommand(Command::new("delete")),
        )
        .subcommand(
            Command::new("docs")
                .subcommand(Command::new("list"))
                .subcommand(Command::new("get"))
                .subcommand(Command::new("delete"))
                .subcommand(Command::new("ingest")),
        )
        .subcommand(Command::new("import"))
        .subcommand(Command::new("export"))
        .subcommand(Command::new("search"))
        .subcommand(Command::new("health"))
        .subcommand(Command::new("completions"))
}
