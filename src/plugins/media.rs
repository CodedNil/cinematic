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
    "[MOVIE_LOOKUP~query] Searches for a movie or movies from a query for example \"is iron man on? is watchmen the ultimate cut?\"
[SERIES_LOOKUP~query] Query should be phrased as a question \"What is Cats movie tmdbId\" etc
[MOVIE_ADD~tmdbId;quality] Adds a movie to the server from the name, always needs to first lookup asking for tmdbId, can specify resolution, add to users memory that they want this movie [MEM_SET~movies;wants Watchmen], defaults to adding in 1080p, options are SD, 720p, 1080p, 2160p
[SERIES_ADD~tvdbId;quality]
[MOVIE_SETRES~id;quality] Sets the resolution of a series or movie on the server, always needs to first lookup asking for radarr id
[SERIES_SETRES~id;quality] Uses sonarr id
tmdbId and tvdbId can be found from the lookup commands, ask for it such as [SERIES_LOOKUP~whats game of thrones tvdbId?]
If user wants to remove a series, set the memory that they dont want it [MEM_SET~series;doesnt want The Office]
If user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MOVIE_LOOKUP~Are these movies on Iron Man 1,Thor 1,Black Widow,...]
If user queries \"how  many gbs do my added movies take up\" look up the memories of what movies the user has added, then lookup on the server to get file size of each".to_string()
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

    // Get list of all media, and searches per term
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
        media_strings.push(media_to_plain_english(&media_type, search_results, 5));
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

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
            return PluginReturn {
                result: format!("{media_type} with id {id} is already on the server"),
                to_user: format!(
                    "❌ Can't add movie {media_type} with id {id} is already on the server"
                ),
            };
        }
    };

    // Get user name clean
    let user_name = apis::user_name_from_id(user_id, user_name)
        .await
        .unwrap_or(user_name.to_string());

    // Sync then get all current tags
    let service = match media_type {
        Format::Movie => apis::ArrService::Radarr,
        Format::Series => apis::ArrService::Sonarr,
    };
    apis::sync_user_tags(service.clone()).await;
    let all_tags = apis::arr_request(
        apis::HttpMethod::Get,
        service,
        "/api/v3/tag".to_string(),
        None,
    )
    .await;
    // Get id for users tag [{id: 1, label: "added-username"}]
    let mut tag_id = 0;
    if let Value::Array(tags) = &all_tags {
        for tag in tags {
            if tag["label"].as_str() == Some(&format!("added-{user_name}")) {
                tag_id = tag["id"].as_u64().unwrap_or(0);
            }
        }
    }

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
    let media_id = media["id"].as_u64().unwrap();
    // Media to json
    let media = serde_json::to_string(&media).unwrap();

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

    PluginReturn {
        result: String::new(),
        to_user: format!("🎬 Updated {media_type} with id {id} to {quality}"),
    }
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
fn media_to_plain_english(media_type: &Format, response: Value, num: usize) -> String {
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
