use crate::fetcher::fetch_url;

pub async fn analyze_assets(url: &str) -> Vec<String> {
    let mut issues = Vec::new();

    // Check for robots.txt file
    let robots_url = format!("{}/robots.txt", &url);
    let response = fetch_url(&robots_url);

    match response.await {
        Ok(_) => {},
        Err(_) => issues.push("Missing robots.txt".to_string()),
    }

    return issues;
}
