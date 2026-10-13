//! Small helpers shared across modules.

use anyhow::{Context, Result, bail};
use scraper::{ElementRef, Selector};
use url::{Host, Url};

/// Parses a CSS selector that is known to be valid at compile time.
pub fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|e| panic!("invalid selector {s:?}: {e}"))
}

/// Visible text of an element with whitespace collapsed.
pub fn text_of(el: ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

/// Keeps at most `max` entries and appends “… and N more”.
pub fn cap(mut v: Vec<String>, max: usize) -> Vec<String> {
    if v.len() > max {
        let extra = v.len() - max;
        v.truncate(max);
        v.push(format!("… and {extra} more"));
    }
    v
}

/// True for localhost, loopback and private-network addresses (dev servers).
pub fn is_local(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local") || d.ends_with(".test")
        }
        Some(Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_unspecified() || ip.is_link_local(),
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        None => false,
    }
}

/// Host without a leading `www.`, lower-cased, for “same site” comparisons.
pub fn host_key(url: &Url) -> String {
    url.host_str().unwrap_or("").trim_start_matches("www.").to_ascii_lowercase()
}

/// Resolves `raw` against `base`, keeping only http(s) URLs and dropping the fragment.
pub fn resolve(base: &Url, raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('#') {
        return None;
    }
    let mut url = base.join(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

pub fn human_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    }
}

/// Turns user input into a URL: `example.com` → https, `localhost:3000` → http.
pub fn normalize_url(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    let candidate = if raw.contains("://") {
        raw.to_string()
    } else {
        let host = raw.split(['/', '?', '#']).next().unwrap_or("");
        let host =
            host.rsplit_once(':').map_or(host, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host });
        let local = host.starts_with('[')
            || host == "localhost"
            || host.starts_with("127.")
            || host == "0.0.0.0"
            || host.ends_with(".local")
            || host.ends_with(".localhost");
        format!("{}://{raw}", if local { "http" } else { "https" })
    };
    let url = Url::parse(&candidate).with_context(|| format!("'{raw}' is not a valid URL"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => bail!("unsupported scheme '{other}' in '{raw}' (only http and https)"),
    }
    if url.host_str().is_none() {
        bail!("'{raw}' has no host name");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        assert_eq!(normalize_url("example.com").unwrap().as_str(), "https://example.com/");
        assert_eq!(normalize_url("localhost:3000/a").unwrap().as_str(), "http://localhost:3000/a");
        assert_eq!(normalize_url("http://example.com").unwrap().scheme(), "http");
        assert!(normalize_url("ftp://example.com").is_err());
    }

    #[test]
    fn detects_local_hosts() {
        assert!(is_local(&Url::parse("http://localhost:8080").unwrap()));
        assert!(is_local(&Url::parse("http://192.168.1.20").unwrap()));
        assert!(!is_local(&Url::parse("https://example.com").unwrap()));
    }

    #[test]
    fn resolves_only_web_links() {
        let base = Url::parse("https://example.com/a/b").unwrap();
        assert_eq!(resolve(&base, "../c#x").unwrap().as_str(), "https://example.com/c");
        assert!(resolve(&base, "mailto:a@b.c").is_none());
        assert!(resolve(&base, "javascript:void(0)").is_none());
        assert!(resolve(&base, "#top").is_none());
    }

    #[test]
    fn caps_lists() {
        let v = cap((0..5).map(|i| i.to_string()).collect(), 2);
        assert_eq!(v, vec!["0", "1", "… and 3 more"]);
    }
}
