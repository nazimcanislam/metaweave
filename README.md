<div align="center">

# metaweave

**Check your website before it ships.**

SEO · social previews · accessibility · performance · security · broken links —
**71 checks in one command**, with plain-language fixes.

[Documentation](https://nazimcanislam.github.io/metaweave/) ·
[Install](#install) ·
[Usage](#usage) ·
[All checks](#what-it-checks)

</div>

---

Lighthouse is great, but a lot of launch-day mistakes live outside its reports: the `og:image` that returns 404,
the favicon nobody uploaded, the canonical URL that still says `localhost`, the `robots.txt` that blocks the
whole site, the sitemap full of staging URLs. You usually notice them **after** deploying — if you remember to
look at all.

`metaweave` fetches a page, runs every check in seconds and tells you exactly what is wrong and how to fix it.

```text
$ metaweave staging.example.com

  ▸ https://staging.example.com/
    HTTP 200 · 212 ms to first byte · 18.4 KB

  Social previews      ██████████████░░░░░░  70  6/10 passed
    ✖ social/og-image-reachable   og:image returns HTTP 404
        → Make sure the file is deployed, publicly accessible and served with an image/* content type.
    ▲ social/twitter-card         Missing twitter:card — X/Twitter will not render a rich preview
        → Add <meta name="twitter:card" content="summary_large_image">.

  Site files & icons   ████████████░░░░░░░░  62  3/7 passed
    ✖ assets/robots-txt           robots.txt disallows the entire site for all crawlers (Disallow: /)
        → Serve /robots.txt with sensible rules and a Sitemap: line; never Disallow: / in production.

  ────────────────────────────────────────────────────────────────
  61 checks · 51 passed · 3 errors · 3 warnings · 4 info
  Not ready to ship — fix 3 errors.   Score 82/100
```

## Features

- **71 checks in 8 categories** — SEO, social previews, accessibility, performance, security, site files & icons,
  links & resources, HTML hygiene.
- **Made for pre-production** — point it at `localhost:3000` or a preview deploy. Checks that only make sense on a
  live site (HSTS, TTFB, caching…) are skipped for local servers, so there is no noise.
- **Actionable output** — every finding has a severity, a message, concrete details and a *how to fix it* line.
  `metaweave explain <id>` tells you why a check exists.
- **Real verification, not just tag-spotting** — it downloads your `og:image` and measures it, probes favicons,
  validates the manifest, parses `robots.txt` and the sitemap, requests a non-existent URL to detect soft 404s,
  and verifies links and resources in parallel.
- **Scores** per category and overall, so you can track progress.
- **CI-friendly** — JSON and Markdown output, `--fail-on`, meaningful exit codes, `NO_COLOR` support.
- **Fast and tiny** — one native binary, async I/O, no browser, no Node.

## Install

You need [Rust](https://rustup.rs) 1.85 or newer.

```bash
# straight from GitHub
cargo install --git https://github.com/nazimcanislam/metaweave

# or from a clone
git clone https://github.com/nazimcanislam/metaweave
cd metaweave
cargo install --path .
```

Shell completions (bash, zsh, fish, powershell, elvish):

```bash
metaweave completions zsh > ~/.zfunc/_metaweave
```

## Usage

```bash
metaweave https://example.com                 # run every check
metaweave localhost:3000                      # dev server (http is assumed for local hosts)
metaweave example.com example.com/pricing     # several pages; site-wide checks run once per origin
metaweave example.com --only seo,social       # only some categories
metaweave example.com --skip links            # skip the slow link crawl
metaweave example.com -i 'sec/*' -i seo/h1    # silence checks (ids or prefix wildcards)
metaweave example.com -f json > report.json   # machine-readable
metaweave example.com --fail-on warning       # exit 1 on warnings too
metaweave list                                # show all checks
metaweave explain social/og-image             # why it matters + how to fix
```

### Options

| Option | Description |
|---|---|
| `-f, --format <text\|json\|markdown>` | Output format (default `text`) |
| `--only <categories>` / `--skip <categories>` | Comma-separated: `seo`, `social`, `a11y`, `perf`, `sec`, `assets`, `links`, `html` |
| `-i, --ignore <id>` | Silence a check by id; prefix wildcards such as `sec/*` work; repeatable |
| `--fail-on <error\|warning\|info\|never>` | Severity that makes the exit code `1` (default `error`) |
| `--max-links <n>` | Maximum number of links to verify (default `100`) |
| `--timeout <secs>` | Per-request timeout (default `15`) |
| `--user-agent <ua>` | Custom `User-Agent` |
| `--insecure` | Accept invalid TLS certificates (self-signed dev/staging servers) |
| `-v, --verbose` / `-q, --quiet` | List passed checks / show only errors and warnings |
| `--no-color` | Disable colors (`NO_COLOR` is honoured as well) |

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Nothing at or above `--fail-on` |
| `1` | At least one finding at or above `--fail-on` |
| `2` | A page could not be fetched, or an argument was invalid |

### Severity and scoring

Each check either passes or produces **one** finding with a severity: **error** (likely broken in production),
**warning** (should be fixed) or **info** (worth knowing). A passed check scores 100%, an error 0%, a warning 50%
and an info 90%; category and overall scores are the average over the checks that ran. A check that cannot apply
to a page (for example, image checks on a page without images) is not counted.

## What it checks

| Category | Examples |
|---|---|
| **SEO** (8) | title and description length, canonical (and leaked `localhost` URLs), single `h1`, leftover `noindex`, valid JSON-LD, viewport, “click here” links |
| **Social previews** (10) | `og:title/description/image/url/type/site_name`, image alt, **og:image reachable, an actual image, ≥ 600×315, ≤ 5 MB, not SVG**, `twitter:card` |
| **Accessibility** (13) | `lang`, image alt, link/button/form names, heading order, zoom lock, duplicate ids, `<main>`, iframe titles, captions, positive `tabindex`, skip link |
| **Performance** (11) | TTFB, HTML size, compression, redirect chains, render-blocking scripts, image dimensions / lazy-loading / formats / weight, DOM size, cache headers |
| **Security** (12) | HTTPS + HTTP redirect, HSTS, CSP, `nosniff`, clickjacking protection, Referrer-/Permissions-Policy, mixed content, SRI, `rel=noopener`, cookie flags, server banners |
| **Site files & icons** (7) | favicon, Apple touch icon, web manifest, `theme-color`, `robots.txt` (not blocking everything), `sitemap.xml` (production hosts only), real 404 status |
| **Links & resources** (5) | broken internal/external links, failing images/scripts/styles, missing `#anchors`, placeholder `href="#"` |
| **HTML hygiene** (5) | 2xx status, doctype, UTF-8, obsolete tags, `meta refresh` |

Run `metaweave list` for the complete catalogue or see the [documentation](https://nazimcanislam.github.io/metaweave/#checks).

## Use it in CI

```yaml
- name: Check the preview deploy
  run: |
    metaweave "$PREVIEW_URL" -f markdown --fail-on error | tee -a "$GITHUB_STEP_SUMMARY"
```

## Limitations

metaweave analyses the HTML the server returns. It does **not** run JavaScript, so client-rendered content, runtime
metadata and layout-based metrics (LCP, CLS, color contrast) are out of scope — keep using Lighthouse for those.
Timing numbers come from a single request from your machine and are a hint, not a benchmark.

## Development

```bash
cargo test                          # unit tests
cargo run -- localhost:3000 -v      # try it
python3 scripts/gen_docs_checks.py  # refresh the check list in docs/index.html after editing src/catalog.rs
```

Adding a check: describe it in `src/catalog.rs` (id, why, fix), implement it in the matching file under
`src/checks/` and report either `out.ok(id)` or `out.flag(id, severity, message)`.

The project website lives in [`docs/`](docs/) — plain HTML and CSS (including the light/dark theme switch), no
build step, no JavaScript. Enable it with *Settings → Pages → Deploy from a branch → `main` → `/docs`*.

## License

[MIT](LICENSE)

---

## Claude işbirliği

Bu proje, [Claude](https://claude.ai) (Anthropic) ile birlikte geliştirildi. Fikir, ürün hedefleri ve ilk Rust
iskeleti Nazımcan İslam'a ait; Claude ise mimariyi genişletme, kontrol kataloğunun ve CLI'ın yazılması, testler, dokümantasyon
sitesi (`docs/`) ve bu README üzerinde çalıştı. Kodun tamamı gözden geçirilmek ve geliştirilmek üzere açıktır.
