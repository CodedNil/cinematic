use crate::{
    apis,
    discordbot::{box_future, Func, Param},
};
use anyhow::anyhow;
use futures::Future;
use regex::Regex;
use reqwest;
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json;
use std::{collections::HashMap, pin::Pin};

#[derive(Serialize, Debug)]
struct SearchResultBrave {
    title: String,
    link: String,
    snippet: String,
    rating: String,
    published: String,
}
#[derive(Serialize, Debug)]
struct SearchBrave {
    results: Vec<SearchResultBrave>,
    summary: String,
}

/// Get available functions
pub fn get_functions() -> Vec<Func> {
    // Create the functions
    vec![Func::new(
        "web_search",
        "Search web for query",
        vec![Param::new(
            "query",
            "A query for information to be answered, phrased as a question",
        )],
        |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
            let query = args.get("query").unwrap().to_string();
            box_future(async move { ai_search(query).await })
        },
    )]
}

async fn brave(query: String) -> anyhow::Result<SearchBrave> {
    let response_search = reqwest::get(format!("https://search.brave.com/search?q={query}")).await;
    if response_search.is_err() {
        return Err(anyhow!("Failed to fetch brave search"));
    }

    // Get the summarizer text if exists
    let response_summary = reqwest::get(format!(
        "https://search.brave.com/api/summarizer?key={query}:us:en"
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
            let regex = Regex::new(r"<[^>]*>").unwrap();
            summary = Some(regex.replace_all(text, "").to_string());
        }
    }

    // Parse the search results
    let html_text = response_search.unwrap().text().await.unwrap();
    let document = Html::parse_document(&html_text);

    let brave_organic_search_results: Vec<SearchResultBrave> = document
        .select(&Selector::parse(".snippet").unwrap())
        .filter_map(|element| {
            let title = element
                .select(&Selector::parse(".title").unwrap())
                .next()?
                .text()
                .collect::<String>()
                .trim()
                .to_string();

            if title.is_empty() {
                return None;
            }

            let link = element
                .select(&Selector::parse(".h").unwrap())
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

            let (published, snippet) = raw_snippet.find(" - ").map_or_else(
                || (String::new(), raw_snippet.to_string()),
                |index| {
                    let (p, s) = raw_snippet.split_at(index);
                    (p.trim().to_string(), s[2..].trim().to_string())
                },
            );

            let rating = element
                .select(&Selector::parse(".ml-10").unwrap())
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default()
                .replace('\n', "")
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

    Ok(SearchBrave {
        results: brave_organic_search_results,
        summary: summary.unwrap_or_default(),
    })
}

/// Perform a search with ai processing to answer a prompt
async fn ai_search(query: String) -> anyhow::Result<String> {
    // Get the search results
    let mut search_results: SearchBrave = brave(query.clone()).await.unwrap();
    if search_results.results.is_empty() {
        return Err(anyhow!("No results found"));
    }

    // Download a larger snippet for the first wikipedia result
    for (index, result) in search_results.results.iter().enumerate() {
        if result.link.contains("wikipedia.org") {
            // Swap link for wiki rest api
            let new_link = format!(
                "https://en.wikipedia.org/api/rest_v1/page/html/{}",
                result.link.split('/').last().unwrap()
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
                    .map(str::trim)
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

    // Search with gpt through the blob to answer the query
    let response = apis::gpt_info_query("gpt-4-turbo".to_string(), blob, format!("Your answers should be on one line and compact with lists having comma separations, recently published articles should get priority, answer verbosely with the question included in the answer\nBased on the given information and only this information, {query}")).await;
    // Return from errors
    if response.is_err() {
        return Err(anyhow!("Couldn't find an answer"));
    }
    Ok(response.unwrap().replace('\n', " "))
}
