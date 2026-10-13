//! Checks that need extra HTTP requests: site files, icons, social image, compression, links.

use std::collections::{HashMap, HashSet};

use futures::future::join_all;
use futures::{StreamExt, stream};
use url::Url;

use crate::extract::{Extracted, ResKind};
use crate::fetcher::{Fetcher, PageResponse, Probe};
use crate::filter::Filter;
use crate::imgdim;
use crate::model::{Out, Severity::*};
use crate::util::{cap, host_key, human_bytes, is_local, truncate};

pub struct NetCtx<'a> {
    pub fetcher: &'a Fetcher,
    pub page: &'a PageResponse,
    pub x: &'a Extracted,
    pub filter: &'a Filter,
    pub local: bool,
    /// First page of this origin: also run the once-per-site checks.
    pub site: bool,
    pub max_links: usize,
}

pub async fn run(n: &NetCtx<'_>) -> Out {
    let (a, b, c, d, e, f, g) = tokio::join!(
        robots_and_sitemap(n),
        not_found(n),
        https(n),
        icons(n),
        manifest(n),
        social_image(n),
        compression(n),
    );
    let links = links(n).await;
    let mut out = Out::default();
    for part in [a, b, c, d, e, f, g, links] {
        out.merge(part);
    }
    out
}

fn origin(u: &Url) -> Url {
    let mut o = u.clone();
    o.set_path("/");
    o.set_query(None);
    o.set_fragment(None);
    o
}

fn same_site(a: &Url, b: &Url) -> bool {
    a.host_str().zip(b.host_str()).is_some_and(|(x, y)| x.eq_ignore_ascii_case(y))
        && a.port_or_known_default() == b.port_or_known_default()
}

// ── robots.txt & sitemap.xml ────────────────────────────────────────────────

struct Robots {
    blocks_all: bool,
    sitemaps: Vec<String>,
}

fn parse_robots(text: &str) -> Robots {
    let mut r = Robots { blocks_all: false, sitemaps: Vec::new() };
    let (mut in_rules, mut star, mut star_disallows_root, mut star_allows) = (false, false, false, false);
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match key.as_str() {
            "user-agent" => {
                if in_rules {
                    star = false;
                    in_rules = false;
                }
                if value == "*" {
                    star = true;
                }
            }
            "disallow" => {
                in_rules = true;
                if star && value == "/" {
                    star_disallows_root = true;
                }
            }
            "allow" => {
                in_rules = true;
                if star && !value.is_empty() {
                    star_allows = true;
                }
            }
            "sitemap" => r.sitemaps.push(value.to_string()),
            _ => in_rules = true,
        }
    }
    r.blocks_all = star_disallows_root && !star_allows;
    r
}

fn looks_like_html(s: &str) -> bool {
    let head = s.trim_start().chars().take(200).collect::<String>().to_ascii_lowercase();
    head.starts_with("<!doctype") || head.starts_with("<html") || head.starts_with("<head")
}

async fn robots_and_sitemap(n: &NetCtx<'_>) -> Out {
    let mut out = Out::default();
    if !n.site || !n.filter.wants_any(&["assets/robots-txt", "assets/sitemap"]) {
        return out;
    }
    let id = "assets/robots-txt";
    let root = origin(&n.page.final_url);
    let robots_url = root.join("robots.txt").expect("valid join");
    let mut sitemaps: Vec<Url> = Vec::new();
    match n.fetcher.get_limited(&robots_url, 512 * 1024).await {
        Err(e) => out.flag(id, Warning, format!("Could not fetch robots.txt: {e}")),
        Ok(r) if !r.ok() => out.flag(id, Warning, format!("robots.txt not found (HTTP {})", r.status)),
        Ok(r) => {
            let text = r.text();
            if looks_like_html(&text) {
                out.flag(id, Warning, "robots.txt returns an HTML page (SPA fallback?) instead of rules");
            } else {
                let parsed = parse_robots(&text);
                sitemaps = parsed.sitemaps.iter().filter_map(|s| Url::parse(s).ok()).collect();
                if parsed.blocks_all {
                    out.flag(id, Error, "robots.txt disallows the entire site for all crawlers (Disallow: /)");
                } else {
                    out.ok(id);
                }
            }
        }
    }
    if n.filter.wants("assets/sitemap") {
        out.merge(sitemap(n, &root, sitemaps).await);
    }
    out
}

fn extract_locs(xml: &str) -> Vec<String> {
    let mut locs = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<loc>") {
        let after = &rest[i + 5..];
        let Some(j) = after.find("</loc>") else { break };
        locs.push(after[..j].trim().replace("&amp;", "&"));
        rest = &after[j + 6..];
        if locs.len() >= 5000 {
            break;
        }
    }
    locs
}

async fn sitemap(n: &NetCtx<'_>, root: &Url, declared: Vec<Url>) -> Out {
    let id = "assets/sitemap";
    let mut out = Out::default();
    // On a dev server robots.txt often names the production sitemap; check the local one instead.
    let url = declared
        .into_iter()
        .find(|u| !n.local || same_site(u, root))
        .unwrap_or_else(|| root.join("sitemap.xml").expect("valid join"));
    let r = match n.fetcher.get_limited(&url, 5 * 1024 * 1024).await {
        Err(e) => {
            out.flag(id, Warning, format!("Could not fetch {url}: {e}"));
            return out;
        }
        Ok(r) if !r.ok() => {
            out.flag(id, Warning, format!("No sitemap found at {url} (HTTP {})", r.status));
            return out;
        }
        Ok(r) => r,
    };
    let text = r.text();
    if !text.contains("<urlset") && !text.contains("<sitemapindex") {
        out.flag(id, Warning, format!("{url} is not a valid sitemap (no <urlset> or <sitemapindex>)"));
        return out;
    }
    let locs = extract_locs(&text);
    if locs.is_empty() {
        out.flag(id, Warning, format!("{url} contains no <loc> entries"));
        return out;
    }
    if n.local {
        out.ok(id);
        return out;
    }
    let page_host = host_key(&n.page.final_url);
    let (mut local, mut foreign) = (Vec::new(), Vec::new());
    for loc in &locs {
        match Url::parse(loc) {
            Ok(u) if is_local(&u) => local.push(loc.clone()),
            Ok(u) if host_key(&u) != page_host => foreign.push(loc.clone()),
            Ok(_) => {}
            Err(_) => foreign.push(loc.clone()),
        }
    }
    if !local.is_empty() {
        out.flag_with(
            id,
            Error,
            format!("{} sitemap URL(s) point at localhost/private addresses", local.len()),
            cap(local, 3),
        );
    } else if !foreign.is_empty() {
        out.flag_with(
            id,
            Warning,
            format!(
                "{} of {} sitemap URL(s) use a different host than {page_host} (staging domain?)",
                foreign.len(),
                locs.len()
            ),
            cap(foreign, 3),
        );
    } else {
        out.ok(id);
    }
    out
}

async fn not_found(n: &NetCtx<'_>) -> Out {
    let id = "assets/not-found-page";
    let mut out = Out::default();
    if !n.site || !n.filter.wants(id) {
        return out;
    }
    let url =
        origin(&n.page.final_url).join(&format!("metaweave-404-check-{}", std::process::id())).expect("valid join");
    match n.fetcher.get_limited(&url, 64 * 1024).await {
        Err(e) => out.flag(id, Warning, format!("Could not request a non-existent URL: {e}")),
        Ok(r) if r.status == 404 || r.status == 410 => out.ok(id),
        Ok(r) if r.ok() => {
            out.flag(id, Warning, format!("A non-existent URL returned HTTP {} instead of 404 (soft 404)", r.status))
        }
        Ok(r) if r.status >= 500 => {
            out.flag(id, Warning, format!("A non-existent URL caused a server error (HTTP {})", r.status))
        }
        Ok(_) => out.ok(id),
    }
    out
}

// ── HTTPS ───────────────────────────────────────────────────────────────────

async fn https(n: &NetCtx<'_>) -> Out {
    let id = "sec/https";
    let mut out = Out::default();
    if n.local || !n.site || !n.filter.wants(id) {
        return out;
    }
    let url = &n.page.final_url;
    if url.scheme() != "https" {
        out.flag(id, Error, format!("The page is served over plain HTTP ({url})"));
        return out;
    }
    let mut http = url.clone();
    if http.port().is_some() || http.set_scheme("http").is_err() {
        out.ok(id);
        return out;
    }
    let probe = n.fetcher.probe(&http).await;
    match (&probe.error, &probe.final_url) {
        (None, Some(f)) if f.scheme() == "http" && probe.ok() => {
            out.flag(id, Warning, format!("{http} is served without redirecting to HTTPS"))
        }
        _ => out.ok(id), // redirects to https, or port 80 is closed
    }
    out
}

// ── Icons & manifest ────────────────────────────────────────────────────────

fn is_image(p: &Probe) -> bool {
    p.ok() && p.content_type.as_deref().is_none_or(|ct| !ct.to_ascii_lowercase().contains("text/html"))
}

async fn icons(n: &NetCtx<'_>) -> Out {
    let mut out = Out::default();
    let root = origin(&n.page.final_url);

    if n.filter.wants("assets/favicon") {
        let id = "assets/favicon";
        let declared: Vec<&Url> = n.x.icons.iter().filter(|i| i.is_favicon()).map(|i| &i.href).take(5).collect();
        if declared.is_empty() {
            let probe = n.fetcher.probe(&root.join("favicon.ico").expect("valid join")).await;
            if is_image(&probe) {
                out.flag(id, Info, "No <link rel=\"icon\"> — relying on the browser's /favicon.ico fallback");
            } else {
                out.flag(
                    id,
                    Error,
                    format!("No favicon: no <link rel=\"icon\"> and /favicon.ico returns {}", probe.describe()),
                );
            }
        } else {
            let probes = join_all(declared.iter().map(|u| n.fetcher.probe(u))).await;
            let bad: Vec<String> = probes
                .iter()
                .filter(|p| !is_image(p))
                .map(|p| format!("{} → {}", truncate(p.url.as_str(), 70), p.describe()))
                .collect();
            if bad.len() == probes.len() {
                out.flag_with(id, Error, "The declared favicon cannot be loaded", bad);
            } else if !bad.is_empty() {
                out.flag_with(id, Warning, format!("{} declared icon(s) cannot be loaded", bad.len()), bad);
            } else {
                out.ok(id);
            }
        }
    }

    if n.filter.wants("assets/apple-touch-icon") {
        let id = "assets/apple-touch-icon";
        let target = n.x.icons.iter().find(|i| i.is_apple()).map(|i| i.href.clone());
        let declared = target.is_some();
        let url = target.unwrap_or_else(|| root.join("apple-touch-icon.png").expect("valid join"));
        let probe = n.fetcher.probe(&url).await;
        if is_image(&probe) {
            out.ok(id);
        } else if declared {
            out.flag(id, Warning, format!("The declared apple-touch-icon cannot be loaded ({})", probe.describe()));
        } else {
            out.flag(id, Warning, "No apple-touch-icon declared and /apple-touch-icon.png does not exist");
        }
    }
    out
}

async fn manifest(n: &NetCtx<'_>) -> Out {
    let id = "assets/manifest";
    let mut out = Out::default();
    if !n.filter.wants(id) {
        return out;
    }
    let Some(url) = &n.x.manifest else {
        out.flag(id, Info, "No web app manifest linked (optional, needed for installable PWAs)");
        return out;
    };
    let r = match n.fetcher.get_limited(url, 256 * 1024).await {
        Err(e) => {
            out.flag(id, Error, format!("Could not fetch the manifest: {e}"));
            return out;
        }
        Ok(r) if !r.ok() => {
            out.flag(id, Error, format!("The manifest returns HTTP {}", r.status));
            return out;
        }
        Ok(r) => r,
    };
    let json: serde_json::Value = match serde_json::from_slice(&r.bytes) {
        Ok(v) => v,
        Err(e) => {
            out.flag(id, Error, format!("The manifest is not valid JSON: {e}"));
            return out;
        }
    };
    let (mut warn, mut info) = (Vec::new(), Vec::new());
    if json.get("name").is_none() && json.get("short_name").is_none() {
        warn.push("missing name / short_name".to_string());
    }
    let sizes: Vec<String> = json
        .get("icons")
        .and_then(|i| i.as_array())
        .map(|a| a.iter().filter_map(|i| i.get("sizes").and_then(|s| s.as_str()).map(str::to_string)).collect())
        .unwrap_or_default();
    let has = |s: &str| sizes.iter().any(|x| x.split_whitespace().any(|p| p == s || p == "any"));
    if !has("192x192") || !has("512x512") {
        warn.push("icons should include 192×192 and 512×512".to_string());
    }
    if json.get("start_url").is_none() {
        info.push("no start_url".to_string());
    }
    if json.get("display").is_none() {
        info.push("no display mode".to_string());
    }
    if !warn.is_empty() {
        warn.extend(info);
        out.flag_with(id, Warning, "The web app manifest is incomplete", warn);
    } else if !info.is_empty() {
        out.flag_with(id, Info, "The web app manifest could be more complete", info);
    } else {
        out.ok(id);
    }
    out
}

// ── Social image ────────────────────────────────────────────────────────────

async fn social_image(n: &NetCtx<'_>) -> Out {
    let mut out = Out::default();
    if !n.filter.wants_any(&["social/og-image-reachable", "social/og-image-size"]) {
        return out;
    }
    let Some(url) = &n.x.og_image else { return out };
    let (reach, size) = ("social/og-image-reachable", "social/og-image-size");
    let r = match n.fetcher.get_limited(url, 256 * 1024).await {
        Err(e) => {
            out.flag(reach, Error, format!("og:image cannot be fetched: {e}"));
            return out;
        }
        Ok(r) => r,
    };
    let ctype = r.content_type.clone().unwrap_or_default().to_ascii_lowercase();
    if !r.ok() {
        out.flag(reach, Error, format!("og:image returns HTTP {} ({url})", r.status));
        return out;
    }
    if !ctype.starts_with("image/") {
        out.flag(
            reach,
            Error,
            format!(
                "og:image is served as “{}” instead of an image",
                if ctype.is_empty() { "unknown type" } else { &ctype }
            ),
        );
        return out;
    }
    out.ok(reach);

    if ctype.contains("svg") {
        out.flag(size, Error, "og:image is an SVG — Facebook, LinkedIn and X do not support SVG previews");
        return out;
    }
    let weight = r.content_length.unwrap_or(r.bytes.len() as u64);
    match imgdim::dimensions(&r.bytes) {
        None => out.flag(size, Info, "Could not determine the og:image dimensions"),
        Some((w, h)) if w < 200 || h < 200 => {
            out.flag(size, Error, format!("og:image is {w}×{h} — Facebook ignores images smaller than 200×200"))
        }
        Some((w, h)) if w < 600 || h < 315 => out.flag(
            size,
            Warning,
            format!("og:image is {w}×{h} — below the 600×315 minimum for large cards (1200×630 recommended)"),
        ),
        Some((w, h)) if weight > 5 * 1024 * 1024 => out.flag(
            size,
            Warning,
            format!("og:image is {} ({w}×{h}) — X rejects images above 5 MB", human_bytes(weight as usize)),
        ),
        Some((w, h)) if ((w as f64 / h as f64) / 1.91 - 1.0).abs() > 0.2 => out.flag(
            size,
            Info,
            format!(
                "og:image is {w}×{h} (ratio {:.2}:1) — large cards use 1.91:1 and will be cropped",
                w as f64 / h as f64
            ),
        ),
        Some(_) => out.ok(size),
    }
    out
}

// ── Compression ─────────────────────────────────────────────────────────────

async fn compression(n: &NetCtx<'_>) -> Out {
    let id = "perf/compression";
    let mut out = Out::default();
    if n.local || !n.filter.wants(id) {
        return out;
    }
    if let Ok(info) = n.fetcher.compression(&n.page.final_url).await {
        if info.encoding.is_none() && info.size > 1400 {
            out.flag(id, Warning, format!("The HTML ({}) is sent uncompressed", human_bytes(info.size)));
        } else {
            out.ok(id);
        }
    }
    out
}

// ── Links, resources, caching, image weight ─────────────────────────────────

enum State {
    Ok,
    Redirected(String),
    Blocked,
    Broken(String),
}

fn classify(p: &Probe) -> State {
    if let Some(e) = &p.error {
        return State::Broken(e.clone());
    }
    match p.status {
        Some(200..=299) => match &p.final_url {
            Some(f) if f.as_str().trim_end_matches('/') != p.url.as_str().trim_end_matches('/') => {
                State::Redirected(f.to_string())
            }
            _ => State::Ok,
        },
        Some(401 | 403 | 429 | 999) => State::Blocked,
        Some(s) => State::Broken(format!("HTTP {s}")),
        None => State::Broken("no response".into()),
    }
}

async fn links(n: &NetCtx<'_>) -> Out {
    const IDS: [&str; 5] =
        ["links/internal", "links/external", "links/resources", "perf/cache-headers", "perf/img-weight"];
    let mut out = Out::default();
    if !n.filter.wants_any(&IDS) {
        return out;
    }
    let page = &n.page.final_url;
    let (internal, external): (Vec<&Url>, Vec<&Url>) = n.x.links.iter().partition(|u| same_site(u, page));
    let link_targets: Vec<Url> =
        internal.iter().chain(external.iter()).take(n.max_links).map(|u| (*u).clone()).collect();
    if n.x.links.len() > n.max_links {
        out.note(format!(
            "Only the first {} of {} links were checked (raise --max-links)",
            n.max_links,
            n.x.links.len()
        ));
    }
    let resources: Vec<(ResKind, Url)> = n.x.resources.iter().take(150).cloned().collect();

    let mut seen = HashSet::new();
    let all: Vec<Url> = link_targets
        .iter()
        .cloned()
        .chain(resources.iter().map(|(_, u)| u.clone()))
        .filter(|u| seen.insert(u.to_string()))
        .collect();

    let fetcher = n.fetcher;
    let probes: Vec<Probe> =
        stream::iter(all).map(move |u| async move { fetcher.probe(&u).await }).buffer_unordered(12).collect().await;
    let map: HashMap<String, Probe> = probes.into_iter().map(|p| (p.url.to_string(), p)).collect();
    let state = |u: &Url| map.get(u.as_str()).map(classify);

    // Internal links
    let internal_targets: Vec<&Url> = link_targets.iter().filter(|u| same_site(u, page)).collect();
    if !internal_targets.is_empty() {
        let (mut broken, mut moved) = (Vec::new(), Vec::new());
        for u in &internal_targets {
            match state(u) {
                Some(State::Broken(e)) => broken.push(format!("{} → {e}", truncate(u.as_str(), 80))),
                Some(State::Redirected(to)) => {
                    moved.push(format!("{} → {}", truncate(u.as_str(), 60), truncate(&to, 60)))
                }
                _ => {}
            }
        }
        let id = "links/internal";
        if !broken.is_empty() {
            out.flag_with(id, Error, format!("{} broken internal link(s)", broken.len()), cap(broken, 10));
        } else if !moved.is_empty() {
            out.flag_with(
                id,
                Info,
                format!("{} internal link(s) redirect — link to the final URL instead", moved.len()),
                cap(moved, 5),
            );
        } else {
            out.ok(id);
        }
    }

    // External links
    let external_targets: Vec<&Url> = link_targets.iter().filter(|u| !same_site(u, page)).collect();
    if !external_targets.is_empty() {
        let (mut broken, mut blocked) = (Vec::new(), 0);
        for u in &external_targets {
            match state(u) {
                Some(State::Broken(e)) => broken.push(format!("{} → {e}", truncate(u.as_str(), 80))),
                Some(State::Blocked) => blocked += 1,
                _ => {}
            }
        }
        let id = "links/external";
        if !broken.is_empty() {
            out.flag_with(id, Warning, format!("{} broken external link(s)", broken.len()), cap(broken, 10));
        } else if blocked > 0 {
            out.flag(
                id,
                Info,
                format!(
                    "{blocked} external link(s) could not be verified (blocked or rate-limited by the remote server)"
                ),
            );
        } else {
            out.ok(id);
        }
    }

    // Resources, caching and image weight
    if !resources.is_empty() {
        let mut broken = Vec::new();
        let (mut same_origin_assets, mut uncached) = (0, Vec::new());
        let (mut heavy, mut huge) = (Vec::new(), false);
        for (kind, u) in &resources {
            let Some(p) = map.get(u.as_str()) else {
                continue;
            };
            if let State::Broken(e) = classify(p) {
                broken.push(format!("{} → {e}", truncate(u.as_str(), 80)));
                continue;
            }
            if same_site(u, page) && p.ok() {
                same_origin_assets += 1;
                if !p.cacheable {
                    uncached.push(truncate(u.as_str(), 80));
                }
            }
            if *kind == ResKind::Image {
                if let Some(len) = p.content_length.filter(|l| *l > 500_000) {
                    huge |= len > 2_000_000;
                    heavy.push(format!("{} ({})", truncate(u.as_str(), 70), human_bytes(len as usize)));
                }
            }
        }
        if n.filter.wants("links/resources") {
            if broken.is_empty() {
                out.ok("links/resources");
            } else {
                out.flag_with(
                    "links/resources",
                    Error,
                    format!("{} resource(s) failed to load", broken.len()),
                    cap(broken, 10),
                );
            }
        }
        if !n.local && same_origin_assets > 0 {
            if uncached.is_empty() {
                out.ok("perf/cache-headers");
            } else {
                out.flag_with(
                    "perf/cache-headers",
                    Warning,
                    format!("{} of {same_origin_assets} static asset(s) have no caching headers", uncached.len()),
                    cap(uncached, 5),
                );
            }
        }
        if resources.iter().any(|(k, _)| *k == ResKind::Image) {
            if heavy.is_empty() {
                out.ok("perf/img-weight");
            } else {
                out.flag_with(
                    "perf/img-weight",
                    if huge { Error } else { Warning },
                    format!("{} image(s) are larger than 500 KB", heavy.len()),
                    cap(heavy, 5),
                );
            }
        }
    }
    out
}
