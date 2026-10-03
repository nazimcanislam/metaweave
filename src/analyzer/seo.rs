use scraper::{Html, Selector};

pub fn analyze_seo(html_content: &str) -> Vec<String> {
    let mut issues = Vec::new();
    let document = Html::parse_document(html_content);

    // Check for title tag
    let title_selector = Selector::parse("title").unwrap();
    if document.select(&title_selector).next().is_none() {
        issues.push("Missing title tag".to_string());
    }

    // Check for meta charset tag
    let meta_charset_selector = Selector::parse("meta[charset]").unwrap();
    if document.select(&meta_charset_selector).next().is_none() {
        issues.push("Missing meta tag with charset attribute".to_string());
    }

    // Check for meta description tag
    let meta_description_selector = Selector::parse("meta[name=\"description\"]").unwrap();
    if document.select(&meta_description_selector).next().is_none() {
        issues.push("Missing meta description tag".to_string());
    }

    return issues;
}
