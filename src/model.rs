//! Core data types shared by every check, the scorer and the renderers.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use clap::ValueEnum;
use serde::Serialize;
use url::Url;

use crate::catalog;
use crate::fetcher::PageResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, ValueEnum)]
pub enum Category {
    #[serde(rename = "seo")]
    Seo,
    #[serde(rename = "social")]
    Social,
    #[value(name = "a11y")]
    #[serde(rename = "a11y")]
    Accessibility,
    #[value(name = "perf")]
    #[serde(rename = "perf")]
    Performance,
    #[value(name = "sec")]
    #[serde(rename = "sec")]
    Security,
    #[serde(rename = "assets")]
    Assets,
    #[serde(rename = "links")]
    Links,
    #[serde(rename = "html")]
    Html,
}

impl Category {
    #[cfg(test)]
    pub const ALL: [Category; 8] = [
        Category::Seo,
        Category::Social,
        Category::Accessibility,
        Category::Performance,
        Category::Security,
        Category::Assets,
        Category::Links,
        Category::Html,
    ];

    /// Short identifier, identical to the prefix of every check id in the category.
    #[cfg(test)]
    pub fn slug(self) -> &'static str {
        match self {
            Category::Seo => "seo",
            Category::Social => "social",
            Category::Accessibility => "a11y",
            Category::Performance => "perf",
            Category::Security => "sec",
            Category::Assets => "assets",
            Category::Links => "links",
            Category::Html => "html",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Seo => "SEO",
            Category::Social => "Social previews",
            Category::Accessibility => "Accessibility",
            Category::Performance => "Performance",
            Category::Security => "Security",
            Category::Assets => "Site files & icons",
            Category::Links => "Links & resources",
            Category::Html => "HTML hygiene",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    pub fix: &'static str,
}

/// Accumulates the outcome of checks: every check reports either `ok` or a finding.
#[derive(Debug, Default)]
pub struct Out {
    pub findings: Vec<Finding>,
    pub passed: Vec<&'static str>,
    pub notes: Vec<String>,
}

impl Out {
    pub fn ok(&mut self, id: &'static str) {
        assert!(catalog::find(id).is_some(), "unknown check id: {id}");
        self.passed.push(id);
    }

    pub fn flag(&mut self, id: &'static str, severity: Severity, message: impl Into<String>) {
        self.flag_with(id, severity, message, Vec::new());
    }

    pub fn flag_with(
        &mut self,
        id: &'static str,
        severity: Severity,
        message: impl Into<String>,
        details: Vec<String>,
    ) {
        let def = catalog::find(id).unwrap_or_else(|| panic!("unknown check id: {id}"));
        self.findings.push(Finding {
            id,
            category: def.category,
            severity,
            message: message.into(),
            details,
            fix: def.fix,
        });
    }

    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    pub fn merge(&mut self, other: Out) {
        self.findings.extend(other.findings);
        self.passed.extend(other.passed);
        self.notes.extend(other.notes);
    }

    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.findings.retain(|f| keep(f.id));
        self.passed.retain(|id| keep(id));
    }
}

#[derive(Debug, Serialize)]
pub struct CategoryScore {
    pub category: Category,
    pub score: u32,
    pub checks: usize,
    pub passed: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub url: String,
    pub final_url: String,
    pub status: u16,
    pub ttfb_ms: u64,
    pub total_ms: u64,
    pub html_bytes: usize,
    pub score: u32,
    pub categories: Vec<CategoryScore>,
    pub findings: Vec<Finding>,
    pub passed: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// How much a finding costs: a failed error check scores 0, a warning half, an info 90%.
fn penalty(s: Severity) -> f64 {
    match s {
        Severity::Error => 1.0,
        Severity::Warning => 0.5,
        Severity::Info => 0.1,
    }
}

#[derive(Default)]
struct Tally {
    checks: usize,
    points: f64,
    passed: usize,
    errors: usize,
    warnings: usize,
    infos: usize,
}

impl Report {
    pub fn build(requested: &Url, page: &PageResponse, out: Out) -> Report {
        let Out { mut findings, passed, notes } = out;
        findings.sort_by(|a, b| (a.category, Reverse(a.severity), a.id).cmp(&(b.category, Reverse(b.severity), b.id)));

        let mut tallies: BTreeMap<Category, Tally> = BTreeMap::new();
        for id in &passed {
            if let Some(def) = catalog::find(id) {
                let t = tallies.entry(def.category).or_default();
                t.checks += 1;
                t.passed += 1;
                t.points += 1.0;
            }
        }
        for f in &findings {
            let t = tallies.entry(f.category).or_default();
            t.checks += 1;
            t.points += 1.0 - penalty(f.severity);
            match f.severity {
                Severity::Error => t.errors += 1,
                Severity::Warning => t.warnings += 1,
                Severity::Info => t.infos += 1,
            }
        }

        let (all_checks, all_points) = tallies.values().fold((0usize, 0f64), |(c, p), t| (c + t.checks, p + t.points));
        let pct = |points: f64, checks: usize| -> u32 {
            if checks == 0 { 100 } else { (100.0 * points / checks as f64).round() as u32 }
        };

        let categories = tallies
            .into_iter()
            .map(|(category, t)| CategoryScore {
                category,
                score: pct(t.points, t.checks),
                checks: t.checks,
                passed: t.passed,
                errors: t.errors,
                warnings: t.warnings,
                infos: t.infos,
            })
            .collect();

        Report {
            url: requested.to_string(),
            final_url: page.final_url.to_string(),
            status: page.status,
            ttfb_ms: page.ttfb_ms as u64,
            total_ms: page.total_ms as u64,
            html_bytes: page.body.len(),
            score: pct(all_points, all_checks),
            categories,
            findings,
            passed,
            notes,
        }
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == severity).count()
    }

    pub fn total_checks(&self) -> usize {
        self.passed.len() + self.findings.len()
    }

    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}
