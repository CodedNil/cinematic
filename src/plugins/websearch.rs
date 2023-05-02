use regex::Regex;
use reqwest;
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json;
use std::error::Error;

use async_openai::{
    types::{
        ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs,
        CreateChatCompletionResponse, Role,
    },
    Client as OpenAiClient,
};

#[derive(Serialize, Debug)]
pub struct SearchResultBrave {
    title: String,
    link: String,
    snippet: String,
    rating: String,
    published: String,
}
#[derive(Serialize, Debug)]
pub struct SearchBrave {
    results: Vec<SearchResultBrave>,
    summary: String,
}

use crate::plugins::PluginReturn;

// Plugins data
pub fn get_plugin_data() -> String {
    "WEB: Searches websites for a query, replies with the answered query".to_string()
}

pub async fn brave(query: String) -> Result<SearchBrave, Box<dyn Error>> {
    let response_search =
        reqwest::get(format!("https://search.brave.com/search?q={}", query)).await;
    if !response_search.is_ok() {
        return Err("Failed to fetch brave search".into());
    }
    // Get the summarizer text if exists
    let response_summary = reqwest::get(format!(
        "https://search.brave.com/api/summarizer?key={}:us:en",
        query
    ))
    .await;

    let mut summary: Option<String> = None;
    if response_summary.is_ok() {
        // Get the text as json
        let json_string = response_summary.unwrap().text().await.unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_string).unwrap();
        // If json has ["results"][0]["text"] then use that as the summary
        if !json["results"][0]["text"].is_null() {
            let text = json["results"][0]["text"].as_str().unwrap_or("No summary");
            let regex = Regex::new(r#"<[^>]*>"#).unwrap();
            summary = Some(regex.replace_all(text, "").to_string());
        }
    }

    // Parse the search results
    let html_text = response_search.unwrap().text().await.unwrap();
    let document = Html::parse_document(&html_text);
    let selector = Selector::parse(".snippet").unwrap();

    let brave_organic_search_results: Vec<SearchResultBrave> = document
        .select(&selector)
        .filter_map(|element| {
            let title = element
                .select(&Selector::parse(".snippet-title").unwrap())
                .next()?
                .text()
                .collect::<String>()
                .trim()
                .to_string();

            if title.is_empty() {
                return None;
            }

            let link = element
                .select(&Selector::parse(".result-header").unwrap())
                .next()?
                .value()
                .attr("href")?
                .to_string();

            let raw_snippet = element
                .select(
                    &Selector::parse(
                        ".snippet-content .snippet-description , .snippet-description:nth-child(1)",
                    )
                    .unwrap(),
                )
                .next()?
                .text()
                .collect::<String>()
                .trim()
                .to_string();

            let (published, snippet) = if let Some(index) = raw_snippet.find(" -\n") {
                let (published, snippet) = raw_snippet.split_at(index);
                (
                    published.trim().to_string(),
                    snippet[2..].trim().to_string(),
                )
            } else {
                (String::new(), raw_snippet)
            };

            let rating = element
                .select(&Selector::parse(".ml-10").unwrap())
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default()
                .replace("\n", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            Some(SearchResultBrave {
                title,
                link,
                snippet,
                rating,
                published,
            })
        })
        .collect();

    return Ok(SearchBrave {
        results: brave_organic_search_results,
        summary: summary.unwrap_or_default(),
    });
}

/// Perform a search with ai processing to answer a prompt
pub async fn ai_search(openai_client: &OpenAiClient, query: String) -> PluginReturn {
    // Get the search results
    let mut search_results: SearchBrave = brave(query.clone()).await.unwrap();
    if search_results.results.is_empty() {
        return PluginReturn {
            result: format!("No results found for {}", query),
            to_user: format!("❌ No results found for {}", query),
        };
    }

    // Download a larger snippet for the first wikipedia result
    for (index, result) in search_results.results.iter().enumerate() {
        if result.link.contains("wikipedia.org") {
            // Swap link for wiki rest api
            let new_link = format!(
                "https://en.wikipedia.org/api/rest_v1/page/html/{}",
                result.link.split("/").last().unwrap()
            );
            let response = reqwest::get(new_link).await;
            if response.is_ok() {
                let body = response.unwrap().text().await.unwrap();
                // Scrape the html, only include paragraphs
                let document = Html::parse_document(&body);
                let selector = Selector::parse("p").unwrap();
                let mut text = document
                    .select(&selector)
                    .map(|element| element.text().collect::<String>())
                    .collect::<Vec<String>>()
                    .join("\n");
                // Trim every line
                text = text
                    .lines()
                    .map(|line| line.trim())
                    .collect::<Vec<&str>>()
                    .join("\n");
                text = text.replace("\n\n", " ");

                // Limit the text characters
                text = text.chars().take(4000).collect::<String>();
                // Replace the snippet with the new text
                search_results.results[index].snippet = text;
            }

            break;
        }
    }

    // Create a blob of text to send to the ai with all site data, with max character limit
    let mut blob = String::new();
    blob += &format!("Summary: {}\n", search_results.summary);
    for (index, result) in search_results.results.iter().enumerate() {
        blob += &format!(
            "[{}] {} ({}): {} {}\n",
            index, result.link, result.published, result.snippet, result.rating
        );
        // If the blob is too large, stop adding to it
        if blob.len() > 8192 {
            break;
        }
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
                .content(format!("Your answers should be on one line and compact with lists having comma separations, recently published articles should get priority\nBased on the given information and only this information, {query}"))
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
        return PluginReturn {
            result: String::from("Couldn't find an answer"),
            to_user: String::from("❌ Web search, couldn't find an answer"),
        };
    }
    // TODO log the openai call and response
    let response: CreateChatCompletionResponse = response.unwrap();

    return PluginReturn {
        result: response.choices.first().unwrap().message.content.clone(),
        to_user: format!("🔍 Web search ran for query {query}"),
    };
}
