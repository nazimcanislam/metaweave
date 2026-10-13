//! `--only`, `--skip` and `--ignore` handling.

use colored::Colorize;

use crate::catalog::{self, CHECKS};
use crate::model::Category;

#[derive(Debug, Default)]
pub struct Filter {
    only: Vec<Category>,
    skip: Vec<Category>,
    ignore: Vec<String>,
}

impl Filter {
    pub fn new(only: &[Category], skip: &[Category], ignore: &[String]) -> Filter {
        for pattern in ignore {
            let matches_any = CHECKS.iter().any(|c| matches(pattern, c.id));
            if !matches_any {
                eprintln!("{} '{pattern}' does not match any check (see `metaweave list`)", "warning:".yellow().bold());
            }
        }
        Filter { only: only.to_vec(), skip: skip.to_vec(), ignore: ignore.to_vec() }
    }

    pub fn category_enabled(&self, c: Category) -> bool {
        (self.only.is_empty() || self.only.contains(&c)) && !self.skip.contains(&c)
    }

    /// Whether the check should run and be reported.
    pub fn wants(&self, id: &str) -> bool {
        let Some(def) = catalog::find(id) else {
            return false;
        };
        self.category_enabled(def.category) && !self.ignore.iter().any(|p| matches(p, id))
    }

    pub fn wants_any(&self, ids: &[&str]) -> bool {
        ids.iter().any(|id| self.wants(id))
    }
}

/// Patterns are exact ids or prefixes ending in `*` (e.g. `sec/*`).
fn matches(pattern: &str, id: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => id.starts_with(prefix),
        None => pattern == id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_categories() {
        let f = Filter::new(&[], &[Category::Links], &["sec/*".to_string(), "seo/title".to_string()]);
        assert!(!f.wants("sec/hsts"));
        assert!(!f.wants("seo/title"));
        assert!(!f.wants("links/internal"));
        assert!(f.wants("seo/h1"));
        let only = Filter::new(&[Category::Seo], &[], &[]);
        assert!(only.wants("seo/h1") && !only.wants("a11y/lang"));
    }
}
