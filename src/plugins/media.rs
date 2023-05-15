//! Media plugin for lookups, adding, editing movies and series

use crate::{apis, plugins::PluginReturn};

use serde_json::Value;
use std::collections::HashMap;

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

// Plugins data
pub fn get_plugin_data() -> String {
    "[MOVIES_LOOKUP~query] Searches for a movie or movies from a query for example \"is iron man on? is watchmen the ultimate cut?\"
[SERIES_LOOKUP~query] Query should be phrased as a question \"What is Cats movie tmdbId\" \"Who added game of thrones?\" etc, if multiple results are found, ask user for clarification
[MOVIES_ADD~tmdbId;quality] Adds a movie to the server from the name, always needs to first lookup asking for tmdbId, can specify resolution, defaults to adding in 1080p, options are SD, 720p, 1080p, 2160p
[SERIES_ADD~tvdbId;quality]
[MOVIES_REMOVE~tmdbId] Removes a movie or series from that users requests, it stays on the server if anyone else wants it
[SERIES_REMOVE~tvdbId] Uses tvdbId
[MOVIES_SETRES~id;quality] Sets the resolution of a series or movie on the server, always needs to first lookup asking for radarr id
[SERIES_SETRES~id;quality] Uses sonarr id
[SERIES_WANTED~user] Returns a list of series that user has requested, user can be self for the user that spoke, or none to get a list of series that noone has requested, if user asks have they requested or what they have requested etc
[MOVIES_WANTED~user] Same as series wanted but for movies
tmdbId and tvdbId can be found from the lookup commands, ask for it such as [SERIES_LOOKUP~What is game of thrones tvdbId?]
If user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MOVIE_LOOKUP~Are these movies on Iron Man 1,Thor 1,Black Widow,...]
If user queries \"how many gbs do my added movies take up\" look up the users wanted, then lookup on the server to get file size of each".to_string()
}

/// Get processing message
pub fn processing_message_lookup(query: &String) -> String {
    format!("🎬 Looking up media {query}")
}

pub fn processing_message_add(query: &String) -> String {
    format!("🎬 Adding media {query}")
}

pub fn processing_message_setres(query: &String) -> String {
    format!("🎬 Changing quality {query}")
}

pub fn processing_message_remove(query: &String) -> String {
    format!("🎬 Removing media {query} from your requests")
}

pub fn processing_message_wanted(query: &String) -> String {
    format!("🎬 Checking wanted media for {query}")
}

/// Perform a lookup of movies with ai processing to answer a prompt
pub async fn lookup(media_type: Format, query: String) -> PluginReturn {
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

    PluginReturn {
        result: response,
        to_user: format!("🎬 {media_type} lookup successful for {query}"),
    }
}

/// Add media to the server
pub async fn add(
    media_type: Format,
    query: String,
    user_id: &String,
    user_name: &str,
) -> PluginReturn {
    let (mut media, id, quality_profile_id, quality) =
        match get_media_info(media_type.clone(), query.clone(), true).await {
            Ok(media_info) => media_info,
            Err(err) => {
                return PluginReturn {
                    result: String::new(),
                    to_user: err,
                }
            }
        };

    let tag_id = get_user_tag_id(media_type.clone(), user_id, user_name).await;

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
            // If already has the tag that user wants it, return message
            if media["tags"].as_array().unwrap().contains(&tag_id.into()) {
                return PluginReturn {
                    result: format!("{media_type} with id {id} is already on the server"),
                    to_user: format!(
                        "❌ Can't add {media_type} with id {id} is already on the server"
                    ),
                };
            }
            // Else add the tag and let the user know it was added
            let mut new_media = media.clone();
            new_media["tags"]
                .as_array_mut()
                .unwrap()
                .push(tag_id.into());
            push(media_type.clone(), new_media).await;
            return PluginReturn {
                result: format!("{media_type} with id {id} is already on the server, noted that user wants it"),
                to_user: format!(
                    "❌ Can't add {media_type} with id {id} is already on the server, noted that user wants it"
                ),
            };
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

    PluginReturn {
        result: String::new(),
        to_user: format!("🎬 Added {media_type} with id {id} in {quality}"),
    }
}

/// Set the resolution of media
pub async fn setres(media_type: Format, query: String) -> PluginReturn {
    let (mut media, id, quality_profile_id, quality) =
        match get_media_info(media_type.clone(), query.clone(), false).await {
            Ok(media_info) => media_info,
            Err(err) => {
                return PluginReturn {
                    result: String::new(),
                    to_user: err,
                }
            }
        };

    // Check if media is on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() == 0 {
            return PluginReturn {
                result: format!("{media_type} with id {id} isnt on the server"),
                to_user: format!(
                    "❌ Couldn't change resolution, {media_type} with id {id} isnt on the server"
                ),
            };
        }
    } else {
        return PluginReturn {
            result: format!("{media_type} with id {id} isnt on the server"),
            to_user: format!(
                "❌ Couldn't change resolution, {media_type} with id {id} isnt on the server"
            ),
        };
    }

    // Update media json with quality profile id
    media["qualityProfileId"] = quality_profile_id.into();
    push(media_type.clone(), media.clone()).await;

    PluginReturn {
        result: String::new(),
        to_user: format!("🎬 Updated {media_type} with id {id} to {quality}"),
    }
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
    query: String,
    user_id: &String,
    user_name: &str,
) -> PluginReturn {
    let (media, id, _quality_profile_id, _quality) =
        match get_media_info(media_type.clone(), query.clone(), true).await {
            Ok(media_info) => media_info,
            Err(err) => {
                return PluginReturn {
                    result: String::new(),
                    to_user: err,
                }
            }
        };

    let tag_id: Option<u64> = get_user_tag_id(media_type.clone(), user_id, user_name).await;

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
                return PluginReturn {
                    result: format!("{media_type} with id {id} has been unrequested for user"),
                    to_user: format!("🎬 {media_type} with id {id} has been unrequested for user"),
                };
            }
        }
    };
    PluginReturn {
        result: format!("{media_type} with id {id} isnt on the server"),
        to_user: format!("❌ Can't remove {media_type} with id {id}, it isnt on the server"),
    }
}

/// Check for media the user wants
pub async fn wanted(
    media_type: Format,
    query: String,
    user_id: &String,
    user_name: &str,
) -> PluginReturn {
    if query.to_lowercase() == "none" {
        let none_media = get_media_with_no_user_tags(media_type.clone())
            .await
            .join(", ");
        return PluginReturn {
            result: format!("🎬 {media_type} with no users requests: {none_media}"),
            to_user: format!("🎬 {media_type} with no users requests found"),
        };
    }
    let user: String = if query == "self" {
        user_name_from_id(user_id, user_name).await.unwrap()
    } else {
        query.to_lowercase()
    };
    let user_media = get_media_with_user_tag(media_type.clone(), &user)
        .await
        .join(", ");
    PluginReturn {
        result: format!("🎬 {media_type} requested by {user}: {user_media}"),
        to_user: format!("🎬 {media_type} requested by {user} found"),
    }
}

/// Get from the memories file the users name if it exists, cleaned up string
async fn user_name_from_id(user_id: &String, user_name_dirty: &str) -> Option<String> {
    let contents = std::fs::read_to_string("memories.toml");
    if contents.is_err() {
        return None;
    }
    let parsed_toml: toml::Value = contents.unwrap().parse().unwrap();
    let user = parsed_toml.get(user_id)?;
    // If doesn't have the name, add it and write the file
    if !user.as_table().unwrap().contains_key("name") {
        // Convert name to plaintext alphanumeric only with gpt
        let response = apis::gpt_info_query(
            "gpt-4".to_string(),
            user_name_dirty.to_string(),
            "Convert the above name to plaintext alphanumeric only".to_string(),
        )
        .await;
        if response.is_err() {
            return None;
        }
        // Write file
        let name = response.unwrap();
        let mut user = user.as_table().unwrap().clone();
        user.insert("name".to_string(), toml::Value::String(name));
        let mut parsed_toml = parsed_toml.as_table().unwrap().clone();
        parsed_toml.insert(user_id.to_string(), toml::Value::Table(user));
        let toml_string = toml::to_string(&parsed_toml).unwrap();
        std::fs::write("memories.toml", toml_string).unwrap();
    }
    // Return clean name
    let user_name = user.get("name").unwrap().as_str().unwrap().to_string();
    Some(user_name)
}

/// Get user tag id
async fn get_user_tag_id(media_type: Format, user_id: &String, user_name: &str) -> Option<u64> {
    // Get user name clean
    let user_name = user_name_from_id(user_id, user_name)
        .await
        .unwrap_or(user_name.to_string());

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
    // User name to id in memories file if exists
    let contents = std::fs::read_to_string("memories.toml");
    if contents.is_err() {
        return Vec::new();
    }
    let parsed_toml: toml::Value = contents.unwrap().parse().unwrap();
    let mut user_id = None;
    for (id, user) in parsed_toml.as_table().unwrap() {
        if !user.as_table().unwrap().contains_key("name") {
            continue;
        }
        if user.get("name").unwrap().as_str().unwrap().to_lowercase() == user_name.to_lowercase() {
            user_id = Some(id.parse::<u64>().unwrap());
        }
    }
    if user_id.is_none() {
        return Vec::new();
    }
    let user_id: String = user_id.unwrap().to_string();
    // Get user tag id
    let tag_id = get_user_tag_id(media_type.clone(), &user_id, user_name).await;
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

/// Get media info from sonarr or radarr based on a search query
async fn get_media_info(
    media_type: Format,
    query: String,
    is_tmdb: bool, // Is tmdb/tvdb id or sonarr/radarr id
) -> Result<(Value, u32, u8, String), String> {
    // Convert query string to id: u32, quality: String, split on ; and convert types, return if error
    // id is either tmdb_id or tvdb_id
    let mut query_split = query.split(';');
    let id: u32 = match query_split.next() {
        Some(id) => match id.parse::<u32>() {
            Ok(id) => id,
            Err(_) => {
                return Err(format!("❌ {media_type} search failed, id not a number"));
            }
        },
        None => {
            return Err(format!("❌ {media_type} search failed, id not found"));
        }
    };
    // Get media info based on media type (Movie or Series)
    let (lookup_path, service) = if is_tmdb {
        match media_type {
            Format::Movie => (
                format!("/api/v3/movie/lookup/tmdb?tmdbId={id}"),
                apis::ArrService::Radarr,
            ),
            Format::Series => (
                format!("/api/v3/series/lookup?term=tvdbId {id}"),
                apis::ArrService::Sonarr,
            ),
        }
    } else {
        match media_type {
            Format::Movie => (format!("/api/v3/movie/{id}"), apis::ArrService::Radarr),
            Format::Series => (format!("/api/v3/series/{id}"), apis::ArrService::Sonarr),
        }
    };

    let mut media =
        apis::arr_request(apis::HttpMethod::Get, service.clone(), lookup_path, None).await;
    // If is series, get first result
    if matches!(media_type, Format::Series) {
        media = media[0].clone();
    };

    // If is another in the query, get resolution else return as is
    if query_split.nth(1).is_none() {
        return Ok((media, id, 0u8, String::new()));
    }
    // Get quality
    let quality: String = match query_split.next() {
        Some(quality) => quality.to_string(),
        None => {
            return Err(format!("❌ {media_type} search failed, quality not found"));
        }
    };
    // Convert quality string to quality profile id
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
    // Default to 4 which is 1080p if none found
    let quality_profile_id: u8 = *quality_profiles.get(quality.as_str()).unwrap_or(&4);

    Ok((media, id, quality_profile_id, quality))
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
