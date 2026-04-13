use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod data;
mod output;
mod rda;
mod search;
mod types;
mod update;

use cli::{Cli, Commands};
use output::Output;
use types::Locale;

fn main() {
    let cli = Cli::parse();
    let locale: Locale = cli.locale.into();
    let out = Output {
        force_compact: cli.json,
        fields: cli
            .fields
            .as_ref()
            .map(|s| s.split(',').map(|x| x.trim().to_string()).collect()),
    };

    let result = dispatch(&cli.command, locale, &out);
    if let Err(e) = result {
        output::emit_error(&e.to_string());
        std::process::exit(1);
    }
}

fn dispatch(command: &Commands, locale: Locale, out: &Output) -> Result<()> {
    match command {
        Commands::Foods { command } => commands::foods::run(command, locale, out),
        Commands::FoodGroups { command } => commands::food_groups::run(command, locale, out),
        Commands::Nutrients { command } => commands::nutrients::run(command, locale, out),
        Commands::Sources { command } => commands::sources::run(command, locale, out),
        Commands::Langual { command } => commands::langual::run(command, out),
        Commands::Rda { command } => commands::rda::run(command, locale, out),
        Commands::Describe { check_upstream } => commands::describe::run(*check_upstream, out),
        Commands::Update { check_only } => update::run(update::UpdateArgs {
            check_only: *check_only,
        }),
    }
}
