//! Pulls URLs out of the parsed page so the network checks can run without the DOM.

use std::collections::HashSet;

use url::Url;

use crate::checks::Ctx;
use crate::util::{resolve, sel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResKind {
    Image,
    Script,
    Style,
}

#[derive(Debug, Clone)]
pub struct Icon {
    pub href: Url,
    pub rel: String,
}

impl Icon {
    pub fn is_favicon(&self) -> bool {
        self.rel.to_ascii_lowercase().split_whitespace().any(|t| t == "icon")
    }

    pub fn is_apple(&self) -> bool {
        self.rel.to_ascii_lowercase().contains("apple-touch-icon")
    }
}

#[derive(Debug, Default)]
pub struct Extracted {
    /// Deduplicated http(s) links found in `<a href>`.
    pub links: Vec<Url>,
    pub resources: Vec<(ResKind, Url)>,
    pub icons: Vec<Icon>,
    pub manifest: Option<Url>,
    pub og_image: Option<Url>,
}

pub fn extract(c: &Ctx) -> Extracted {
    let mut x = Extracted::default();
    let mut seen = HashSet::new();

    for a in c.doc.select(&sel("a[href]")) {
        if let Some(u) = a.value().attr("href").and_then(|h| resolve(&c.base, h)) {
            if seen.insert(u.to_string()) {
                x.links.push(u);
            }
        }
    }

    let mut seen_res = HashSet::new();
    let mut add = |kind: ResKind, raw: Option<&str>| {
        if let Some(u) = raw.and_then(|r| resolve(&c.base, r)) {
            if seen_res.insert(u.to_string()) {
                x.resources.push((kind, u));
            }
        }
    };
    for el in c.doc.select(&sel("img[src]")) {
        add(ResKind::Image, el.value().attr("src"));
    }
    for el in c.doc.select(&sel("picture source[srcset]")) {
        let first =
            el.value().attr("srcset").and_then(|s| s.split(',').next()).and_then(|s| s.split_whitespace().next());
        add(ResKind::Image, first);
    }
    for el in c.doc.select(&sel("script[src]")) {
        add(ResKind::Script, el.value().attr("src"));
    }
    for el in c.doc.select(&sel("link[rel~=\"stylesheet\"][href]")) {
        add(ResKind::Style, el.value().attr("href"));
    }

    for el in c.doc.select(&sel("link[rel][href]")) {
        let rel = el.value().attr("rel").unwrap_or("");
        let Some(href) = el.value().attr("href").and_then(|h| resolve(&c.base, h)) else {
            continue;
        };
        let lower = rel.to_ascii_lowercase();
        if lower.split_whitespace().any(|t| t == "manifest") && x.manifest.is_none() {
            x.manifest = Some(href);
        } else if lower.split_whitespace().any(|t| t == "icon") || lower.contains("apple-touch-icon") {
            x.icons.push(Icon { href, rel: rel.to_string() });
        }
    }

    x.og_image = c.meta.get("og:image").and_then(|s| resolve(&c.base, s));
    x
}
