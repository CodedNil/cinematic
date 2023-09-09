use crate::apis;
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

pub fn query_server_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let query = args.get("query").unwrap().to_string();
    let details = args.get("details").unwrap().to_string();
    let fut = async move { query_server(format, query, details).await };
    drop(args);
    Box::pin(fut)
}

pub fn lookup_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let searches = args.get("searches").unwrap().to_string();
    let query = args.get("query").unwrap().to_string();
    let fut = async move { lookup(format, searches, query).await };
    drop(args);
    Box::pin(fut)
}

pub fn add_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let db_id = args.get("db_id").unwrap().to_string();
    let quality = args.get("quality").unwrap().to_string();
    let fut = async move { add(format, db_id, args.get("user_name").unwrap(), quality).await };
    Box::pin(fut)
}

pub fn setres_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let id = args.get("id").unwrap().to_string();
    let quality = args.get("quality").unwrap().to_string();
    let fut = async move { setres(format, id, quality).await };
    drop(args);
    Box::pin(fut)
}

pub fn remove_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let id = args.get("id").unwrap().to_string();
    let fut = async move { remove(format, id, args.get("user_name").unwrap()).await };
    Box::pin(fut)
}

pub fn wanted_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let format = match args.get("format").unwrap().as_str() {
        "series" => Format::Series,
        _ => Format::Movie,
    };
    let user = args.get("user").unwrap().to_string();
    let fut = async move { wanted(format, user, args.get("user_name").unwrap()).await };
    Box::pin(fut)
}

pub fn downloads_args(
    args: HashMap<String, String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
    let fut = async move { check_downloads().await };
    drop(args);
    Box::pin(fut)
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
        availability: details.contains(&"availability".to_string()),
        quality: details.contains(&"quality".to_string()),
        tags: details.contains(&"tags".to_string()),
        db_id: details.contains(&"db_id".to_string()),
        file_details: details.contains(&"file_details".to_string()),
        genres: details.contains(&"genres".to_string()),
    };

    let media_string = media_to_plain_english(&media_type, all_media, 0, output_details).await?;

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo-16k".to_string(),
        media_string,
        format!("Using very concise language\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    Ok(response)
}

/// Perform a lookup of movies with ai processing to answer a prompt
async fn lookup(media_type: Format, searches: String, query: String) -> anyhow::Result<String> {
    let terms: Vec<String> = searches.split('|').map(|s| s.trim().to_string()).collect();

    // Get list of searches per term
    let mut arr_searches = vec![];
    let arr_service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    for term in &terms {
        let cleaned_term = term.replace(' ', "%20");
        arr_searches.push(apis::arr_request(
            reqwest::Method::GET,
            arr_service.clone(),
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
        let search_results = search.clone();
        // Trim the searches so each only contains 5 results max and output plain english
        media_strings.push(
            media_to_plain_english(
                &media_type,
                search_results,
                10,
                OutputDetails {
                    availability: true,
                    quality: true,
                    tags: true,
                    db_id: true,
                    file_details: true,
                    genres: false,
                },
            )
            .await?,
        );
    }

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo".to_string(),
        media_strings.join("\n"),
        format!("Using very concise language\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    Ok(response)
}

/// Add media to the server
#[allow(clippy::too_many_lines)]
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
        .map_err(|e| anyhow!("Failed media lookup {}, {}", media_type, e))?;
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
        .map_err(|e| anyhow!("Failed movie lookup {}, {}", media_type, e))?;
        media = media[0].clone();
    };

    let quality_profile_id = get_quality_profile_id(&quality);

    let tag_id = get_user_tag_id(media_type.clone(), user_name).await;

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
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
            push(media_type.clone(), new_media).await;
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
    let response = match media_type {
        Format::Movie => {
            apis::arr_request(
                reqwest::Method::POST,
                apis::ArrService::Radarr,
                "/api/v3/movie".to_string(),
                Some(media),
            )
            .await
        }
        Format::Series => {
            apis::arr_request(
                reqwest::Method::POST,
                apis::ArrService::Sonarr,
                "/api/v3/series".to_string(),
                Some(media),
            )
            .await
        }
    };
    // Return error if response is an error
    if response.is_err() {
        return Err(anyhow!(
            "Couldn't add {media_type}, {error}",
            media_type = media_type,
            error = response.err().unwrap()
        ));
    }

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
        .map_err(|e| anyhow!("Failed media lookup {}, {}", media_type, e))?;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

    let quality_profile_id = get_quality_profile_id(&quality);

    // Check if the media item exists on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() == 0 {
            return Err(anyhow!(
                "Couldn't change resolution, {media_type} with id {id} isnt on the server"
            ));
        }
    } else {
        return Err(anyhow!(
            "Couldn't change resolution, {media_type} with id {id} isnt on the server"
        ));
    }

    // Update the media item's quality profile id
    media["qualityProfileId"] = quality_profile_id.into();
    push(media_type.clone(), media.clone()).await;

    // Return a success message
    Ok(format!(
        "Changed resolution of {media_type} with id {id} to {quality}"
    ))
}

/// Push data for media
async fn push(media_type: Format, media_json: Value) {
    let media_id = media_json["id"].as_u64().unwrap();
    // Media to json
    let media = serde_json::to_string(&media_json).unwrap();

    // Push the media to sonarr or radarr
    match media_type {
        Format::Movie => {
            apis::arr_request(
                reqwest::Method::PUT,
                apis::ArrService::Radarr,
                format!("/api/v3/movie/{media_id}"),
                Some(media),
            )
            .await
        }
        Format::Series => {
            apis::arr_request(
                reqwest::Method::PUT,
                apis::ArrService::Sonarr,
                format!("/api/v3/series/{media_id}"),
                Some(media),
            )
            .await
        }
    }
    .expect("Failed to push media");
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
        .map_err(|e| anyhow!("Couldn't remove {}, {}", media_type, e))?;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

    let tag_id: Option<u64> = get_user_tag_id(media_type.clone(), user_name).await;

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
                push(media_type.clone(), new_media).await;
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
            .await
            .join(", ");
        if none_media.is_empty() {
            return Ok(format!("{media_type} with no users requests: none"));
        }
        return Ok(format!("{media_type} with no users requests: {none_media}"));
    }
    let user: String = if query == "self" {
        user_name.to_string()
    } else {
        query.to_lowercase()
    };
    let user_media = get_media_with_user_tag(media_type.clone(), &user)
        .await
        .join(", ");
    if user_media.is_empty() {
        return Ok(format!("{media_type} requested by {user}: none"));
    }
    Ok(format!("{media_type} requested by {user}: {user_media}"))
}

/// Get status of media that is currently downloading or awaiting import
async fn check_downloads() -> anyhow::Result<String> {
    // Fetch the current downloads queue from Radarr
    let radarr_downloads_value = apis::arr_request(
        reqwest::Method::GET,
        apis::ArrService::Radarr,
        "/api/v3/queue".to_string(),
        None,
    )
    .await?;

    // Fetch the current downloads queue from Sonarr
    let sonarr_downloads_value = apis::arr_request(
        reqwest::Method::GET,
        apis::ArrService::Sonarr,
        "/api/v3/queue".to_string(),
        None,
    )
    .await?;

    let extract_downloads = |value: &serde_json::Value| -> anyhow::Result<Vec<String>> {
        // Ensure that the returned JSON is an array
        let downloads = value["records"]
            .as_array()
            .ok_or_else(|| anyhow!("Expected an array of downloads"))?;

        // Extract relevant information about the downloads
        let mut downloads_info = Vec::new();
        let mut seen_titles = std::collections::HashSet::new();
        for download in downloads {
            let title = download["title"].as_str().unwrap_or("Unknown Title");
            if seen_titles.contains(title) {
                continue; // Skip this title since it's already processed
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
        Ok(downloads_info)
    };

    let radarr_downloads = extract_downloads(&radarr_downloads_value)?;
    let sonarr_downloads = extract_downloads(&sonarr_downloads_value)?;

    // Format the output based on the presence of downloads
    let mut output_parts = Vec::new();
    if !sonarr_downloads.is_empty() {
        output_parts.push(format!("Series Downloads: {}", sonarr_downloads.join("; ")));
    }
    if !radarr_downloads.is_empty() {
        output_parts.push(format!("Movies downloads: {}", radarr_downloads.join("; ")));
    }

    // Return a human-readable summary
    Ok(output_parts.join(" | "))
}

/// Get user tag id
async fn get_user_tag_id(media_type: Format, user_name: &str) -> Option<u64> {
    // Sync then get all current tags
    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    sync_user_tags(media_type.clone()).await;
    let all_tags = apis::arr_request(
        reqwest::Method::GET,
        service,
        "/api/v3/tag".to_string(),
        None,
    )
    .await;
    // Return error if all_tags is an error
    if all_tags.is_err() {
        return None;
    }
    let all_tags = all_tags.unwrap();
    // Get id for users tag [{id: 1, label: "added-username"}]
    if let Value::Array(tags) = &all_tags {
        for tag in tags {
            // print label and added-user_name
            if tag["label"].as_str() == Some(&format!("added-{user_name}")) {
                return tag["id"].as_u64();
            }
        }
    }
    None
}

/// Sync tags on sonarr or radarr for added-username
async fn sync_user_tags(media_type: Format) {
    let contents = std::fs::read_to_string("names.toml");
    if contents.is_err() {
        return;
    }
    let parsed_toml: toml::Value = contents.unwrap().parse().unwrap();
    let mut user_names = vec![];
    // Get all users, then the name from each
    for (_id, user) in parsed_toml.as_table().unwrap() {
        if !user.as_table().unwrap().contains_key("name") {
            continue;
        }
        user_names.push(user.get("name").unwrap().as_str().unwrap().to_lowercase());
    }

    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };

    // Get all current tags
    let all_tags = apis::arr_request(
        reqwest::Method::GET,
        service.clone(),
        "/api/v3/tag".to_string(),
        None,
    )
    .await
    .expect("Failed to get tags");
    // Get tags with prefix
    let mut current_tags = Vec::new();
    for tag in all_tags.as_array().unwrap() {
        let tag_str = tag["label"].as_str().unwrap();
        if tag_str.starts_with("added-") {
            current_tags.push(tag_str.to_string());
        }
    }

    // Add missing tags
    let mut tags_to_add = Vec::new();
    for user_name in &user_names {
        let tag = format!("added-{user_name}");
        if !current_tags.contains(&tag) {
            tags_to_add.push(tag);
        }
    }
    for tag in tags_to_add {
        let body = serde_json::json!({ "label": tag }).to_string();
        apis::arr_request(
            reqwest::Method::POST,
            service.clone(),
            "/api/v3/tag".to_string(),
            Some(body),
        )
        .await
        .expect("Failed to add tag");
    }

    // Remove extra tags
    let mut tags_to_remove = Vec::new();
    for tag in &current_tags {
        let tag_without_prefix = tag.strip_prefix("added-").unwrap();
        if !user_names.contains(&tag_without_prefix.to_string()) {
            tags_to_remove.push(tag.clone());
        }
    }
    for tag in tags_to_remove {
        let tag_id = all_tags
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["label"].as_str().unwrap() == tag)
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        apis::arr_request(
            reqwest::Method::DELETE,
            service.clone(),
            format!("/api/v3/tag/{tag_id}"),
            None,
        )
        .await
        .expect("Failed to remove tag");
    }
}

/// Get media that has no user tags
async fn get_media_with_no_user_tags(media_type: Format) -> Vec<String> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie".to_string(), apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series".to_string(), apis::ArrService::Sonarr),
    };
    let all_media = apis::arr_request(reqwest::Method::GET, service, url, None).await;
    // Return error if media is an error
    if all_media.is_err() {
        return Vec::new();
    }
    let all_media = all_media.unwrap();
    let mut media_with_no_user_tags = Vec::new();
    for media in all_media.as_array().unwrap() {
        let tags = media["tags"].as_array().unwrap();
        if tags.is_empty() {
            media_with_no_user_tags.push(media["title"].as_str().unwrap().to_string());
        }
    }
    media_with_no_user_tags
}

/// Get media tagged for user
async fn get_media_with_user_tag(media_type: Format, user_name: &str) -> Vec<String> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie".to_string(), apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series".to_string(), apis::ArrService::Sonarr),
    };
    // Get user tag id
    println!("Getting user tag id {} {}", media_type.clone(), user_name);
    let tag_id = get_user_tag_id(media_type.clone(), user_name).await;
    // Get all media and return ones the user requested
    println!("Getting all media");
    let all_media = apis::arr_request(reqwest::Method::GET, service, url, None).await;
    // Return error if media is an error
    if all_media.is_err() {
        println!(
            "Error getting media with user tag: {}",
            all_media.err().unwrap()
        );
        return Vec::new();
    }
    let all_media = all_media.unwrap();
    let mut media_with_user_tag = Vec::new();
    for media in all_media.as_array().unwrap() {
        let tags = media["tags"].as_array().unwrap();
        for tag in tags {
            if tag.as_u64() == tag_id {
                media_with_user_tag.push(media["title"].as_str().unwrap().to_string());
            }
        }
    }
    media_with_user_tag
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
        ("any quality", 7),
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
        (7, "Any"),
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

    let mut results = Vec::new();
    for item in media_items {
        let mut result = Vec::new();
        // Get title and year
        if let (Value::String(title), Value::Number(year)) = (&item["title"], &item["year"]) {
            result.push(format!("{title} ({year})"));
        }

        // Get id and availability
        if output_details.availability {
            if let Value::Number(id) = &item["id"] {
                if id.as_u64().unwrap() == 0 {
                    result.push("unavailable on the server".to_string());
                } else {
                    result.push("available on the server".to_string());
                    result.push(format!("id on server {id}"));
                }
            } else {
                result.push("unavailable on the server".to_string());
            }
        }

        // Get quality
        if output_details.quality {
            if let Value::Number(quality_profile_id) = &item["qualityProfileId"] {
                if let Some(quality_profile_id_u64) = quality_profile_id.as_u64() {
                    if let Some(quality) = quality_profiles.get(&quality_profile_id_u64) {
                        result.push(format!("requested {quality}"));
                    }
                }
            }
        }

        // Get tags added-users
        if output_details.tags {
            if let Value::Array(tags) = &item["tags"] {
                // Get tags added-users
                let mut tag_labels = String::new();
                for tag in tags {
                    if let Value::Number(tag_id) = tag {
                        for all_tag in all_tags.as_array().unwrap() {
                            if let (Some(id), Some(label)) =
                                (all_tag.get("id"), all_tag.get("label"))
                            {
                                if id.as_u64() == tag_id.as_u64() {
                                    if !tag_labels.is_empty() {
                                        tag_labels.push_str(", ");
                                    }
                                    tag_labels.push_str(
                                        label.as_str().unwrap().replace("added-", "").as_str(),
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
                if !tag_labels.is_empty() {
                    result.push(format!("added by {tag_labels}"));
                }
            }
        }

        // File details
        match media_type {
            Format::Movie => {
                // Get tmdbId
                if output_details.db_id {
                    if let Value::Number(tmdb_id) = &item["tmdbId"] {
                        result.push(format!("tmdbId {tmdb_id}"));
                    }
                }
                // Get movie file info
                if output_details.file_details {
                    if item["hasFile"].as_bool().unwrap_or(false) {
                        if let Value::Number(size_on_disk) = &item["sizeOnDisk"] {
                            result.push(format!(
                                "file size {}",
                                sizeof_fmt(size_on_disk.as_f64().unwrap())
                            ));
                        }
                        let movie_file = &item["movieFile"];
                        if let Value::String(resolution) = &movie_file["mediaInfo"]["resolution"] {
                            result.push(format!("file resolution {resolution}"));
                        }
                        if let Value::String(edition) = &movie_file["edition"] {
                            if !edition.is_empty() {
                                result.push(format!("file edition {edition}"));
                            }
                        }
                    } else {
                        result.push("no file on disk".to_string());
                    }
                }
            }
            Format::Series => {
                // Get tvdbId
                if output_details.db_id {
                    if let Value::Number(tvdb_id) = &item["tvdbId"] {
                        result.push(format!("tvdbId {tvdb_id}"));
                    }
                }
            }
        }

        // Get genres
        if output_details.genres {
            if let Value::Array(genres) = &item["genres"] {
                let genres_string = genres
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<&str>>()
                    .join(",");
                if !genres_string.is_empty() {
                    result.push(format!("genres {genres_string}"));
                }
            }
        }

        // Push result to results
        results.push(result.join(";"));

        if num != 0 && results.len() >= num {
            break;
        }
    }

    Ok(results.join("\n"))
}
