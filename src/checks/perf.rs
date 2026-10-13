use super::Ctx;
use crate::model::{Out, Severity::*};
use crate::util::{cap, human_bytes, sel, truncate};

pub fn run(c: &Ctx, out: &mut Out) {
    ttfb(c, out);
    html_size(c, out);
    redirects(c, out);
    render_blocking(c, out);
    img_dimensions(c, out);
    img_lazy(c, out);
    img_format(c, out);
    dom_size(c, out);
}

fn ttfb(c: &Ctx, out: &mut Out) {
    if c.local {
        return; // dev-server latency says nothing about production
    }
    let ms = c.page.ttfb_ms;
    if ms <= 600 {
        out.ok("perf/ttfb");
    } else if ms <= 1800 {
        out.flag(
            "perf/ttfb",
            Warning,
            format!("Server took {ms} ms to respond (target ≤ 600 ms; measured from this machine, one request)"),
        );
    } else {
        out.flag(
            "perf/ttfb",
            Error,
            format!("Server took {ms} ms to respond (target ≤ 600 ms; measured from this machine, one request)"),
        );
    }
}

fn html_size(c: &Ctx, out: &mut Out) {
    let n = c.page.body.len();
    if n > 500 * 1024 {
        out.flag("perf/html-size", Error, format!("HTML is {} (target < 100 KB)", human_bytes(n)));
    } else if n > 100 * 1024 {
        out.flag("perf/html-size", Warning, format!("HTML is {} (target < 100 KB)", human_bytes(n)));
    } else {
        out.ok("perf/html-size");
    }
}

fn redirects(c: &Ctx, out: &mut Out) {
    let hops = &c.page.redirects;
    if hops.len() < 2 {
        out.ok("perf/redirects");
    } else {
        out.flag_with(
            "perf/redirects",
            Warning,
            format!("Reaching the page takes {} redirects", hops.len()),
            hops.clone(),
        );
    }
}

fn render_blocking(c: &Ctx, out: &mut Out) {
    let bad: Vec<String> = c
        .doc
        .select(&sel("head script[src]"))
        .filter(|s| {
            let v = s.value();
            v.attr("async").is_none() && v.attr("defer").is_none() && v.attr("type") != Some("module")
        })
        .map(|s| truncate(s.value().attr("src").unwrap_or(""), 80))
        .collect();
    if bad.is_empty() {
        out.ok("perf/render-blocking");
    } else {
        out.flag_with(
            "perf/render-blocking",
            Warning,
            format!("{} script(s) in <head> block rendering", bad.len()),
            cap(bad, 5),
        );
    }
}

fn img_dimensions(c: &Ctx, out: &mut Out) {
    let imgs: Vec<_> = c.doc.select(&sel("img")).collect();
    if imgs.is_empty() {
        return;
    }
    let bad: Vec<String> = imgs
        .iter()
        .filter(|i| {
            let v = i.value();
            let svg = v.attr("src").is_some_and(|s| s.split(['?', '#']).next().unwrap_or("").ends_with(".svg"));
            !svg && (v.attr("width").is_none() || v.attr("height").is_none())
        })
        .map(|i| truncate(i.value().attr("src").unwrap_or("(no src)"), 70))
        .collect();
    if bad.is_empty() {
        out.ok("perf/img-dimensions");
    } else {
        out.flag_with(
            "perf/img-dimensions",
            Warning,
            format!("{} of {} images lack width/height (layout shift)", bad.len(), imgs.len()),
            cap(bad, 5),
        );
    }
}

fn img_lazy(c: &Ctx, out: &mut Out) {
    let imgs: Vec<_> = c.doc.select(&sel("img")).collect();
    if imgs.len() <= 4 {
        return;
    }
    if imgs.iter().any(|i| i.value().attr("loading") == Some("lazy")) {
        out.ok("perf/img-lazy");
    } else {
        out.flag("perf/img-lazy", Info, format!("None of the {} images use loading=\"lazy\"", imgs.len()));
    }
}

fn extension(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or("").rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

fn img_format(c: &Ctx, out: &mut Out) {
    let (mut legacy, mut modern) = (0, 0);
    for img in c.doc.select(&sel("img[src]")) {
        match extension(img.value().attr("src").unwrap_or("")).as_str() {
            "jpg" | "jpeg" | "png" | "gif" => legacy += 1,
            "webp" | "avif" | "svg" | "jxl" => modern += 1,
            _ => {}
        }
    }
    if c.doc.select(&sel("source[type*=\"webp\"], source[type*=\"avif\"]")).next().is_some() {
        modern += 1;
    }
    if legacy > 0 && modern == 0 {
        out.flag(
            "perf/img-format",
            Info,
            format!("All {legacy} raster image(s) are JPEG/PNG/GIF — consider WebP or AVIF"),
        );
    } else if legacy + modern > 0 {
        out.ok("perf/img-format");
    }
}

fn dom_size(c: &Ctx, out: &mut Out) {
    let n = c.doc.select(&sel("*")).count();
    if n > 3500 {
        out.flag("perf/dom-size", Error, format!("DOM has {n} elements (target < 1500)"));
    } else if n > 1500 {
        out.flag("perf/dom-size", Warning, format!("DOM has {n} elements (target < 1500)"));
    } else {
        out.ok("perf/dom-size");
    }
}
