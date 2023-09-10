use crate::{
    apis,
    discordbot::{box_future, Func, Param},
};
use anyhow::anyhow;
use futures::Future;
use serde_json::Value;
use std::{collections::HashMap, pin::Pin};

#[derive(Debug, Clone)]
enum Format {
    Movie,
    Series,
}
impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Self::Movie => write!(f, "Movie"),
            Self::Series => write!(f, "Series"),
        }
    }
}

/// Get available functions
#[allow(clippy::too_many_lines)]
pub fn get_functions() -> Vec<Func> {
    // Common parameters for the functions
    let format_param = Param::new("format", "The format of the media to be searched for")
        .with_enum_values(&["movie", "series"]);
    let quality_param = Param::new(
        "quality",
        "The quality to set the media to, default to 1080p if not specified",
    )
    .with_enum_values(&["SD", "720p", "1080p", "2160p", "720p/1080p", "Any"]);
    let id_param = Param::new("id", "The id of the media item");

    // Create the functions
    vec![
        Func::new(
            "media_query",
            "Performs a query against media on the server",
            vec![
                format_param.clone(),
                Param::new(
                    "query",
                    "A query for information to be answered, phrased as a question, for example \"What action movies are available?\"",
                ),
                Param::new(
                    "details",
                    "Details to be included in the search, comma separated list from the following (use as few as possible, 3 at most): \"quality,added_by,database_id,file_details,genres\"",
                ),
            ],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let query = args.get("query").unwrap().to_string();
                let details = args.get("details").unwrap().to_string();
                box_future(async move { query_server(format, query, details).await })
            },
        ),
        Func::new(
            "media_lookup",
            "Search the media server for query information about a piece of media",
            vec![
                format_param.clone(),
                Param::new(
                    "searches",
                    "List of movies/series to search for separated by pipe |, for example \"Game of Thrones|Watchmen|Cats\"",
                ),
                Param::new(
                    "query",
                    "A query for information to be answered, query should be phrased as a question, for example \"Available on the server?\" \"Is series Watchmen available on the server in the Ultimate Cut?\" \"What is Cats movie tmdbId/tvdbId?\" \"Who added series Game of Thrones to the server?\" \"What is series Game of Thrones tmdbId/tvdbId?\", if multiple results are returned, ask user for clarification",
                ),
            ],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let searches = args.get("searches").unwrap().to_string();
                let query = args.get("query").unwrap().to_string();
                box_future(async move { lookup(format, searches, query).await })
            },
        ),
        Func::new(
            "media_add",
            "Adds media to the server and mark it as wanted by user, if media is already on server it just marks as wanted, perform a lookup first to get the tmdbId/tvdbId",
            vec![
                format_param.clone(),
                Param::new("db_id", "The tmdbId/tvdbId of the media item"),
                quality_param.clone(),
            ],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let db_id = args.get("db_id").unwrap().to_string();
                let quality = args.get("quality").unwrap().to_string();
                let user_name = args.get("user_name").unwrap().to_string();
                box_future(async move { add(format, db_id, &user_name, quality).await })
            },
        ),
        Func::new(
            "media_setres",
            "Change the targeted resolution of a piece of media on the server, perform a lookup first to get the id on server (not the tmdbId/tvdbId)",
            vec![format_param.clone(), id_param.clone(), quality_param],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let id = args.get("id").unwrap().to_string();
                let quality = args.get("quality").unwrap().to_string();
                box_future(async move { setres(format, id, quality).await })
            },
        ),
        Func::new(
            "media_remove",
            "Removes media from users requests, media items remain on the server if another user has requested also, perform a lookup first to get the id on server (not the tmdbId/tvdbId)",
            vec![format_param.clone(), id_param],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let id = args.get("id").unwrap().to_string();
                let user_name = args.get("user_name").unwrap().to_string();
                box_future(async move { remove(format, id, &user_name).await })
            },
        ),
        Func::new(
            "media_wanted",
            "Returns a list of series that user or noone has requested ... Aim for the most condensed list while retaining clarity knowing that the user can always request more specific detail.",
            vec![
                format_param,
                Param::new(
                    "user",
                    "Self for the user that spoke, none to get a list of movies or series that noone has requested",
                )
                .with_enum_values(&["self", "none"]),
            ],
            |args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                let format = get_format(args);
                let user = args.get("user").unwrap().to_string();
                let user_name = args.get("user_name").unwrap().to_string();
                box_future(async move { wanted(format, user, &user_name).await })
            },
        ),
        Func::new(
            "media_downloads",
            "Returns a list of series or movies that are downloading and their status, if user asks how long until a series is on etc",
            Vec::new(),
            |_args: &HashMap<String, String>| -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
                box_future(async move { check_downloads().await })
            },
        ),
    ]
}

/// Get the "format" from the args and return the corresponding Format enum
fn get_format(args: &HashMap<String, String>) -> Format {
    match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    }
}

/// Query with GPT to get relevant responses.
async fn query_with_gpt(media_string: String, query: String) -> anyhow::Result<String> {
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo-16k".to_string(),
        media_string,
        format!("Using very concise language and in a single line response (comma separated if outputting a list)\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    if response.is_empty() {
        return Err(anyhow!("Couldn't find any results for {}", query));
    }

    Ok(response)
}

/// Perform a query against the servers media with ai processing to answer a prompt
async fn query_server(
    media_type: Format,
    query: String,
    details: String,
) -> anyhow::Result<String> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie".to_string(), apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series".to_string(), apis::ArrService::Sonarr),
    };
    let all_media = apis::arr_request(reqwest::Method::GET, service, url, None).await?;

    let details: Vec<String> = details.split(',').map(|s| s.trim().to_string()).collect();

    let output_details = OutputDetails {
        availability: false,
        quality: details.contains(&"quality".to_string()),
        tags: details.contains(&"added_by".to_string()),
        db_id: details.contains(&"db_id".to_string()),
        file_details: details.contains(&"file_details".to_string()),
        genres: details.contains(&"genres".to_string()),
    };

    let media_string = media_to_plain_english(&media_type, all_media, 0, output_details).await?;

    // Search with gpt through series and movies to get ones of relevance
    query_with_gpt(media_string, query).await
}

/// Perform a lookup of movies with ai processing to answer a prompt
async fn lookup(media_type: Format, searches: String, query: String) -> anyhow::Result<String> {
    let terms: Vec<String> = searches.split('|').map(|s| s.trim().to_string()).collect();

    // Get list of searches per term
    let mut arr_searches = vec![];
    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    for term in &terms {
        let cleaned_term = term.replace(' ', "%20");
        arr_searches.push(apis::arr_request(
            reqwest::Method::GET,
            service.clone(),
            format!("/api/v3/{media_type}/lookup?term={cleaned_term}"),
            None,
        ));
    }

    // Wait for all searches to finish parallel
    let arr_searches: Vec<Result<Value, anyhow::Error>> =
        futures::future::join_all(arr_searches).await;
    // Gather human readable data for each item
    let mut media_strings: Vec<String> = Vec::new();
    for search in arr_searches.iter().flatten() {
        media_strings.push(
            media_to_plain_english(
                &media_type,
                search.clone(),
                10,
                OutputDetails {
                    availability: true,
                    quality: true,
                    tags: false,
                    db_id: true,
                    file_details: true,
                    genres: false,
                },
            )
            .await?,
        );
    }

    // Search with gpt through series and movies to get ones of relevance
    query_with_gpt(media_strings.join("\n"), query).await
}

/// Add media to the server
async fn add(
    media_type: Format,
    db_id: String,
    user_name: &str,
    quality: String,
) -> anyhow::Result<String> {
    // Determine the lookup path and service based on the media type
    let (lookup_path, service) = match media_type {
        Format::Movie => (
            format!("/api/v3/movie/lookup/tmdb?tmdbId={db_id}"),
            apis::ArrService::Radarr,
        ),
        Format::Series => (
            format!("/api/v3/series/lookup?term=tvdbId {db_id}"),
            apis::ArrService::Sonarr,
        ),
    };

    // Perform the API request
    let mut media = apis::arr_request(reqwest::Method::GET, service.clone(), lookup_path, None)
        .await
        .map_err(|e| anyhow!("Failed media lookup {media_type}, {e}"))?;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };
    // If media type is a movie, perform another lookup with the movies title to gather extra data
    if matches!(media_type, Format::Movie) {
        let title = media["title"].as_str().unwrap().replace(' ', "%20");
        media = apis::arr_request(
            reqwest::Method::GET,
            service.clone(),
            format!("/api/v3/movie/lookup?term={title}"),
            None,
        )
        .await
        .map_err(|e| anyhow!("Failed movie lookup {media_type}, {e}"))?;
        media = media[0].clone();
    };

    let quality_profile_id = get_quality_profile_id(&quality);
    let tag_id = get_user_tag_id(media_type.clone(), user_name).await?;

    // Check if media is already on the server
    if let Some(id) = &media["id"].as_u64() {
        if *id != 0 {
            // If already has the tag that user wants it, return message
            if media["tags"].as_array().unwrap().contains(&tag_id.into()) {
                return Err(anyhow!(
                    "Couldn't add {media_type} with id {id}, it is already on the server and user has requested it"
                ));
            }
            // Else add the tag and let the user know it was added
            let mut new_media = media.clone();
            new_media["tags"]
                .as_array_mut()
                .unwrap()
                .push(tag_id.into());
            push(media_type.clone(), new_media).await?;
            return Err(anyhow!(
                "Couldn't add {media_type} with id {id}, it is already on the server, noted that the user wants it"
            ));
        }
    };

    // Update media json with quality profile id
    media["qualityProfileId"] = quality_profile_id.into();
    media["monitored"] = true.into();
    media["minimumAvailability"] = "announced".into();
    media["tags"] = serde_json::json!([tag_id]);
    match media_type {
        Format::Movie => {
            media["addOptions"] = serde_json::json!({ "searchForMovie": true });
            media["rootFolderPath"] = "/movies".into();
        }
        Format::Series => {
            media["addOptions"] = serde_json::json!({ "searchForMissingEpisodes": true });
            media["rootFolderPath"] = "/tv".into();
            media["languageProfileId"] = 1.into();
        }
    }

    // Media to json
    let media = serde_json::to_string(&media).unwrap();

    // Add the media to sonarr or radarr
    let endpoint = match media_type {
        Format::Movie => "/api/v3/movie",
        Format::Series => "/api/v3/series",
    }
    .to_string();
    apis::arr_request(reqwest::Method::POST, service, endpoint, Some(media))
        .await
        .map_err(|e| anyhow!("Failed to add media: {e}"))?;

    Ok(format!(
        "Added {media_type} with tmdbId/tvdbId {db_id} in {quality}"
    ))
}

/// Updates the resolution of a media item.
async fn setres(media_type: Format, id: String, quality: String) -> anyhow::Result<String> {
    // Determine the lookup path and service based on the media type
    let (lookup_path, service) = match media_type {
        Format::Movie => (format!("/api/v3/movie/{id}"), apis::ArrService::Radarr),
        Format::Series => (format!("/api/v3/series/{id}"), apis::ArrService::Sonarr),
    };

    // Perform the API request
    let mut media = apis::arr_request(reqwest::Method::GET, service.clone(), lookup_path, None)
        .await
        .map_err(|e| anyhow!("Failed media lookup {media_type}, {e}"))?;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

    // Check if the media item exists on the server
    match media["id"].as_u64() {
        Some(0) | None => {
            return Err(anyhow!(
                "Couldn't change resolution, {media_type} with id {id} isn't on the server"
            ))
        }
        _ => {}
    }

    // Update the media item's quality profile id
    media["qualityProfileId"] = get_quality_profile_id(&quality).into();
    push(media_type.clone(), media).await?;

    // Return a success message
    Ok(format!(
        "Changed resolution of {media_type} with id {id} to {quality}"
    ))
}

/// Push data for media
async fn push(media_type: Format, media_json: Value) -> anyhow::Result<()> {
    let media_id = media_json["id"]
        .as_u64()
        .ok_or_else(|| anyhow!("Invalid media ID"))?;

    // Media to json
    let media = serde_json::to_string(&media_json)?;

    // Push the media to sonarr or radarr
    let (path, service) = match media_type {
        Format::Movie => (
            format!("/api/v3/movie/{media_id}"),
            apis::ArrService::Radarr,
        ),
        Format::Series => (
            format!("/api/v3/series/{media_id}"),
            apis::ArrService::Sonarr,
        ),
    };

    apis::arr_request(reqwest::Method::PUT, service, path, Some(media))
        .await
        .map_err(|e| anyhow!("Failed to push media: {e}"))
        .map(|_| ())
}

/// Remove wanted tag from media for user
async fn remove(media_type: Format, id: String, user_name: &str) -> anyhow::Result<String> {
    // Determine the lookup path and service based on the media type
    let (lookup_path, service) = match media_type {
        Format::Movie => (format!("/api/v3/movie/{id}"), apis::ArrService::Radarr),
        Format::Series => (format!("/api/v3/series/{id}"), apis::ArrService::Sonarr),
    };

    // Perform the API request
    let mut media = apis::arr_request(reqwest::Method::GET, service.clone(), lookup_path, None)
        .await
        .map_err(|e| anyhow!("Couldn't remove {media_type}, {e}"))?;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

    let tag_id = get_user_tag_id(media_type.clone(), user_name).await?;

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
            // If already has the tag that user wants it, remove the tag for the user and return
            if media["tags"].as_array().unwrap().contains(&tag_id.into()) {
                let mut new_media = media.clone();
                let new_tags = new_media["tags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|&x| x.as_u64() != tag_id)
                    .cloned()
                    .collect::<Vec<Value>>();
                new_media["tags"] = new_tags.into();
                push(media_type.clone(), new_media).await?;
                return Ok(format!(
                    "{media_type} with id {id} has been unrequested for user"
                ));
            }
            return Err(anyhow!(
                "Couldn't remove {media_type} with id {id}, user hasn't requested it"
            ));
        }
    };
    Err(anyhow!(
        "Couldn't remove {media_type} with id {id}, it isn't on the server"
    ))
}

/// Check for media the user wants
async fn wanted(media_type: Format, query: String, user_name: &str) -> anyhow::Result<String> {
    if query.to_lowercase() == "none" {
        let none_media = get_media_with_no_user_tags(media_type.clone())
            .await?
            .join(";");

        let result_msg = if none_media.is_empty() {
            format!("{media_type} with no users requests: none")
        } else {
            format!("{media_type} with no users requests: {none_media}")
        };

        return Ok(result_msg);
    }

    let user = if query == "self" {
        user_name.to_string()
    } else {
        query.to_lowercase()
    };

    let user_media = get_media_with_user_tag(media_type.clone(), &user)
        .await?
        .join(";");

    let result_msg = if user_media.is_empty() {
        format!("{media_type} requested by {user}: none")
    } else {
        format!("{media_type} requested by {user}: {user_media}")
    };

    Ok(result_msg)
}

/// Get status of media that is currently downloading or awaiting import
async fn check_downloads() -> anyhow::Result<String> {
    let mut output_parts = Vec::new();

    for (service, service_name) in &[
        (apis::ArrService::Radarr, "Movies"),
        (apis::ArrService::Sonarr, "Series"),
    ] {
        let download_value = apis::arr_request(
            reqwest::Method::GET,
            service.clone(),
            "/api/v3/queue".to_string(),
            None,
        )
        .await?;

        // Extract downloads
        let downloads = download_value["records"]
            .as_array()
            .ok_or_else(|| anyhow!("Expected an array of downloads"))?;

        let mut downloads_info = Vec::new();
        let mut seen_titles = std::collections::HashSet::new();

        for download in downloads {
            let title = download["title"].as_str().unwrap_or("Unknown Title");

            // Skip already processed titles
            if seen_titles.contains(title) {
                continue;
            }
            seen_titles.insert(title);

            let status = download["status"].as_str().unwrap_or("Unknown Status");
            let time_left = download["timeleft"].as_str().unwrap_or("Unknown Time Left");

            let messages: Vec<String> = download["statusMessages"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|msg| msg["title"].as_str())
                .map(String::from)
                .collect();

            let formatted_message = if messages.is_empty() {
                String::new()
            } else {
                format!(", Messages: {}", messages.join(", "))
            };

            downloads_info.push(format!(
                "{title} (Status: {status} Time Left: {time_left}{formatted_message})"
            ));
        }

        if !downloads_info.is_empty() {
            output_parts.push(format!(
                "{} Downloads: {}",
                service_name,
                downloads_info.join("; ")
            ));
        }
    }

    // Return a human-readable summary
    if output_parts.is_empty() {
        return Ok("No downloads in progress".to_string());
    }
    Ok(output_parts.join(" | "))
}

/// Get user tag id
async fn get_user_tag_id(media_type: Format, user_name: &str) -> anyhow::Result<Option<u64>> {
    // Sync then get all current tags
    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    sync_user_tags(media_type.clone()).await?;

    // Fetch all tags from the service
    let all_tags = apis::arr_request(
        reqwest::Method::GET,
        service.clone(),
        "/api/v3/tag".to_string(),
        None,
    )
    .await?;

    // Get id for users tag [{id: 1, label: "added-username"}]
    if let Value::Array(tags) = &all_tags {
        for tag in tags {
            // print label and added-user_name
            if tag["label"].as_str() == Some(&format!("added-{user_name}")) {
                return Ok(tag["id"].as_u64());
            }
        }
    }
    Err(anyhow!("No tag id found for user: '{}'", user_name))
}

/// Sync tags on sonarr or radarr for added-username
async fn sync_user_tags(media_type: Format) -> anyhow::Result<()> {
    // Read and parse the TOML file
    let parsed_toml: toml::Value = std::fs::read_to_string("names.toml")
        .map_err(|e| anyhow!("Failed to read names.toml {e}"))?
        .parse()?;

    // Extract user names
    let user_names: Vec<String> = parsed_toml
        .as_table()
        .unwrap()
        .values()
        .filter_map(|user| user.get("name"))
        .filter_map(toml::Value::as_str)
        .map(str::to_lowercase)
        .collect();

    // Determine the service based on media type
    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };

    // Fetch all tags from the service
    let all_tags = apis::arr_request(
        reqwest::Method::GET,
        service.clone(),
        "/api/v3/tag".to_string(),
        None,
    )
    .await?;

    // Get tags with prefix
    let mut current_tags = Vec::new();
    for tag in all_tags.as_array().unwrap() {
        let tag_str = tag["label"].as_str().unwrap();
        if tag_str.starts_with("added-") {
            current_tags.push(tag_str.to_string());
        }
    }

    // Create missing tags
    for user_name in &user_names {
        let tag = format!("added-{user_name}");
        if !current_tags.contains(&tag) {
            let body = serde_json::json!({ "label": tag }).to_string();
            apis::arr_request(
                reqwest::Method::POST,
                service.clone(),
                "/api/v3/tag".to_string(),
                Some(body),
            )
            .await?;
        }
    }

    // Remove extra tags
    for tag in &current_tags {
        let tag_without_prefix = tag.strip_prefix("added-").unwrap().to_string();
        if !user_names.contains(&tag_without_prefix) {
            let tag_id = all_tags
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["label"].as_str().unwrap() == *tag)
                .unwrap()["id"]
                .as_i64()
                .unwrap();
            apis::arr_request(
                reqwest::Method::DELETE,
                service.clone(),
                format!("/api/v3/tag/{tag_id}"),
                None,
            )
            .await?;
        }
    }

    Ok(())
}

/// Get media that has no user tags
async fn get_media_with_no_user_tags(media_type: Format) -> anyhow::Result<Vec<String>> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie", apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series", apis::ArrService::Sonarr),
    };

    // Get all media
    let all_media = apis::arr_request(reqwest::Method::GET, service, url.into(), None)
        .await
        .map_err(|e| anyhow!("Failed to get all media {e}"))?;

    let mut media_with_no_user_tags = Vec::new();
    for media in all_media.as_array().unwrap() {
        let tags = media["tags"].as_array().unwrap();
        if tags.is_empty() {
            media_with_no_user_tags.push(media["title"].as_str().unwrap().to_string());
        }
    }
    Ok(media_with_no_user_tags)
}

/// Get media tagged for user
async fn get_media_with_user_tag(
    media_type: Format,
    user_name: &str,
) -> anyhow::Result<Vec<String>> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie", apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series", apis::ArrService::Sonarr),
    };

    // Get user tag id
    println!("Getting user tag id {} {}", media_type.clone(), user_name);
    let tag_id = get_user_tag_id(media_type.clone(), user_name)
        .await?
        .ok_or_else(|| anyhow!("No tag id found for user: '{}'", user_name))?;

    // Get all media
    let all_media = apis::arr_request(reqwest::Method::GET, service, url.into(), None)
        .await
        .map_err(|e| anyhow!("Failed to get all media {e}"))?;

    let mut media_with_user_tag = Vec::new();

    for media in all_media.as_array().unwrap() {
        let tags = media["tags"].as_array().unwrap();
        for tag in tags {
            if tag.as_u64() == Some(tag_id) {
                media_with_user_tag.push(media["title"].as_str().unwrap().to_string());
            }
        }
    }
    Ok(media_with_user_tag)
}

/// Convert number size to string with units
fn sizeof_fmt(mut num: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut index = 0;
    while num >= 1024.0 && index < units.len() - 1 {
        num /= 1024.0;
        index += 1;
    }
    format!("{:.2} {}", num, units[index])
}

/// Get quality profile from string
fn get_quality_profile_id(quality: &str) -> u8 {
    // Map quality strings to quality profile ids
    let quality_profiles: HashMap<&str, u8> = [
        ("SD", 2),
        ("720p", 3),
        ("1080p", 4),
        ("2160p", 5),
        ("720p/1080p", 6),
        ("Any", 7),
    ]
    .iter()
    .copied()
    .collect();
    // Default to 4 (1080p) if the quality string is not found
    *quality_profiles.get(quality).unwrap_or(&4)
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
struct OutputDetails {
    availability: bool,
    quality: bool,
    tags: bool,
    db_id: bool,
    file_details: bool,
    genres: bool,
}

/// Convert json of movies/series data to plain english
#[allow(clippy::too_many_lines)]
async fn media_to_plain_english(
    media_type: &Format,
    response: Value,
    num: usize,
    output_details: OutputDetails,
) -> anyhow::Result<String> {
    let quality_profiles = [
        (2, "SD"),
        (3, "720p"),
        (4, "1080p"),
        (5, "2160p"),
        (6, "720p/1080p"),
        (7, "any quality"),
    ]
    .iter()
    .copied()
    .collect::<HashMap<u64, &str>>();

    // Get all current tags
    let all_tags = apis::arr_request(
        reqwest::Method::GET,
        match media_type {
            Format::Movie => apis::ArrService::Radarr,
            Format::Series => apis::ArrService::Sonarr,
        },
        "/api/v3/tag".to_string(),
        None,
    )
    .await?;

    let media_items = response
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected array from response"))?;

    let num = if num == 0 { media_items.len() } else { num };

    let mut results = Vec::new();
    for item in media_items.iter().take(num) {
        let mut result = Vec::new();

        // Get title and year
        if let (Some(title), Some(year)) = (item["title"].as_str(), item["year"].as_u64()) {
            result.push(format!("{title} ({year})"));
        }

        // Get id and availability
        if output_details.availability {
            result.push(if item["id"].as_u64().unwrap_or(0) == 0 {
                "unavailable on the server".to_string()
            } else {
                format!(
                    "available on the server;id on server {}",
                    item["id"].as_u64().unwrap()
                )
            });
        }

        // Get quality
        if output_details.quality {
            if let Some(quality) = item["qualityProfileId"]
                .as_u64()
                .and_then(|id| quality_profiles.get(&id))
            {
                result.push(format!("requested {quality}"));
            }
        }

        // Get tags added-users
        if output_details.tags {
            let tag_labels: Vec<String> = item["tags"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|tag| {
                    all_tags
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|all_tag| all_tag["id"] == *tag)
                        .and_then(|t| t["label"].as_str())
                })
                .map(|s| s.replace("added-", ""))
                .collect();
            if !tag_labels.is_empty() {
                result.push(format!("added by {}", tag_labels.join(",")));
            }
        }

        // File details
        let db_string = match media_type {
            Format::Movie => "tmdbId",
            Format::Series => "tvdbId",
        };
        // Get db_id
        if output_details.db_id {
            if let Some(db_id) = item[db_string].as_u64() {
                result.push(format!("{db_string} {db_id}"));
            }
        }
        // Get movie file info
        if matches!(media_type, Format::Movie) && output_details.file_details {
            if item["hasFile"].as_bool().unwrap_or(false) {
                result.push(format!(
                    "file size {}",
                    sizeof_fmt(item["sizeOnDisk"].as_f64().unwrap())
                ));
                if let Some(resolution) = item["movieFile"]["mediaInfo"]["resolution"].as_str() {
                    result.push(format!("file resolution {resolution}"));
                }
                if let Some(edition) = item["movieFile"]["edition"].as_str() {
                    if !edition.is_empty() {
                        result.push(format!("file edition {edition}"));
                    }
                }
            } else {
                result.push("no file on disk".to_string());
            }
        }

        // Get genres
        if output_details.genres {
            if let Some(genres) = item["genres"].as_array() {
                let genres: Vec<_> = genres
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect();
                if !genres.is_empty() {
                    result.push(format!("genres {}", genres.join(",")));
                }
            }
        }

        // Push result to results
        results.push(result.join(";"));
    }

    Ok(results.join("|"))
}
