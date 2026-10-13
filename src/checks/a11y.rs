use std::collections::{BTreeMap, HashSet};

use scraper::ElementRef;

use super::Ctx;
use crate::model::{Out, Severity::*};
use crate::util::{cap, sel, text_of, truncate};

pub fn run(c: &Ctx, out: &mut Out) {
    lang(c, out);
    img_alt(c, out);
    link_name(c, out);
    button_name(c, out);
    form_label(c, out);
    heading_order(c, out);
    viewport_zoom(c, out);
    duplicate_id(c, out);
    landmarks(c, out);
    iframe_title(c, out);
    video_captions(c, out);
    tabindex(c, out);
    skip_link(c, out);
}

fn non_empty(el: ElementRef, attr: &str) -> bool {
    el.value().attr(attr).is_some_and(|v| !v.trim().is_empty())
}

/// Visible text, an ARIA label, a title, or a labelled image/svg inside.
fn has_name(el: ElementRef) -> bool {
    if ["aria-label", "aria-labelledby", "title"].iter().any(|a| non_empty(el, a)) || !text_of(el).is_empty() {
        return true;
    }
    el.select(&sel("img[alt], [aria-label], svg title")).any(|d| {
        if d.value().name() == "title" {
            !text_of(d).is_empty()
        } else {
            non_empty(d, "alt") || non_empty(d, "aria-label")
        }
    })
}

fn lang(c: &Ctx, out: &mut Out) {
    let id = "a11y/lang";
    let value = c.doc.select(&sel("html")).next().and_then(|h| h.value().attr("lang")).map(str::trim);
    match value {
        None | Some("") => out.flag(id, Error, "<html> has no lang attribute"),
        Some(l) => {
            let mut parts = l.split('-');
            let first = parts.next().unwrap_or("");
            let valid = (2..=3).contains(&first.len())
                && first.chars().all(|ch| ch.is_ascii_alphabetic())
                && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|ch| ch.is_ascii_alphanumeric()));
            if valid { out.ok(id) } else { out.flag(id, Warning, format!("lang=\"{l}\" is not a valid language tag")) }
        }
    }
}

fn img_alt(c: &Ctx, out: &mut Out) {
    let imgs: Vec<_> = c.doc.select(&sel("img")).collect();
    if imgs.is_empty() {
        return;
    }
    let missing: Vec<String> = imgs
        .iter()
        .filter(|i| i.value().attr("alt").is_none() && i.value().attr("role") != Some("presentation"))
        .map(|i| truncate(i.value().attr("src").unwrap_or("(no src)"), 70))
        .collect();
    if missing.is_empty() {
        out.ok("a11y/img-alt");
    } else {
        out.flag_with(
            "a11y/img-alt",
            Error,
            format!("{} of {} images have no alt attribute", missing.len(), imgs.len()),
            cap(missing, 5),
        );
    }
}

fn link_name(c: &Ctx, out: &mut Out) {
    let links: Vec<_> = c.doc.select(&sel("a[href]")).collect();
    if links.is_empty() {
        return;
    }
    let bad: Vec<String> = links
        .iter()
        .filter(|a| !has_name(**a))
        .map(|a| format!("<a href=\"{}\">", truncate(a.value().attr("href").unwrap_or(""), 60)))
        .collect();
    if bad.is_empty() {
        out.ok("a11y/link-name");
    } else {
        out.flag_with("a11y/link-name", Error, format!("{} link(s) have no accessible name", bad.len()), cap(bad, 5));
    }
}

fn button_name(c: &Ctx, out: &mut Out) {
    let buttons: Vec<_> = c
        .doc
        .select(&sel(
            "button, [role=\"button\"], input[type=\"button\"], input[type=\"submit\"], input[type=\"reset\"]",
        ))
        .collect();
    if buttons.is_empty() {
        return;
    }
    let bad =
        buttons.iter().filter(|b| !(has_name(**b) || (b.value().name() == "input" && non_empty(**b, "value")))).count();
    if bad == 0 {
        out.ok("a11y/button-name");
    } else {
        out.flag("a11y/button-name", Error, format!("{bad} button(s) have no accessible name"));
    }
}

fn form_label(c: &Ctx, out: &mut Out) {
    let fields: Vec<_> = c
        .doc
        .select(&sel("input:not([type=\"hidden\"]):not([type=\"submit\"]):not([type=\"button\"]):not([type=\"reset\"]):not([type=\"image\"]), select, textarea"))
        .collect();
    if fields.is_empty() {
        return;
    }
    let label_for: HashSet<&str> = c.doc.select(&sel("label[for]")).filter_map(|l| l.value().attr("for")).collect();
    let bad: Vec<String> = fields
        .iter()
        .filter(|f| {
            let v = f.value();
            let wrapped = f.ancestors().filter_map(ElementRef::wrap).any(|a| a.value().name() == "label");
            let by_for = v.id().is_some_and(|id| label_for.contains(id));
            !(wrapped || by_for || ["aria-label", "aria-labelledby", "title"].iter().any(|a| non_empty(**f, a)))
        })
        .map(|f| {
            let v = f.value();
            format!("<{} name=\"{}\" id=\"{}\">", v.name(), v.attr("name").unwrap_or(""), v.id().unwrap_or(""))
        })
        .collect();
    if bad.is_empty() {
        out.ok("a11y/form-label");
    } else {
        out.flag_with("a11y/form-label", Error, format!("{} form field(s) have no label", bad.len()), cap(bad, 5));
    }
}

fn heading_order(c: &Ctx, out: &mut Out) {
    let mut prev = 0u8;
    let mut problems = Vec::new();
    let mut any = false;
    for h in c.doc.select(&sel("h1, h2, h3, h4, h5, h6")) {
        any = true;
        let level = h.value().name().as_bytes()[1] - b'0';
        let text = text_of(h);
        if text.is_empty() && h.select(&sel("img[alt]")).next().is_none() {
            problems.push(format!("empty <h{level}>"));
        }
        if prev != 0 && level > prev + 1 {
            problems.push(format!("<h{prev}> → <h{level}> skips a level (“{}”)", truncate(&text, 40)));
        }
        prev = level;
    }
    if !any {
        return;
    }
    if problems.is_empty() {
        out.ok("a11y/heading-order");
    } else {
        out.flag_with(
            "a11y/heading-order",
            Warning,
            format!("{} heading problem(s)", problems.len()),
            cap(problems, 5),
        );
    }
}

fn viewport_zoom(c: &Ctx, out: &mut Out) {
    let Some(content) = c.meta.get("viewport") else {
        return;
    };
    let mut problems = Vec::new();
    for part in content.to_ascii_lowercase().split(',') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k == "user-scalable" && matches!(v, "no" | "0") {
            problems.push("user-scalable=no".to_string());
        }
        if k == "maximum-scale" && v.parse::<f32>().is_ok_and(|n| n < 2.0) {
            problems.push(format!("maximum-scale={v}"));
        }
    }
    if problems.is_empty() {
        out.ok("a11y/viewport-zoom");
    } else {
        out.flag("a11y/viewport-zoom", Error, format!("Zooming is restricted ({})", problems.join(", ")));
    }
}

fn duplicate_id(c: &Ctx, out: &mut Out) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for el in c.doc.select(&sel("[id]")) {
        if let Some(id) = el.value().attr("id").filter(|i| !i.is_empty()) {
            *counts.entry(id).or_default() += 1;
        }
    }
    let dups: Vec<String> =
        counts.into_iter().filter(|(_, n)| *n > 1).map(|(id, n)| format!("#{id} used {n}×")).collect();
    if dups.is_empty() {
        out.ok("a11y/duplicate-id");
    } else {
        out.flag_with(
            "a11y/duplicate-id",
            Error,
            format!("{} id(s) are used more than once", dups.len()),
            cap(dups, 5),
        );
    }
}

fn landmarks(c: &Ctx, out: &mut Out) {
    if c.doc.select(&sel("main, [role=\"main\"]")).next().is_some() {
        out.ok("a11y/landmarks");
    } else {
        out.flag("a11y/landmarks", Warning, "No <main> landmark found");
    }
}

fn iframe_title(c: &Ctx, out: &mut Out) {
    let frames: Vec<_> = c.doc.select(&sel("iframe")).collect();
    if frames.is_empty() {
        return;
    }
    let bad = frames.iter().filter(|f| !non_empty(**f, "title") && !non_empty(**f, "aria-label")).count();
    if bad == 0 {
        out.ok("a11y/iframe-title");
    } else {
        out.flag("a11y/iframe-title", Warning, format!("{bad} <iframe> element(s) have no title"));
    }
}

fn video_captions(c: &Ctx, out: &mut Out) {
    let videos: Vec<_> = c.doc.select(&sel("video")).collect();
    if videos.is_empty() {
        return;
    }
    let track = sel("track[kind=\"captions\"], track[kind=\"subtitles\"]");
    let bad = videos.iter().filter(|v| v.select(&track).next().is_none()).count();
    if bad == 0 {
        out.ok("a11y/video-captions");
    } else {
        out.flag("a11y/video-captions", Warning, format!("{bad} <video> element(s) have no captions track"));
    }
}

fn tabindex(c: &Ctx, out: &mut Out) {
    let bad = c
        .doc
        .select(&sel("[tabindex]"))
        .filter(|e| e.value().attr("tabindex").and_then(|v| v.trim().parse::<i32>().ok()).is_some_and(|n| n > 0))
        .count();
    if bad == 0 {
        out.ok("a11y/tabindex");
    } else {
        out.flag("a11y/tabindex", Warning, format!("{bad} element(s) use a positive tabindex"));
    }
}

fn skip_link(c: &Ctx, out: &mut Out) {
    let links: Vec<_> = c.doc.select(&sel("a[href]")).collect();
    if links.len() < 8 {
        return; // little to skip
    }
    let found = links.iter().take(5).any(|a| {
        let label = format!("{} {}", text_of(*a), a.value().attr("aria-label").unwrap_or("")).to_lowercase();
        a.value().attr("href").is_some_and(|h| h.starts_with('#') && h.len() > 1)
            && (label.contains("skip") || label.contains("atla"))
    });
    if found {
        out.ok("a11y/skip-link");
    } else {
        out.flag("a11y/skip-link", Info, "No “skip to content” link found among the first links");
    }
}
