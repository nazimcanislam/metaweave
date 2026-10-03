/// Fetches the content of the given URL as a String asynchronously.
///
/// ## Arguments
///
/// * `url` - A string slice that holds the URL to fetch.
/// 
/// ## Example
/// ```rust
/// let result = fetch_url("https://example.com").await;
/// assert!(result.is_ok() || result.is_err());
/// ```
///
/// ## Errors
///
/// Returns an error if the request fails, the server returns a non-200/404 status,
/// or if the response body cannot be read.
pub async fn fetch_url(url: &str) -> Result<String, anyhow::Error> {
    let response = reqwest::get(url).await?;

    match response.status() {
        reqwest::StatusCode::OK => {
            let text = response.text().await?;
            Ok(text)
        }
        reqwest::StatusCode::NOT_FOUND => {
            Err(anyhow::anyhow!("Dosya sunucuda mevcut değil (404)"))
        }
        _ => {
            Err(anyhow::anyhow!("Beklenmedik hata: {}", response.status()))
        }
    }
}
