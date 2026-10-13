//! HTTP layer: fetches the page, probes resources and downloads small files.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE, EXPIRES, HeaderMap};
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder};
use url::Url;

pub struct FetchOptions {
    pub timeout: Duration,
    pub user_agent: String,
    pub insecure: bool,
}

/// The fetched page plus everything the checks need to know about the response.
#[derive(Debug)]
pub struct PageResponse {
    pub final_url: Url,
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
    /// Time until the response headers arrived (includes DNS/TLS/redirects).
    pub ttfb_ms: u128,
    pub total_ms: u128,
    /// Every URL visited after the initial request.
    pub redirects: Vec<String>,
}

/// Result of a lightweight HEAD (or GET fallback) request.
#[derive(Debug, Clone)]
pub struct Probe {
    pub url: Url,
    pub status: Option<u16>,
    pub final_url: Option<Url>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub cacheable: bool,
    pub error: Option<String>,
}

impl Probe {
    pub fn ok(&self) -> bool {
        self.error.is_none() && matches!(self.status, Some(200..=299))
    }

    pub fn describe(&self) -> String {
        match (&self.error, self.status) {
            (Some(e), _) => e.clone(),
            (None, Some(s)) => format!("HTTP {s}"),
            _ => "no response".into(),
        }
    }
}

/// A downloaded (size-capped) response body.
#[derive(Debug)]
pub struct Fetched {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub bytes: Vec<u8>,
}

impl Fetched {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

pub struct CompressionInfo {
    pub encoding: Option<String>,
    pub size: usize,
}

pub struct Fetcher {
    client: Client,
    /// Same as `client` but records redirect hops; used for the main page only.
    page_client: Client,
    /// Never decompresses, so `Content-Encoding` stays visible.
    raw_client: Client,
    hops: Arc<Mutex<Vec<String>>>,
}

fn builder(o: &FetchOptions) -> ClientBuilder {
    Client::builder()
        .user_agent(o.user_agent.clone())
        .timeout(o.timeout)
        .connect_timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(o.insecure)
}

impl Fetcher {
    pub fn new(o: &FetchOptions) -> Result<Fetcher> {
        let hops = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&hops);
        let policy = Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            if let Ok(mut h) = recorder.lock() {
                h.push(attempt.url().to_string());
            }
            attempt.follow()
        });
        Ok(Fetcher {
            client: builder(o).build()?,
            page_client: builder(o).redirect(policy).build()?,
            raw_client: builder(o).no_gzip().no_brotli().build()?,
            hops,
        })
    }

    pub async fn fetch_page(&self, url: &Url) -> Result<PageResponse> {
        if let Ok(mut h) = self.hops.lock() {
            h.clear();
        }
        let start = Instant::now();
        let resp = self
            .page_client
            .get(url.clone())
            .header(ACCEPT, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
            .send()
            .await
            .map_err(describe)?;
        let ttfb_ms = start.elapsed().as_millis();
        let status = resp.status().as_u16();
        let final_url = resp.url().clone();
        let headers = resp.headers().clone();
        let body = resp.text().await.map_err(describe)?;
        let redirects = self.hops.lock().map(|h| h.clone()).unwrap_or_default();
        Ok(PageResponse { final_url, status, headers, body, ttfb_ms, total_ms: start.elapsed().as_millis(), redirects })
    }

    /// HEAD request with a GET fallback for servers that reject HEAD.
    pub async fn probe(&self, url: &Url) -> Probe {
        let mut p = Probe {
            url: url.clone(),
            status: None,
            final_url: None,
            content_type: None,
            content_length: None,
            cacheable: false,
            error: None,
        };
        let first = self.client.head(url.clone()).send().await;
        let resp = match first {
            Ok(r) if !matches!(r.status().as_u16(), 400 | 403 | 405 | 406 | 501) => Ok(r),
            _ => self.client.get(url.clone()).send().await,
        };
        match resp {
            Ok(r) => {
                p.status = Some(r.status().as_u16());
                p.final_url = Some(r.url().clone());
                p.content_type = header_str(r.headers(), CONTENT_TYPE.as_str());
                p.content_length = r.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok());
                p.cacheable = is_cacheable(r.headers());
            }
            Err(e) => p.error = Some(short_error(&e)),
        }
        p
    }

    /// GET that stops reading after `cap` bytes.
    pub async fn get_limited(&self, url: &Url, cap: usize) -> Result<Fetched> {
        let mut resp = self.client.get(url.clone()).send().await.map_err(describe)?;
        let status = resp.status().as_u16();
        let content_type = header_str(resp.headers(), CONTENT_TYPE.as_str());
        let content_length = resp.content_length();
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(describe)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() >= cap {
                break;
            }
        }
        Ok(Fetched { status, content_type, content_length, bytes })
    }

    /// Asks for compressed content and reports what the server actually sent.
    pub async fn compression(&self, url: &Url) -> Result<CompressionInfo> {
        let resp = self
            .raw_client
            .get(url.clone())
            .header(ACCEPT_ENCODING, "gzip, br, zstd")
            .send()
            .await
            .map_err(describe)?;
        let encoding =
            header_str(resp.headers(), CONTENT_ENCODING.as_str()).filter(|e| !e.eq_ignore_ascii_case("identity"));
        let size = resp.bytes().await.map_err(describe)?.len();
        Ok(CompressionInfo { encoding, size })
    }
}

fn header_str(h: &HeaderMap, name: &str) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

fn is_cacheable(h: &HeaderMap) -> bool {
    if let Some(cc) = h.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()) {
        let cc = cc.to_ascii_lowercase();
        if cc.contains("no-store") || cc.contains("no-cache") {
            return false;
        }
        if cc.contains("immutable") {
            return true;
        }
        if let Some(i) = cc.find("max-age=") {
            let n: u64 = cc[i + 8..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0);
            return n > 0;
        }
    }
    h.contains_key(EXPIRES)
}

fn short_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_redirect() {
        "redirect loop".into()
    } else if e.is_connect() {
        "connection failed".into()
    } else {
        "request failed".into()
    }
}

/// Turns reqwest errors into messages that tell the user what to do next.
fn describe(e: reqwest::Error) -> anyhow::Error {
    let chain = format!("{e:?}").to_ascii_lowercase();
    if e.is_timeout() {
        anyhow!("the request timed out (raise --timeout if the server is just slow)")
    } else if chain.contains("certificate") || chain.contains("invalid peer") {
        anyhow!("TLS certificate problem — use --insecure to ignore it for local/staging servers")
    } else if e.is_connect() {
        anyhow!("could not connect — is the server running and is the URL correct?")
    } else if e.is_redirect() {
        anyhow!("too many redirects (possible redirect loop)")
    } else {
        anyhow!("{}", e.without_url())
    }
}
