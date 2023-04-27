use reqwest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResult {
    title: String,
    link: String,
    snippet: String,
}

// Plugins data
pub fn get_plugin_data() -> String {
    let mut data = String::new();
    data.push_str("WEB: Searches websites for a query, replies with the answered query\n");
    data
}

/// Perform a DuckDuckGo Search and return the results
pub async fn duckduckgo(
    query: String,
    num_results: i32,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    // Get the search results
    let url = format!("https://ddg-api.herokuapp.com/search?query={query}&limit={num_results}");
    let response = reqwest::get(&url).await;
    if !response.is_ok() {
        return Err(format!("Failed to fetch the URL: {}", &url).into());
    }

    let search = response.unwrap().text().await?;
    let search_results: Vec<SearchResult> = serde_json::from_str(&search).unwrap();
    return Ok(search_results);
}

/// Get the main text contents of a site
pub async fn get_main_text(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get(url).await;
    if !response.is_ok() {
        return Err(format!("Failed to fetch the URL: {}", url).into());
    }

    let body = response.unwrap().text().await?;
    let document = Html::parse_document(&body);

    // Modify the CSS selector according to the specific page structure you want to extract the content from
    let content_selector = Selector::parse("body")?;
    let content_element = document
        .select(&content_selector)
        .next()
        .ok_or_else(|| "Failed to find the main content element")?;

    Ok(content_element.text().collect::<String>())
}
