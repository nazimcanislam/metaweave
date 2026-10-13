//! Checks. Static checks inspect the fetched HTML and headers; `net` performs extra requests.

pub mod a11y;
pub mod html;
pub mod net;
pub mod perf;
pub mod security;
pub mod seo;
pub mod social;

use std::collections::HashMap;

use scraper::Html;
use url::Url;

use crate::fetcher::PageResponse;
use crate::model::Out;
use crate::util::{is_local, sel};

/// `<meta name|property=… content=…>` lookup table (keys are lower-cased).
pub struct Meta(HashMap<String, Vec<String>>);

impl Meta {
    fn from_doc(doc: &Html) -> Meta {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for el in doc.select(&sel("meta")) {
            let v = el.value();
            let Some(content) = v.attr("content") else {
                continue;
            };
            for key in ["name", "property"] {
                if let Some(k) = v.attr(key) {
                    map.entry(k.trim().to_ascii_lowercase()).or_default().push(content.trim().to_string());
                }
            }
        }
        Meta(map)
    }

    /// First non-empty value for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.iter().map(String::as_str).find(|s| !s.is_empty())
    }

    /// The tag exists (even if its content is empty).
    pub fn present(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
}

/// Everything a static check may look at.
pub struct Ctx<'a> {
    pub page: &'a PageResponse,
    pub doc: &'a Html,
    pub meta: Meta,
    /// The page is served from localhost / a private address (a dev server).
    pub local: bool,
    /// Base for resolving relative URLs (honours `<base href>`).
    pub base: Url,
}

impl<'a> Ctx<'a> {
    pub fn new(page: &'a PageResponse, doc: &'a Html) -> Ctx<'a> {
        let base = doc
            .select(&sel("base[href]"))
            .next()
            .and_then(|e| e.value().attr("href"))
            .and_then(|h| page.final_url.join(h).ok())
            .unwrap_or_else(|| page.final_url.clone());
        Ctx { page, doc, meta: Meta::from_doc(doc), local: is_local(&page.final_url), base }
    }

    pub fn https(&self) -> bool {
        self.page.final_url.scheme() == "https"
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.page.headers.get(name)?.to_str().ok()
    }
}

pub fn run_static(c: &Ctx, out: &mut Out) {
    seo::run(c, out);
    social::run(c, out);
    a11y::run(c, out);
    perf::run(c, out);
    security::run(c, out);
    html::run(c, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Severity;
    use reqwest::header::HeaderMap;

    fn page(body: &str, url: &str, headers: &[(&'static str, &'static str)]) -> PageResponse {
        let mut h = HeaderMap::new();
        for (k, v) in headers {
            h.append(*k, v.parse().unwrap());
        }
        PageResponse {
            final_url: Url::parse(url).unwrap(),
            status: 200,
            headers: h,
            body: body.to_string(),
            ttfb_ms: 120,
            total_ms: 150,
            redirects: vec![],
        }
    }

    fn run(body: &str, url: &str, headers: &[(&'static str, &'static str)]) -> Out {
        let p = page(body, url, headers);
        let doc = Html::parse_document(&p.body);
        let ctx = Ctx::new(&p, &doc);
        let mut out = Out::default();
        run_static(&ctx, &mut out);
        out
    }

    const GOOD: &str = r##"<!DOCTYPE html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Metaweave — pre-production checks</title>
<meta name="description" content="Metaweave checks SEO, social previews, accessibility, performance and security of your website before you ship it to production.">
<link rel="canonical" href="https://example.com/">
<link rel="icon" href="/favicon.ico">
<meta name="theme-color" content="#000000">
<meta property="og:title" content="Metaweave">
<meta property="og:description" content="Pre-production checks">
<meta property="og:image" content="https://example.com/og.png">
<meta property="og:image:alt" content="Logo">
<meta property="og:url" content="https://example.com/">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Metaweave">
<meta name="twitter:card" content="summary_large_image">
<script src="/app.js" defer></script>
<script type="application/ld+json">{"@context":"https://schema.org","@type":"WebSite","name":"Metaweave"}</script>
</head><body>
<a href="#main">Skip to content</a>
<header><nav><a href="/about">About us</a></nav></header>
<main id="main"><h1>Hello</h1><h2>World</h2>
<img src="/a.webp" alt="A picture" width="10" height="10">
<label for="q">Search</label><input id="q" type="text">
<button>Go</button></main></body></html>"##;

    #[test]
    fn good_page_has_no_errors_or_warnings() {
        let out = run(
            GOOD,
            "https://example.com/",
            &[
                ("strict-transport-security", "max-age=63072000"),
                ("x-content-type-options", "nosniff"),
                ("x-frame-options", "DENY"),
            ],
        );
        let bad: Vec<_> = out
            .findings
            .iter()
            .filter(|f| f.severity >= Severity::Warning)
            .map(|f| format!("{}: {}", f.id, f.message))
            .collect();
        assert!(bad.is_empty(), "unexpected findings: {bad:#?}");
        assert!(out.passed.len() > 25);
    }

    #[test]
    fn bad_page_is_flagged() {
        let bad = r##"<html><head><meta name="robots" content="noindex">
<meta name="viewport" content="width=device-width, user-scalable=no">
<link rel="canonical" href="http://localhost:3000/"><script src="/blocking.js"></script></head>
<body><font>old</font><img src="x.png"><a href="#"></a><a href="#nowhere">x</a>
<div id="d"></div><div id="d"></div><h1>a</h1><h1>b</h1><h4>c</h4>
<input type="text"><button></button><script src="https://cdn.example.net/x.js"></script>
<img src="http://insecure.example/i.png" alt="x"></body></html>"##;
        let out = run(bad, "https://example.com/", &[("server", "nginx/1.18.0")]);
        let ids: Vec<_> = out.findings.iter().map(|f| f.id).collect();
        for expected in [
            "seo/title",
            "seo/description",
            "seo/canonical",
            "seo/indexable",
            "seo/h1",
            "social/og-title",
            "social/og-image",
            "social/twitter-card",
            "a11y/lang",
            "a11y/img-alt",
            "a11y/link-name",
            "a11y/button-name",
            "a11y/form-label",
            "a11y/heading-order",
            "a11y/viewport-zoom",
            "a11y/duplicate-id",
            "html/doctype",
            "html/charset",
            "html/deprecated",
            "links/placeholder",
            "links/anchors",
            "sec/mixed-content",
            "sec/sri",
            "sec/hsts",
            "sec/server-header",
            "perf/render-blocking",
        ] {
            assert!(ids.contains(&expected), "{expected} was not flagged; got {ids:?}");
        }
    }

    #[test]
    fn sri_ignores_dynamic_font_stylesheets() {
        let html = r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter"></head><body></body></html>"#;
        let out = run(html, "https://example.com/", &[]);
        assert!(!out.findings.iter().any(|f| f.id == "sec/sri"));
    }

    #[test]
    fn local_pages_skip_production_only_checks() {
        let out = run(GOOD, "http://localhost:3000/", &[]);
        let ids: Vec<_> = out.findings.iter().map(|f| f.id).collect();
        assert!(!ids.iter().any(|id| id.starts_with("sec/hsts") || *id == "perf/ttfb"));
    }
}
