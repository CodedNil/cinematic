use crate::apis;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

#[derive(Debug, Clone)]
pub enum Format {
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

/// Perform a lookup of movies with ai processing to answer a prompt
pub async fn lookup(media_type: Format, query: String) -> Result<String, Box<dyn Error>> {
    let prompt = match media_type {
        Format::Movie => "The above text is a query to lookup movies, from the text above gather a list of movie titles mentioned, mention each movies title in its core form such as ('Avatar: The Way of Water' to 'Avatar', 'The Lord of the Rings: Return of the King Ultimate Cut' to just 'Lord of the Rings', 'Thor 2' to 'Thor'), return a list with ; divider on a single line".to_string(),
        Format::Series => "The above text is a query to lookup series, from the text above gather a list of series titles mentioned, mention each series title in its core form such as ('Stargate: SG1' to 'Stargate', 'The Witcher' to just 'Witcher'), return a list with ; divider on a single line".to_string(),
    };
    // Use gpt to get a list of all movies to search for
    let response = apis::gpt_info_query("gpt-4".to_string(), query.clone(), prompt)
        .await
        .unwrap_or_default();
    let terms: Vec<String> = response.split(';').map(|s| s.trim().to_string()).collect();

    // Get list of searches per term
    let mut arr_searches = vec![];
    let arr_service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    for term in &terms {
        arr_searches.push(apis::arr_request(
            apis::HttpMethod::Get,
            arr_service.clone(),
            format!("/api/v3/{media_type}/lookup?term={term}"),
            None,
        ));
    }
    // Wait for all searches to finish parallel
    let arr_searches = futures::future::join_all(arr_searches).await;
    // Gather human readable data for each item
    let mut media_strings: Vec<String> = Vec::new();
    for search in arr_searches {
        let search_results = search.clone();
        // Trim the searches so each only contains 5 results max and output plain english
        media_strings.push(media_to_plain_english(&media_type, search_results, 5).await);
    }

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-4".to_string(),
        media_strings.join("\n"),
        format!("Using very concise language\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    Ok(response)
}

/// Add media to the server
pub async fn add(
    media_type: Format,
    db_id: String,
    user_name: &str,
    quality: String,
) -> Result<String, Box<dyn Error>> {
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
    let mut media =
        apis::arr_request(apis::HttpMethod::Get, service.clone(), lookup_path, None).await;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

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
    let quality_profile_id: u8 = *quality_profiles.get(quality.as_str()).unwrap_or(&4);

    let tag_id = get_user_tag_id(media_type.clone(), user_name).await;

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
            // If already has the tag that user wants it, return message
            if media["tags"].as_array().unwrap().contains(&tag_id.into()) {
                return Err(format!(
                    "Couldn't add {media_type} with id {id}, it is already on the server and user has requested it"
                )
                .into());
            }
            // Else add the tag and let the user know it was added
            let mut new_media = media.clone();
            new_media["tags"]
                .as_array_mut()
                .unwrap()
                .push(tag_id.into());
            push(media_type.clone(), new_media).await;
            return Err(format!(
                "Couldn't add {media_type} with id {id}, it is already on the server, noted that the user wants it"
            ).into());
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
    match media_type {
        Format::Movie => {
            apis::arr_request(
                apis::HttpMethod::Post,
                apis::ArrService::Radarr,
                "/api/v3/movie".to_string(),
                Some(media),
            )
            .await
        }
        Format::Series => {
            apis::arr_request(
                apis::HttpMethod::Post,
                apis::ArrService::Sonarr,
                "/api/v3/series".to_string(),
                Some(media),
            )
            .await
        }
    };

    Ok(format!("Added {media_type} with tmdbId/tvdbId {db_id} in {quality}"))
}

/// Updates the resolution of a media item.
pub async fn setres(
    media_type: Format,
    id: String,
    quality: String,
) -> Result<String, Box<dyn Error>> {
    // Determine the lookup path and service based on the media type
    let (lookup_path, service) = match media_type {
        Format::Movie => (format!("/api/v3/movie/{id}"), apis::ArrService::Radarr),
        Format::Series => (format!("/api/v3/series/{id}"), apis::ArrService::Sonarr),
    };

    // Perform the API request
    let mut media =
        apis::arr_request(apis::HttpMethod::Get, service.clone(), lookup_path, None).await;
    // If the media type is a series, get the first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

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
    let quality_profile_id: u8 = *quality_profiles.get(quality.as_str()).unwrap_or(&4);

    // Check if the media item exists on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() == 0 {
            return Err(format!(
                "Couldn't change resolution, {media_type} with id {id} isnt on the server"
            )
            .into());
        }
    } else {
        return Err(format!(
            "Couldn't change resolution, {media_type} with id {id} isnt on the server"
        )
        .into());
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
pub async fn push(media_type: Format, media_json: Value) {
    let media_id = media_json["id"].as_u64().unwrap();
    // Media to json
    let media = serde_json::to_string(&media_json).unwrap();

    // Push the media to sonarr or radarr
    match media_type {
        Format::Movie => {
            apis::arr_request(
                apis::HttpMethod::Put,
                apis::ArrService::Radarr,
                format!("/api/v3/movie/{media_id}"),
                Some(media),
            )
            .await
        }
        Format::Series => {
            apis::arr_request(
                apis::HttpMethod::Put,
                apis::ArrService::Sonarr,
                format!("/api/v3/series/{media_id}"),
                Some(media),
            )
            .await
        }
    };
}

/// Remove wanted tag from media for user
pub async fn remove(
    media_type: Format,
    id: String,
    user_name: &str,
) -> Result<String, Box<dyn Error>> {
    // Determine the lookup path and service based on the media type
    let (lookup_path, service) = match media_type {
        Format::Movie => (format!("/api/v3/movie/{id}"), apis::ArrService::Radarr),
        Format::Series => (format!("/api/v3/series/{id}"), apis::ArrService::Sonarr),
    };

    // Perform the API request
    let mut media =
        apis::arr_request(apis::HttpMethod::Get, service.clone(), lookup_path, None).await;
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
            return Err(format!(
                "Couldn't remove {media_type} with id {id}, user hasn't requested it"
            )
            .into());
        }
    };
    Err(format!("Couldn't remove {media_type} with id {id}, it isn't on the server").into())
}

/// Check for media the user wants
pub async fn wanted(
    media_type: Format,
    query: String,
    user_name: &str,
) -> Result<String, Box<dyn Error>> {
    if query.to_lowercase() == "none" {
        let none_media = get_media_with_no_user_tags(media_type.clone())
            .await
            .join(", ");
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
    Ok(format!("{media_type} requested by {user}: {user_media}"))
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
        apis::HttpMethod::Get,
        service,
        "/api/v3/tag".to_string(),
        None,
    )
    .await;
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
    let contents = std::fs::read_to_string("memories.toml");
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
        apis::HttpMethod::Get,
        service.clone(),
        "/api/v3/tag".to_string(),
        None,
    )
    .await;
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
            apis::HttpMethod::Post,
            service.clone(),
            "/api/v3/tag".to_string(),
            Some(body),
        )
        .await;
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
            apis::HttpMethod::Delete,
            service.clone(),
            format!("/api/v3/tag/{tag_id}"),
            None,
        )
        .await;
    }
}

/// Get media that has no user tags
async fn get_media_with_no_user_tags(media_type: Format) -> Vec<String> {
    let (url, service) = match media_type {
        Format::Movie => ("/api/v3/movie".to_string(), apis::ArrService::Radarr),
        Format::Series => ("/api/v3/series".to_string(), apis::ArrService::Sonarr),
    };
    let all_media = apis::arr_request(apis::HttpMethod::Get, service, url, None).await;
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
    let tag_id = get_user_tag_id(media_type.clone(), user_name).await;
    // Get all media and return ones the user requested
    let all_media = apis::arr_request(apis::HttpMethod::Get, service, url, None).await;
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

/// Convert json of movies/series data to plain english
#[allow(clippy::too_many_lines)]
async fn media_to_plain_english(media_type: &Format, response: Value, num: usize) -> String {
    let quality_profiles: HashMap<u64, &str> = [
        (2, "SD"),
        (3, "720p"),
        (4, "1080p"),
        (5, "2160p"),
        (6, "720p/1080p"),
        (7, "Any"),
    ]
    .iter()
    .copied()
    .collect();

    let mut results = Vec::new();
    if let Value::Array(media_items) = response {
        for item in media_items {
            let mut result = Vec::new();
            // Get title and year
            if let (Value::String(title), Value::Number(year)) = (&item["title"], &item["year"]) {
                result.push(format!("{title} ({year})"));
            }
            // Get id and availability
            if let Value::Number(id) = &item["id"] {
                if id.as_u64().unwrap() == 0 {
                    result.push("unavailable on the server".to_string());
                } else {
                    result.push("available on the server".to_string());
                    result.push(format!("{media_type} id {id}"));
                }
            } else {
                result.push("unavailable on the server".to_string());
            }
            // Get quality
            if let Value::Number(quality_profile_id) = &item["qualityProfileId"] {
                if let Some(quality_profile_id_u64) = quality_profile_id.as_u64() {
                    if let Some(quality) = quality_profiles.get(&quality_profile_id_u64) {
                        result.push(format!("requested at quality {quality}"));
                    }
                }
            }
            // Get tags added-users
            if let Value::Array(tags) = &item["tags"] {
                // Get all current tags
                let all_tags: Value = apis::arr_request(
                    apis::HttpMethod::Get,
                    match media_type {
                        Format::Movie => apis::ArrService::Radarr,
                        Format::Series => apis::ArrService::Sonarr,
                    },
                    "/api/v3/tag".to_string(),
                    None,
                )
                .await;
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
                                    tag_labels.push_str(label.as_str().unwrap());
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
            match media_type {
                Format::Movie => {
                    // Get tmdbId
                    if let Value::Number(tmdb_id) = &item["tmdbId"] {
                        result.push(format!("tmdbId {tmdb_id}"));
                    };
                    // Get movie file info
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
                Format::Series => {
                    // Get tvdbId
                    if let Value::Number(tvdb_id) = &item["tvdbId"] {
                        result.push(format!("tvdbId {tvdb_id}"));
                    }
                }
            }
            results.push(result.join(";"));

            if results.len() >= num {
                break;
            }
        }
    }
    results.join("\n")
}
