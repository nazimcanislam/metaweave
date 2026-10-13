//! Command-line interface definition.

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use crate::model::{Category, Severity};

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Magenta.on_default() | Effects::BOLD)
        .usage(AnsiColor::Magenta.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Green.on_default())
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

const EXAMPLES: &str = "\
Examples:
  metaweave https://example.com                 Run every check against a page
  metaweave localhost:3000                      Check your dev server (http is assumed)
  metaweave example.com example.com/pricing     Check several pages in one run
  metaweave example.com --only seo,social       Run only some categories
  metaweave example.com --skip links            Skip the slow link crawl
  metaweave example.com -i 'sec/*' -i seo/h1    Silence checks (ids or prefix wildcards)
  metaweave example.com -f json > report.json   Machine-readable output
  metaweave example.com --fail-on warning       Exit 1 on warnings too (CI)
  metaweave list                                Show all checks
  metaweave explain social/og-image             Why a check exists and how to fix it

Exit codes: 0 = passed, 1 = findings at or above --fail-on, 2 = could not run.";

#[derive(Parser, Debug)]
#[command(
    name = "metaweave",
    version,
    about = "Pre-production website checker: SEO, social previews, accessibility, performance, security and more.",
    long_about = "metaweave fetches a page and runs 70+ checks that Lighthouse does not cover — Open Graph \
tags and images, favicons, robots.txt, sitemap, canonical URLs, security headers, broken links and more — \
so you catch them before launch instead of after.",
    after_help = EXAMPLES,
    styles = styles(),
    propagate_version = true,
    arg_required_else_help = true,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub check: CheckArgs,

    /// Disable colored output (also honours NO_COLOR)
    #[arg(long, global = true, env = "NO_COLOR")]
    pub no_color: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List every check
    List {
        /// Only show checks of this category
        #[arg(short, long, value_enum)]
        category: Option<Category>,
    },
    /// Explain a check: why it matters and how to fix it
    Explain {
        /// Check id, e.g. social/og-image (a unique short name like og-image also works)
        id: String,
    },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Colored, human-friendly report
    Text,
    /// Machine-readable JSON
    Json,
    /// Markdown (great for PR comments and $GITHUB_STEP_SUMMARY)
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FailOn {
    Error,
    Warning,
    Info,
    /// Always exit 0, whatever is found
    Never,
}

impl FailOn {
    pub fn threshold(self) -> Option<Severity> {
        match self {
            FailOn::Error => Some(Severity::Error),
            FailOn::Warning => Some(Severity::Warning),
            FailOn::Info => Some(Severity::Info),
            FailOn::Never => None,
        }
    }
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Page(s) to check — the scheme is optional (example.com, localhost:3000)
    #[arg(value_name = "URL", required = true)]
    pub urls: Vec<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Run only these categories (comma separated)
    #[arg(long, value_enum, value_delimiter = ',', value_name = "CATEGORY")]
    pub only: Vec<Category>,

    /// Skip these categories (comma separated)
    #[arg(long, value_enum, value_delimiter = ',', value_name = "CATEGORY")]
    pub skip: Vec<Category>,

    /// Ignore checks by id; `sec/*` style prefixes work. Repeatable.
    #[arg(short, long, value_delimiter = ',', value_name = "ID")]
    pub ignore: Vec<String>,

    /// Exit with code 1 when a finding of at least this severity exists
    #[arg(long, value_enum, default_value_t = FailOn::Error)]
    pub fail_on: FailOn,

    /// Per-request timeout in seconds
    #[arg(long, default_value_t = 15, value_name = "SECS")]
    pub timeout: u64,

    /// Maximum number of links to verify (internal links first)
    #[arg(long, default_value_t = 100, value_name = "N")]
    pub max_links: usize,

    /// Custom User-Agent header
    #[arg(long, value_name = "UA")]
    pub user_agent: Option<String>,

    /// Accept invalid TLS certificates (self-signed dev/staging servers)
    #[arg(long)]
    pub insecure: bool,

    /// Show only errors and warnings
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Also list the checks that passed
    #[arg(short, long)]
    pub verbose: bool,
}
