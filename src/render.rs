//! Output: colored text, Markdown, JSON, plus the `list` and `explain` commands.

use std::fmt::Write;

use anyhow::{Result, bail};
use colored::{ColoredString, Colorize};
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{ContentArrangement, Table};

use crate::catalog::{self, CHECKS};
use crate::model::{Category, Finding, Report, Severity};
use crate::util::human_bytes;

fn symbol(s: Severity) -> ColoredString {
    match s {
        Severity::Error => "✖".red().bold(),
        Severity::Warning => "▲".yellow().bold(),
        Severity::Info => "ℹ".blue(),
    }
}

fn paint_score(score: u32, text: String) -> ColoredString {
    match score {
        90..=100 => text.green().bold(),
        70..=89 => text.yellow().bold(),
        _ => text.red().bold(),
    }
}

fn bar(score: u32) -> ColoredString {
    let filled = (score as usize * 20).div_ceil(100).min(20);
    paint_score(score, format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled)))
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 { format!("{n} {one}") } else { format!("{n} {one}s") }
}

pub fn text(r: &Report, verbose: bool, quiet: bool) -> String {
    let mut s = String::new();
    let _ = writeln!(s);
    let _ = writeln!(s, "  {} {}", "▸".magenta().bold(), r.url.bold().underline());
    let mut meta = format!("HTTP {} · {} ms to first byte · {}", r.status, r.ttfb_ms, human_bytes(r.html_bytes));
    if r.final_url != r.url {
        meta.push_str(&format!(" · redirected to {}", r.final_url));
    }
    let _ = writeln!(s, "    {}", meta.dimmed());

    for cat in &r.categories {
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "  {}  {} {}  {}",
            format!("{:<20}", cat.category.label()).bold(),
            bar(cat.score),
            paint_score(cat.score, format!("{:>3}", cat.score)),
            format!("{}/{} passed", cat.passed, cat.checks).dimmed()
        );
        for f in r.findings.iter().filter(|f| f.category == cat.category) {
            if quiet && f.severity == Severity::Info {
                continue;
            }
            finding(&mut s, f);
        }
        if verbose {
            for id in r.passed.iter().filter(|id| catalog::find(id).is_some_and(|c| c.category == cat.category)) {
                let title = catalog::find(id).map_or("", |c| c.title);
                let _ = writeln!(s, "    {} {}  {}", "✔".green(), format!("{id:<26}").dimmed(), title.dimmed());
            }
        }
    }

    let (e, w, i) = (r.count(Severity::Error), r.count(Severity::Warning), r.count(Severity::Info));
    let _ = writeln!(s);
    let _ = writeln!(s, "  {}", "─".repeat(64).dimmed());
    let _ = writeln!(
        s,
        "  {} checks · {} · {} · {} · {}",
        r.total_checks(),
        format!("{} passed", r.passed.len()).green(),
        plural(e, "error").red(),
        plural(w, "warning").yellow(),
        format!("{i} info").blue()
    );
    let verdict = if e > 0 {
        format!("Not ready to ship — fix {}.", plural(e, "error")).red().bold()
    } else if w > 0 {
        format!("Almost there — {} worth a look.", plural(w, "warning")).yellow().bold()
    } else {
        "All clear — ready to ship. ✨".green().bold()
    };
    let _ = writeln!(s, "  {}   {}", verdict, paint_score(r.score, format!("Score {}/100", r.score)));
    for note in &r.notes {
        let _ = writeln!(s, "  {} {}", "note:".dimmed(), note.dimmed());
    }
    if !verbose && !r.passed.is_empty() {
        let _ =
            writeln!(s, "  {}", "Run with -v to list passed checks, `metaweave explain <id>` for details.".dimmed());
    }
    s
}

fn finding(s: &mut String, f: &Finding) {
    let _ = writeln!(s, "    {} {}  {}", symbol(f.severity), format!("{:<26}", f.id).dimmed(), f.message);
    for d in &f.details {
        let _ = writeln!(s, "        {} {}", "·".dimmed(), d.dimmed());
    }
    let _ = writeln!(s, "        {} {}", "→".cyan(), f.fix.cyan());
}

pub fn markdown(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "## metaweave report — {}\n", r.url);
    let _ = writeln!(
        s,
        "**Score: {}/100** · {} · {} · {} info · HTTP {} · {} ms TTFB\n",
        r.score,
        plural(r.count(Severity::Error), "error"),
        plural(r.count(Severity::Warning), "warning"),
        r.count(Severity::Info),
        r.status,
        r.ttfb_ms
    );
    let _ = writeln!(s, "| Category | Score | Passed | Errors | Warnings | Info |\n|---|---:|---:|---:|---:|---:|");
    for c in &r.categories {
        let _ = writeln!(
            s,
            "| {} | {} | {}/{} | {} | {} | {} |",
            c.category.label(),
            c.score,
            c.passed,
            c.checks,
            c.errors,
            c.warnings,
            c.infos
        );
    }
    for c in &r.categories {
        let items: Vec<&Finding> = r.findings.iter().filter(|f| f.category == c.category).collect();
        if items.is_empty() {
            continue;
        }
        let _ = writeln!(s, "\n### {}\n", c.category.label());
        for f in items {
            let icon = match f.severity {
                Severity::Error => "❌",
                Severity::Warning => "⚠️",
                Severity::Info => "ℹ️",
            };
            let _ = writeln!(s, "- {icon} **`{}`** — {}", f.id, f.message);
            for d in &f.details {
                let _ = writeln!(s, "  - {d}");
            }
            let _ = writeln!(s, "  - *Fix:* {}", f.fix);
        }
    }
    for note in &r.notes {
        let _ = writeln!(s, "\n> {note}");
    }
    s
}

pub fn json(reports: &[Report]) -> Result<String> {
    Ok(if let [one] = reports { serde_json::to_string_pretty(one)? } else { serde_json::to_string_pretty(reports)? })
}

pub fn list(category: Option<Category>) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED).set_content_arrangement(ContentArrangement::Dynamic).set_header([
        "ID",
        "Category",
        "Checks that…",
        "Net",
    ]);
    let mut count = 0;
    for c in CHECKS.iter().filter(|c| category.is_none_or(|cat| c.category == cat)) {
        table.add_row([c.id, c.category.label(), c.title, if c.network { "yes" } else { "" }]);
        count += 1;
    }
    println!("{table}");
    println!(
        "{} {}  ·  {}",
        count.to_string().bold(),
        "checks".bold(),
        "“Net” checks make extra HTTP requests. Details: metaweave explain <id>".dimmed()
    );
}

pub fn explain(query: &str) -> Result<()> {
    let exact = catalog::find(query);
    let by_suffix: Vec<_> = CHECKS.iter().filter(|c| c.id.ends_with(&format!("/{query}"))).collect();
    let check = match (exact, by_suffix.as_slice()) {
        (Some(c), _) => c,
        (None, [one]) => one,
        (None, []) => {
            let guesses: Vec<_> = CHECKS.iter().filter(|c| c.id.contains(query)).map(|c| c.id).take(6).collect();
            if guesses.is_empty() {
                bail!("unknown check '{query}' — run `metaweave list` to see all ids");
            }
            bail!("unknown check '{query}' — did you mean: {}?", guesses.join(", "));
        }
        (None, many) => bail!("'{query}' is ambiguous: {}", many.iter().map(|c| c.id).collect::<Vec<_>>().join(", ")),
    };
    println!();
    println!("  {}  {}", check.id.bold().magenta(), format!("· {}", check.category.label()).dimmed());
    println!("  {}", check.title.bold());
    println!("\n  {}\n    {}", "Why it matters".underline(), check.why);
    println!("\n  {}\n    {}", "How to fix".underline(), check.fix.cyan());
    if check.network {
        println!("\n  {}", "This check makes additional HTTP requests.".dimmed());
    }
    println!("\n  {} metaweave <url> --ignore {}\n", "Silence it:".dimmed(), check.id);
    Ok(())
}
