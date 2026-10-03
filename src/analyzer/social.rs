use scraper::{Html, Selector};

pub fn analyze_social(html_content: &str) -> Vec<String> {
    let mut issues = Vec::new();
    let document = Html::parse_document(html_content);

    // Check for Open Graph tags
    let og_title_selector = Selector::parse("meta[property=\"og:title\"]").unwrap();
    if document.select(&og_title_selector).next().is_none() {
        issues.push("Missing Open Graph title tag".to_string());
    }

    // Check for Open Graph description tag
    let og_description_selector = Selector::parse("meta[property=\"og:description\"]").unwrap();
    if document.select(&og_description_selector).next().is_none() {
        issues.push("Missing Open Graph description tag".to_string());
    }

    // Check for Open Graph image tag
    let og_image_selector = Selector::parse("meta[property=\"og:image\"]").unwrap();
    if document.select(&og_image_selector).next().is_none() {
        issues.push("Missing Open Graph image tag".to_string());
    }

    return issues;
}
