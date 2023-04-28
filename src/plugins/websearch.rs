use reqwest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json;

use async_openai::{
    types::{
        ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs,
        CreateChatCompletionResponse, Role,
    },
    Client as OpenAiClient,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResult {
    title: String,
    link: String,
    snippet: String,
    text: String,
}
// Plugins data
pub fn get_plugin_data() -> String {
    "!WEB: Searches websites for a query, replies with the answered query".to_string()
}

/// Perform a DuckDuckGo Search and return the results
pub async fn duckduckgo(
    query: String,
    num_results: i32,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    // Get the search results
    let url = format!("https://api.duckduckgo.com/?q={}&format=json", query);
    let response = reqwest::get(&url).await;
    if !response.is_ok() {
        return Err(format!("Failed to fetch the URL: {}", &url).into());
    }

    // Parse the search results
    let json_string = response.unwrap().text().await?;
    let search: serde_json::Value = serde_json::from_str(&json_string)?;
    let mut results: Vec<SearchResult> = Vec::new();
    for index in 0..num_results {
        let result = &search["RelatedTopics"][index as usize];
        if result.is_null() {
            break;
        }
        if result["FirstURL"].is_null() {
            continue;
        }
        let title = result["Text"].to_string();
        let link = result["FirstURL"].to_string();
        let snippet = result["Result"].to_string();
        results.push(SearchResult {
            title,
            link,
            snippet,
            text: String::new(),
        });
    }

    return Ok(results);
}

pub async fn brave(query: String) -> Result<(), Box<dyn std::error::Error>> {
    let brave_search_url = format!("https://search.brave.com/search?q={}", query);

    let html = fetch_html(&brave_search_url).await?;
    let search_results = extract_search_results(&html);

    for (i, result) in search_results.iter().enumerate() {
        println!("{}. {}", i + 1, result);
    }

    Ok(())
}

async fn fetch_html(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?.text().await?;
    Ok(response)
}

fn extract_search_results(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    // Get div id results, inside this are the search results
    // class=snippet fdb for the articles
    // id=summarizer if there is a summarizer
    // Get div id side-right for the sidebar with infobox

    return vec![];
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

/// Perform a search with ai processing to answer a prompt
pub async fn ai_search(openai_client: &OpenAiClient, query: String) -> String {
    println!("Running ai_search with query: {}", query);

    // Get the search results
    let mut search_results = duckduckgo(query.clone(), 3).await.unwrap();

    // Get main content of each in parallel with tokio with timeout
    let timeout = std::time::Duration::from_secs(5);
    let mut main_texts: Vec<(String, String)> = Vec::new();
    println!("Ai search grabbing main texts");
    for result in &search_results {
        let url = result.link.clone();
        let main_text = tokio::time::timeout(timeout, get_main_text(&url)).await;
        if main_text.is_ok() {
            main_texts.push((url, main_text.unwrap().unwrap().clone()));
        }
    }
    // Wait for all to finish, wait timeout seconds
    tokio::time::sleep(timeout).await;
    println!("Ai search grabbed main texts");
    // Add all to the search_results.text
    let mut big_lengths = 0;
    for (url, main_text) in main_texts {
        for result in &mut search_results {
            if result.link == url {
                result.text = main_text.clone();
                big_lengths += main_text.len();
            }
        }
    }

    // Create a blob of text to send to the ai with all site data
    let mut blob = String::new();
    for (index, result) in search_results.iter().enumerate() {
        let mut text = result.snippet.clone();
        // If it has text, use that instead of snippet
        if result.text.len() > 0 {
            // Snip the text to have a limit of characters between all big texts
            let chars_available = (result.text.len() / big_lengths) * 6000;
            text = result.text.clone();
            if text.len() > chars_available {
                text = text[..chars_available].to_string();
            }
        }
        blob += &format!("[{}] {}: {}\n", index, result.link, text);
    }

    // Search with gpt through the example prompts for relevant ones
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .messages([
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content(blob)
                .build().unwrap(),
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(format!("Your answers should be on one line and compact with lists having comma separations\nBased on the given information, {query}"))
                .build().unwrap(),
        ])
        .build().unwrap();

    // Retry the request if it fails
    let mut tries = 0;
    let response = loop {
        let response = openai_client.chat().create(request.clone()).await;
        if let Ok(response) = response {
            break Ok(response);
        } else {
            tries += 1;
            if tries >= 3 {
                break response;
            }
        }
    };
    // Return from errors
    if let Err(error) = response {
        println!("Error: {:?}", error);
        return "Couldn't find an answer".to_string();
    }
    // TODO log the openai call and response
    let response: CreateChatCompletionResponse = response.unwrap();

    println!("Ai search response: {:?}", response);

    return response.choices.first().unwrap().message.content.clone();
}
