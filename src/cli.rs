use crate::types::Locale;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "mvt",
    version,
    about = "Agent-friendly CLI for the Norwegian Food Composition Table (matvaretabellen.no)"
)]
pub struct Cli {
    /// Force compact JSON (overrides TTY pretty-print).
    #[arg(long, global = true)]
    pub json: bool,

    /// Comma-separated list of field paths to keep. Supports dotted paths:
    /// `--fields foodName,energy.quantity`.
    #[arg(long, global = true, value_name = "LIST")]
    pub fields: Option<String>,

    /// Dataset locale. Default `en`, override via `--locale nb` or `$MVT_LOCALE`.
    #[arg(long, global = true, env = "MVT_LOCALE", default_value = "en")]
    pub locale: LocaleArg,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum LocaleArg {
    Nb,
    En,
}

impl From<LocaleArg> for Locale {
    fn from(v: LocaleArg) -> Self {
        match v {
            LocaleArg::Nb => Locale::Nb,
            LocaleArg::En => Locale::En,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Food records.
    Foods {
        #[command(subcommand)]
        command: FoodsCmd,
    },
    /// Food-group hierarchy (e.g. "Dairy", "Dairy/Cheese").
    FoodGroups {
        #[command(subcommand)]
        command: FoodGroupsCmd,
    },
    /// Nutrient definitions (id, unit, EuroFIR mapping).
    Nutrients {
        #[command(subcommand)]
        command: NutrientsCmd,
    },
    /// Data-source codes used by constituents.
    Sources {
        #[command(subcommand)]
        command: SourcesCmd,
    },
    /// LanguaL thesaurus codes (language-independent).
    Langual {
        #[command(subcommand)]
        command: LangualCmd,
    },
    /// Recommended-daily-allowance profiles.
    Rda {
        #[command(subcommand)]
        command: RdaCmd,
    },
    /// Report embedded schema + data version + resource counts.
    Describe {
        /// HEAD the upstream endpoints to report data drift.
        #[arg(long)]
        check_upstream: bool,
    },
    /// Self-update from GitHub releases.
    Update {
        /// Check for a newer version without downloading.
        #[arg(long)]
        check_only: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum FoodsCmd {
    /// List all foods in the locale.
    List,
    /// Get a single food by `foodId`.
    Get { id: String },
    /// Search foods by name/keyword (edge-ngram + diacritic-folded).
    Search {
        query: String,
        /// Limit the number of results.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Per-nutrient RDA coverage for a food.
    Rda {
        id: String,
        /// RDA profile id (default: first profile in the dataset).
        #[arg(long)]
        profile: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FoodGroupsCmd {
    List,
    Get { id: String },
}

#[derive(Subcommand, Debug)]
pub enum NutrientsCmd {
    List,
    Get { id: String },
}

#[derive(Subcommand, Debug)]
pub enum SourcesCmd {
    List,
    Get { id: String },
}

#[derive(Subcommand, Debug)]
pub enum LangualCmd {
    List,
    Get { id: String },
}

#[derive(Subcommand, Debug)]
pub enum RdaCmd {
    List,
    Get { id: String },
}
