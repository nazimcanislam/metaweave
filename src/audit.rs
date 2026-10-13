//! Runs every check against one URL and builds the report.

use anyhow::Result;
use scraper::Html;
use url::Url;

use crate::checks::{self, Ctx, net::NetCtx};
use crate::extract;
use crate::fetcher::Fetcher;
use crate::filter::Filter;
use crate::model::{Out, Report};
use crate::{ui, util};

pub async fn run(fetcher: &Fetcher, url: &Url, filter: &Filter, site: bool, max_links: usize) -> Result<Report> {
    ui::step(format!("Fetching {url}"));
    let page = fetcher.fetch_page(url).await?;

    ui::step(format!("Analysing HTML ({})", util::human_bytes(page.body.len())));
    // The parsed DOM is not Send, so it lives only inside this block (never across an await).
    let (mut out, extracted) = {
        let doc = Html::parse_document(&page.body);
        let ctx = Ctx::new(&page, &doc);
        let mut out = Out::default();
        checks::run_static(&ctx, &mut out);
        (out, extract::extract(&ctx))
    };

    ui::step("Probing site files, icons, images and links");
    let net = checks::net::run(&NetCtx {
        fetcher,
        page: &page,
        x: &extracted,
        filter,
        local: util::is_local(&page.final_url),
        site,
        max_links,
    })
    .await;
    out.merge(net);

    out.retain(|id| filter.wants(id));
    Ok(Report::build(url, &page, out))
}
