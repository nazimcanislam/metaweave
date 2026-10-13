use url::Url;

use super::Ctx;
use crate::model::{Out, Severity::*};
use crate::util::{cap, host_key, is_local, sel, text_of, truncate};

pub fn run(c: &Ctx, out: &mut Out) {
    title(c, out);
    description(c, out);
    canonical(c, out);
    h1(c, out);
    indexable(c, out);
    structured_data(c, out);
    viewport(c, out);
    link_text(c, out);
}

fn title(c: &Ctx, out: &mut Out) {
    let id = "seo/title";
    match c.doc.select(&sel("title")).next().map(text_of) {
        None => out.flag(id, Error, "No <title> element found"),
        Some(t) if t.is_empty() => out.flag(id, Error, "The <title> is empty"),
        Some(t) => {
            let n = t.chars().count();
            if n > 60 {
                out.flag(
                    id,
                    Warning,
                    format!("Title is {n} characters long (recommended 10–60) and will be truncated in search results"),
                );
            } else if n < 10 {
                out.flag(id, Warning, format!("Title is only {n} characters long (recommended 10–60)"));
            } else {
                out.ok(id);
            }
        }
    }
}

fn description(c: &Ctx, out: &mut Out) {
    let id = "seo/description";
    match c.meta.get("description") {
        None if c.meta.present("description") => out.flag(id, Error, "The meta description is empty"),
        None => out.flag(id, Error, "No meta description found"),
        Some(d) => {
            let n = d.chars().count();
            if n < 50 {
                out.flag(id, Warning, format!("Meta description is only {n} characters long (recommended 50–160)"));
            } else if n > 160 {
                out.flag(
                    id,
                    Warning,
                    format!("Meta description is {n} characters long (recommended 50–160) and may be cut off"),
                );
            } else {
                out.ok(id);
            }
        }
    }
}

fn canonical(c: &Ctx, out: &mut Out) {
    let id = "seo/canonical";
    let links: Vec<_> = c.doc.select(&sel("link[rel~=\"canonical\"]")).collect();
    if links.is_empty() {
        return out.flag(id, Warning, "No <link rel=\"canonical\"> found");
    }
    if links.len() > 1 {
        return out.flag(id, Warning, format!("{} canonical links declared — only one is allowed", links.len()));
    }
    let href = links[0].value().attr("href").unwrap_or("").trim();
    if href.is_empty() {
        return out.flag(id, Error, "The canonical link has an empty href");
    }
    let Ok(url) = Url::parse(href) else {
        return out.flag(id, Warning, format!("Canonical href “{}” is not an absolute URL", truncate(href, 60)));
    };
    if is_local(&url) && !c.local {
        return out.flag(
            id,
            Error,
            format!("Canonical points at a local address ({url}) — a dev URL leaked into the page"),
        );
    }
    if !c.local && host_key(&url) != host_key(&c.page.final_url) {
        return out.flag(
            id,
            Warning,
            format!("Canonical points at a different host ({}) — is that intended?", host_key(&url)),
        );
    }
    out.ok(id);
}

fn h1(c: &Ctx, out: &mut Out) {
    let id = "seo/h1";
    match c.doc.select(&sel("h1")).count() {
        0 => out.flag(id, Warning, "No <h1> found"),
        1 => out.ok(id),
        n => out.flag(id, Info, format!("{n} <h1> elements found — one per page is the common recommendation")),
    }
}

fn indexable(c: &Ctx, out: &mut Out) {
    let mut reasons = Vec::new();
    for name in ["robots", "googlebot"] {
        if c.meta.get(name).is_some_and(|v| v.to_ascii_lowercase().contains("noindex")) {
            reasons.push(format!("<meta name=\"{name}\"> contains noindex"));
        }
    }
    if c.header("x-robots-tag").is_some_and(|v| v.to_ascii_lowercase().contains("noindex")) {
        reasons.push("X-Robots-Tag response header contains noindex".to_string());
    }
    if reasons.is_empty() {
        out.ok("seo/indexable");
    } else {
        out.flag_with(
            "seo/indexable",
            Warning,
            "Page asks search engines not to index it (expected on staging, fatal in production)",
            reasons,
        );
    }
}

fn structured_data(c: &Ctx, out: &mut Out) {
    let id = "seo/structured-data";
    let scripts: Vec<_> = c.doc.select(&sel("script[type=\"application/ld+json\"]")).collect();
    if scripts.is_empty() {
        return out.flag(id, Info, "No JSON-LD structured data found (optional, but enables rich results)");
    }
    let bad: Vec<String> = scripts
        .iter()
        .filter_map(|s| {
            let raw: String = s.text().collect();
            serde_json::from_str::<serde_json::Value>(raw.trim()).err().map(|e| format!("invalid JSON: {e}"))
        })
        .collect();
    if bad.is_empty() {
        out.ok(id);
    } else {
        out.flag_with(id, Error, format!("{} JSON-LD block(s) are not valid JSON", bad.len()), bad);
    }
}

fn viewport(c: &Ctx, out: &mut Out) {
    let id = "seo/viewport";
    match c.meta.get("viewport") {
        None => out.flag(id, Error, "No <meta name=\"viewport\"> found — the page is not mobile-friendly"),
        Some(v) if !v.replace(' ', "").contains("width=device-width") => {
            out.flag(id, Warning, format!("Viewport is “{v}” — expected width=device-width"))
        }
        Some(_) => out.ok(id),
    }
}

fn link_text(c: &Ctx, out: &mut Out) {
    const GENERIC: [&str; 14] = [
        "click here",
        "here",
        "read more",
        "more",
        "learn more",
        "link",
        "this link",
        "details",
        "tıklayın",
        "buraya tıklayın",
        "devamını oku",
        "devamı",
        "daha fazla",
        "detaylar",
    ];
    let bad: Vec<String> = c
        .doc
        .select(&sel("a[href]"))
        .filter(|a| {
            let t = text_of(*a).to_lowercase();
            GENERIC.contains(&t.trim_matches(|ch: char| !ch.is_alphanumeric()).trim())
        })
        .map(|a| format!("“{}” → {}", text_of(a), truncate(a.value().attr("href").unwrap_or(""), 50)))
        .collect();
    if bad.is_empty() {
        out.ok("seo/link-text");
    } else {
        out.flag_with("seo/link-text", Info, format!("{} link(s) use generic text", bad.len()), cap(bad, 5));
    }
}
