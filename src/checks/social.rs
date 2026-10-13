use url::Url;

use super::Ctx;
use crate::model::{
    Out,
    Severity::{self, *},
};
use crate::util::{host_key, is_local};

pub fn run(c: &Ctx, out: &mut Out) {
    required(c, out, "social/og-title", "og:title", Error);
    required(c, out, "social/og-description", "og:description", Warning);
    og_image(c, out);
    required(c, out, "social/og-image-alt", "og:image:alt", Info);
    og_url(c, out);
    required(c, out, "social/og-type", "og:type", Info);
    required(c, out, "social/og-site-name", "og:site_name", Info);
    twitter(c, out);
}

fn required(c: &Ctx, out: &mut Out, id: &'static str, key: &str, missing: Severity) {
    if c.meta.get(key).is_some() {
        out.ok(id);
    } else if c.meta.present(key) {
        out.flag(id, missing, format!("{key} is present but empty"));
    } else {
        out.flag(id, missing, format!("Missing {key}"));
    }
}

fn og_image(c: &Ctx, out: &mut Out) {
    let id = "social/og-image";
    let Some(raw) = c.meta.get("og:image") else {
        return out.flag(id, Error, "Missing og:image");
    };
    match Url::parse(raw) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            if is_local(&u) && !c.local {
                out.flag(id, Error, format!("og:image points at a local address ({u}) — crawlers can't reach it"));
            } else {
                out.ok(id);
            }
        }
        _ => out.flag(
            id,
            Error,
            format!("og:image “{raw}” is not an absolute http(s) URL — most crawlers ignore relative paths"),
        ),
    }
}

fn og_url(c: &Ctx, out: &mut Out) {
    let id = "social/og-url";
    let Some(raw) = c.meta.get("og:url") else {
        return out.flag(id, Warning, "Missing og:url");
    };
    let Ok(u) = Url::parse(raw) else {
        return out.flag(id, Warning, format!("og:url “{raw}” is not an absolute URL"));
    };
    if is_local(&u) && !c.local {
        return out.flag(id, Error, format!("og:url points at a local address ({u})"));
    }
    let canonical = c
        .doc
        .select(&crate::util::sel("link[rel~=\"canonical\"]"))
        .next()
        .and_then(|e| e.value().attr("href"))
        .and_then(|h| Url::parse(h).ok());
    if let Some(canon) = canonical {
        if canon.as_str().trim_end_matches('/') != u.as_str().trim_end_matches('/') && host_key(&canon) == host_key(&u)
        {
            return out.flag(id, Info, format!("og:url ({u}) differs from the canonical URL ({canon})"));
        }
    }
    out.ok(id);
}

fn twitter(c: &Ctx, out: &mut Out) {
    let id = "social/twitter-card";
    match c.meta.get("twitter:card") {
        None => out.flag(id, Warning, "Missing twitter:card — X/Twitter will not render a rich preview"),
        Some(v) if !["summary", "summary_large_image", "app", "player"].contains(&v) => {
            out.flag(id, Warning, format!("twitter:card “{v}” is not a valid value"))
        }
        Some(_) => out.ok(id),
    }
}
