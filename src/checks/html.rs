use std::collections::HashSet;

use super::Ctx;
use crate::model::{Out, Severity::*};
use crate::util::{cap, sel, truncate};

pub fn run(c: &Ctx, out: &mut Out) {
    status(c, out);
    doctype(c, out);
    charset(c, out);
    deprecated(c, out);
    meta_refresh(c, out);
    theme_color(c, out);
    anchors(c, out);
    placeholder_links(c, out);
}

fn status(c: &Ctx, out: &mut Out) {
    let s = c.page.status;
    if (200..300).contains(&s) {
        out.ok("html/status");
    } else {
        out.flag("html/status", Error, format!("The page responded with HTTP {s}"));
    }
}

fn doctype(c: &Ctx, out: &mut Out) {
    let head: String = c.page.body.trim_start_matches('\u{feff}').trim_start().chars().take(200).collect();
    // Allow leading comments before the doctype.
    if head.to_ascii_lowercase().contains("<!doctype html") {
        out.ok("html/doctype");
    } else {
        out.flag("html/doctype", Warning, "No <!DOCTYPE html> found — the browser will use quirks mode");
    }
}

fn charset(c: &Ctx, out: &mut Out) {
    let id = "html/charset";
    let declared = c
        .doc
        .select(&sel("meta[charset]"))
        .next()
        .and_then(|m| m.value().attr("charset").map(str::to_string))
        .or_else(|| {
            c.doc
                .select(&sel("meta[http-equiv]"))
                .find(|m| m.value().attr("http-equiv").is_some_and(|h| h.eq_ignore_ascii_case("content-type")))
                .and_then(|m| m.value().attr("content"))
                .and_then(|v| v.to_ascii_lowercase().split("charset=").nth(1).map(|s| s.trim().to_string()))
        })
        .or_else(|| {
            c.header("content-type")
                .and_then(|v| v.to_ascii_lowercase().split("charset=").nth(1).map(|s| s.trim().to_string()))
        });
    match declared {
        None => out.flag(id, Error, "No character encoding declared (<meta charset> or Content-Type header)"),
        Some(cs) if cs.trim().eq_ignore_ascii_case("utf-8") || cs.trim().eq_ignore_ascii_case("utf8") => out.ok(id),
        Some(cs) => out.flag(id, Warning, format!("Encoding is “{}” — UTF-8 is strongly recommended", cs.trim())),
    }
}

fn deprecated(c: &Ctx, out: &mut Out) {
    let tags = "font, center, marquee, blink, big, strike, frameset, frame, acronym, applet, basefont, dir";
    let mut found: Vec<String> = c.doc.select(&sel(tags)).map(|e| format!("<{}>", e.value().name())).collect();
    found.sort();
    found.dedup();
    if found.is_empty() {
        out.ok("html/deprecated");
    } else {
        out.flag("html/deprecated", Warning, format!("Obsolete element(s) in use: {}", found.join(", ")));
    }
}

fn meta_refresh(c: &Ctx, out: &mut Out) {
    let used = c
        .doc
        .select(&sel("meta[http-equiv]"))
        .any(|m| m.value().attr("http-equiv").is_some_and(|h| h.eq_ignore_ascii_case("refresh")));
    if used {
        out.flag("html/meta-refresh", Warning, "The page uses <meta http-equiv=\"refresh\">");
    } else {
        out.ok("html/meta-refresh");
    }
}

fn theme_color(c: &Ctx, out: &mut Out) {
    if c.meta.get("theme-color").is_some() {
        out.ok("assets/theme-color");
    } else {
        out.flag("assets/theme-color", Info, "No <meta name=\"theme-color\"> found");
    }
}

fn anchors(c: &Ctx, out: &mut Out) {
    let mut ids: HashSet<&str> = c.doc.select(&sel("[id]")).filter_map(|e| e.value().attr("id")).collect();
    ids.extend(c.doc.select(&sel("a[name]")).filter_map(|e| e.value().attr("name")));
    let mut checked = 0;
    let mut missing = Vec::new();
    for a in c.doc.select(&sel("a[href^=\"#\"]")) {
        let frag = a.value().attr("href").unwrap_or("").trim_start_matches('#');
        // "#", "#top" and SPA hash routes ("#/path", "#!") are not DOM anchors.
        if frag.is_empty() || frag.eq_ignore_ascii_case("top") || frag.starts_with('/') || frag.starts_with('!') {
            continue;
        }
        checked += 1;
        if !ids.contains(frag) {
            missing.push(format!("#{frag}"));
        }
    }
    missing.sort();
    missing.dedup();
    if checked == 0 {
        return;
    }
    if missing.is_empty() {
        out.ok("links/anchors");
    } else {
        out.flag_with(
            "links/anchors",
            Warning,
            format!("{} in-page link(s) point to a missing id", missing.len()),
            cap(missing, 5),
        );
    }
}

fn placeholder_links(c: &Ctx, out: &mut Out) {
    let bad: Vec<String> = c
        .doc
        .select(&sel("a[href]"))
        .filter(|a| {
            let h = a.value().attr("href").unwrap_or("").trim().to_ascii_lowercase();
            h == "#" || h.starts_with("javascript:")
        })
        .map(|a| truncate(a.value().attr("href").unwrap_or(""), 40))
        .collect();
    if bad.is_empty() {
        out.ok("links/placeholder");
    } else {
        out.flag(
            "links/placeholder",
            Info,
            format!("{} link(s) use href=\"#\" or javascript: instead of a real URL", bad.len()),
        );
    }
}
