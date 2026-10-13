use super::Ctx;
use crate::model::{Out, Severity::*};
use crate::util::{cap, resolve, sel, truncate};

pub fn run(c: &Ctx, out: &mut Out) {
    mixed_content(c, out);
    sri(c, out);
    target_blank(c, out);
    if c.local {
        return; // response headers of a dev server say nothing about production
    }
    hsts(c, out);
    csp(c, out);
    nosniff(c, out);
    frame_protection(c, out);
    referrer_policy(c, out);
    permissions_policy(c, out);
    cookies(c, out);
    server_header(c, out);
}

fn csp_value(c: &Ctx) -> Option<String> {
    c.header("content-security-policy").map(str::to_string).or_else(|| {
        c.doc
            .select(&sel("meta[http-equiv]"))
            .find(|m| m.value().attr("http-equiv").is_some_and(|h| h.eq_ignore_ascii_case("content-security-policy")))
            .and_then(|m| m.value().attr("content").map(str::to_string))
    })
}

fn hsts(c: &Ctx, out: &mut Out) {
    if !c.https() {
        return;
    }
    let id = "sec/hsts";
    let Some(v) = c.header("strict-transport-security") else {
        return out.flag(id, Warning, "Missing Strict-Transport-Security header");
    };
    let lower = v.to_ascii_lowercase();
    let max_age: u64 = lower
        .split("max-age=")
        .nth(1)
        .map(|s| s.chars().take_while(char::is_ascii_digit).collect::<String>())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if max_age < 15_552_000 {
        out.flag(id, Warning, format!("HSTS max-age is {max_age}s (recommended ≥ 15552000, i.e. 6 months)"));
    } else {
        out.ok(id);
    }
}

fn csp(c: &Ctx, out: &mut Out) {
    let id = "sec/csp";
    let Some(policy) = csp_value(c) else {
        return out.flag(id, Info, "No Content-Security-Policy found");
    };
    let lower = policy.to_ascii_lowercase();
    let script_directive = lower
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src"))
        .or_else(|| lower.split(';').map(str::trim).find(|d| d.starts_with("default-src")))
        .unwrap_or("");
    let mut weak = Vec::new();
    if script_directive.contains("'unsafe-inline'")
        && !script_directive.contains("'nonce-")
        && !script_directive.contains("'strict-dynamic'")
    {
        weak.push("'unsafe-inline' allows any inline script");
    }
    if script_directive.contains("'unsafe-eval'") {
        weak.push("'unsafe-eval' allows eval()");
    }
    if weak.is_empty() {
        out.ok(id);
    } else {
        out.flag(id, Warning, format!("CSP is weakened: {}", weak.join("; ")));
    }
}

fn nosniff(c: &Ctx, out: &mut Out) {
    if c.header("x-content-type-options").is_some_and(|v| v.eq_ignore_ascii_case("nosniff")) {
        out.ok("sec/nosniff");
    } else {
        out.flag("sec/nosniff", Warning, "Missing X-Content-Type-Options: nosniff");
    }
}

fn frame_protection(c: &Ctx, out: &mut Out) {
    let has_csp = csp_value(c).is_some_and(|p| p.to_ascii_lowercase().contains("frame-ancestors"));
    if has_csp || c.header("x-frame-options").is_some() {
        out.ok("sec/frame-protection");
    } else {
        out.flag(
            "sec/frame-protection",
            Warning,
            "Neither X-Frame-Options nor CSP frame-ancestors is set — the page can be framed by other sites",
        );
    }
}

fn referrer_policy(c: &Ctx, out: &mut Out) {
    if c.header("referrer-policy").is_some() || c.meta.get("referrer").is_some() {
        out.ok("sec/referrer-policy");
    } else {
        out.flag("sec/referrer-policy", Info, "No Referrer-Policy set");
    }
}

fn permissions_policy(c: &Ctx, out: &mut Out) {
    if c.header("permissions-policy").is_some() || c.header("feature-policy").is_some() {
        out.ok("sec/permissions-policy");
    } else {
        out.flag("sec/permissions-policy", Info, "No Permissions-Policy set");
    }
}

fn mixed_content(c: &Ctx, out: &mut Out) {
    if !c.https() {
        return;
    }
    let selectors = [
        ("script[src]", "src"),
        ("link[rel~=\"stylesheet\"][href]", "href"),
        ("img[src]", "src"),
        ("iframe[src]", "src"),
        ("source[src]", "src"),
        ("video[src]", "src"),
        ("audio[src]", "src"),
        ("link[rel~=\"icon\"][href]", "href"),
    ];
    let mut bad = Vec::new();
    for (selector, attr) in selectors {
        for el in c.doc.select(&sel(selector)) {
            if let Some(v) =
                el.value().attr(attr).filter(|v| v.trim_start().to_ascii_lowercase().starts_with("http://"))
            {
                bad.push(truncate(v.trim(), 80));
            }
        }
    }
    if bad.is_empty() {
        out.ok("sec/mixed-content");
    } else {
        out.flag_with(
            "sec/mixed-content",
            Error,
            format!("{} http:// resource(s) on an HTTPS page", bad.len()),
            cap(bad, 5),
        );
    }
}

/// Hosts whose responses change per browser/version, so a fixed integrity hash cannot work.
const SRI_EXEMPT: [&str; 2] = ["fonts.googleapis.com", "use.typekit.net"];

fn sri(c: &Ctx, out: &mut Out) {
    let page_host = c.page.final_url.host_str().unwrap_or("").to_ascii_lowercase();
    let mut cross = 0;
    let mut bad = Vec::new();
    for (selector, attr) in [("script[src]", "src"), ("link[rel~=\"stylesheet\"][href]", "href")] {
        for el in c.doc.select(&sel(selector)) {
            let Some(url) = el.value().attr(attr).and_then(|v| resolve(&c.base, v)) else {
                continue;
            };
            let exempt = url.host_str().is_some_and(|h| SRI_EXEMPT.contains(&h.to_ascii_lowercase().as_str()));
            if !exempt && url.host_str().is_some_and(|h| !h.eq_ignore_ascii_case(&page_host)) {
                cross += 1;
                if el.value().attr("integrity").is_none() {
                    bad.push(truncate(url.as_str(), 80));
                }
            }
        }
    }
    if cross == 0 {
        return;
    }
    if bad.is_empty() {
        out.ok("sec/sri");
    } else {
        out.flag_with(
            "sec/sri",
            Warning,
            format!("{} third-party resource(s) have no integrity attribute", bad.len()),
            cap(bad, 5),
        );
    }
}

fn target_blank(c: &Ctx, out: &mut Out) {
    let links: Vec<_> = c.doc.select(&sel("a[target=\"_blank\"]")).collect();
    if links.is_empty() {
        return;
    }
    let bad: Vec<String> = links
        .iter()
        .filter(|a| {
            let rel = a.value().attr("rel").unwrap_or("").to_ascii_lowercase();
            !(rel.contains("noopener") || rel.contains("noreferrer"))
        })
        .map(|a| truncate(a.value().attr("href").unwrap_or(""), 70))
        .collect();
    if bad.is_empty() {
        out.ok("sec/target-blank");
    } else {
        out.flag_with(
            "sec/target-blank",
            Info,
            format!("{} target=\"_blank\" link(s) lack rel=\"noopener\"", bad.len()),
            cap(bad, 5),
        );
    }
}

fn cookies(c: &Ctx, out: &mut Out) {
    let cookies: Vec<String> =
        c.page.headers.get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).map(str::to_string).collect();
    if cookies.is_empty() {
        return;
    }
    let mut worst = Info;
    let mut details = Vec::new();
    for cookie in &cookies {
        let lower = cookie.to_ascii_lowercase();
        let name = cookie.split('=').next().unwrap_or("?").trim().to_string();
        let mut missing = Vec::new();
        if c.https() && !lower.contains("; secure") {
            missing.push("Secure");
            worst = Warning;
        }
        if !lower.contains("httponly") {
            missing.push("HttpOnly");
        }
        if !lower.contains("samesite") {
            missing.push("SameSite");
        }
        if !missing.is_empty() {
            details.push(format!("{name}: missing {}", missing.join(", ")));
        }
    }
    if details.is_empty() {
        out.ok("sec/cookies");
    } else {
        out.flag_with(
            "sec/cookies",
            worst,
            format!("{} cookie(s) lack recommended flags", details.len()),
            cap(details, 5),
        );
    }
}

fn server_header(c: &Ctx, out: &mut Out) {
    let mut leaks = Vec::new();
    if let Some(s) = c.header("server") {
        if s.chars().any(|ch| ch.is_ascii_digit()) {
            leaks.push(format!("Server: {s}"));
        }
    }
    if let Some(p) = c.header("x-powered-by") {
        leaks.push(format!("X-Powered-By: {p}"));
    }
    if leaks.is_empty() {
        out.ok("sec/server-header");
    } else {
        out.flag_with("sec/server-header", Info, "Response headers reveal server software", leaks);
    }
}
