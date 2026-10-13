mod audit;
mod catalog;
mod checks;
mod cli;
mod extract;
mod fetcher;
mod filter;
mod imgdim;
mod model;
mod render;
mod ui;
mod util;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::Colorize;

use cli::{CheckArgs, Cli, Command, Format};
use fetcher::{FetchOptions, Fetcher};
use filter::Filter;
use model::Report;

#[tokio::main]
async fn main() -> ExitCode {
    reset_sigpipe();
    let cli = Cli::parse();
    setup_color(cli.no_color);
    match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e:#}", "error:".red().bold());
            ExitCode::from(2)
        }
    }
}

/// Rust ignores SIGPIPE by default, which turns `metaweave … | head` into a panic.
fn reset_sigpipe() {
    #[cfg(unix)]
    // SAFETY: restoring the default disposition of a signal is always sound.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn setup_color(no_color: bool) {
    let forced = std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0");
    if no_color {
        colored::control::set_override(false);
    } else if forced {
        colored::control::set_override(true);
    } else if !std::io::stdout().is_terminal() {
        colored::control::set_override(false);
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Some(Command::List { category }) => {
            render::list(category);
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Explain { id }) => {
            render::explain(&id)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Completions { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "metaweave", &mut std::io::stdout());
            Ok(ExitCode::SUCCESS)
        }
        None => check(cli.check).await,
    }
}

async fn check(args: CheckArgs) -> Result<ExitCode> {
    ui::init(args.format == Format::Text && !args.quiet && std::io::stderr().is_terminal());
    let filter = Filter::new(&args.only, &args.skip, &args.ignore);
    let fetcher = Fetcher::new(&FetchOptions {
        timeout: Duration::from_secs(args.timeout),
        user_agent: args.user_agent.clone().unwrap_or_else(|| format!("metaweave/{}", env!("CARGO_PKG_VERSION"))),
        insecure: args.insecure,
    })?;

    let mut seen_origins = HashSet::new();
    let mut reports: Vec<Report> = Vec::new();
    let mut failed = false;
    for raw in &args.urls {
        let url = match util::normalize_url(raw) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("{} {e:#}", "error:".red().bold());
                failed = true;
                continue;
            }
        };
        // Once-per-site checks (robots.txt, sitemap, 404 page…) run for the first URL of each origin.
        let site = seen_origins.insert(url.origin().ascii_serialization());
        match audit::run(&fetcher, &url, &filter, site, args.max_links).await {
            Ok(report) => reports.push(report),
            Err(e) => {
                eprintln!("{} {url}: {e:#}", "error:".red().bold());
                failed = true;
            }
        }
    }

    match args.format {
        Format::Text => reports.iter().for_each(|r| print!("{}", render::text(r, args.verbose, args.quiet))),
        Format::Markdown => println!("{}", reports.iter().map(render::markdown).collect::<Vec<_>>().join("\n---\n\n")),
        Format::Json if !reports.is_empty() => println!("{}", render::json(&reports)?),
        Format::Json => {}
    }

    if failed {
        return Ok(ExitCode::from(2));
    }
    let breached =
        args.fail_on.threshold().is_some_and(|min| reports.iter().any(|r| r.worst().is_some_and(|w| w >= min)));
    Ok(if breached { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
