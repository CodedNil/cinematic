use crate::{apis, plugins::PluginReturn};

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum MediaType {
    Movie,
    Series,
}
impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            MediaType::Movie => write!(f, "Movie"),
            MediaType::Series => write!(f, "Series"),
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
If user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MOVIE_LOOKUP~Are these movies on Iron Man 1,Thor 1,Black Widow,...]".to_string()
}
// Get size of movies, series, per user or total

/// Get processing message
pub async fn processing_message_lookup(query: String) -> String {
    return format!("🎬 Looking up media {query}");
}

pub async fn processing_message_add(query: String) -> String {
    return format!("🎬 Adding media {query}");
}

pub async fn processing_message_setres(query: String) -> String {
    return format!("🎬 Changing quality {query}");
}

/// Perform a lookup of movies with ai processing to answer a prompt
pub async fn movie_lookup(query: String) -> PluginReturn {
    // Use gpt to get a list of all movies to search for
    let response = apis::gpt_info_query(
        "gpt-4".to_string(),
        query.clone(),
        format!("The above text is a query to lookup movies, from the text above gather a list of movie titles mentioned, mention each movies title in its core form such as ('Avatar: The Way of Water' to 'Avatar', 'The Lord of the Rings: Return of the King Ultimate Cut' to just 'Lord of the Rings', 'Thor 2' to 'Thor'), return a list with ; divider on a single line"),
    )
    .await
    .unwrap_or_default();
    let terms: Vec<String> = response.split(";").map(|s| s.trim().to_string()).collect();

    // Get list of all movies, and searches per term
    let mut arr_searches = vec![];
    for term in terms.iter() {
        arr_searches.push(apis::arr_request(
            apis::HttpMethod::Get,
            apis::ArrService::Radarr,
            format!("/api/v3/movie/lookup?term={}", term),
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
        media_strings.push(movies_to_plain_english(search_results, 5));
    }

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo".to_string(),
        media_strings.join("\n"),
        format!("Using very concise language\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    return PluginReturn {
        result: response,
        to_user: format!("🎬 Movie lookup successful for {query}"),
    };
}

/// Perform a lookup of series with ai processing to answer a prompt
pub async fn series_lookup(query: String) -> PluginReturn {
    // Use gpt to get a list of all series to search for
    let response = apis::gpt_info_query(
        "gpt-4".to_string(),
        query.clone(),
        format!("From the text above gather a list of tv series titles mentioned, mention each series title in its core form such as ('Stargate: SG1' to 'Stargate', 'The Witcher' to just 'Witcher'), return a list with ; divider on a single line"),
    )
    .await
    .unwrap_or_default();
    let terms: Vec<String> = response.split(";").map(|s| s.trim().to_string()).collect();

    // Get list of all movies, and searches per term
    let mut arr_searches = vec![];
    for term in terms.iter() {
        arr_searches.push(apis::arr_request(
            apis::HttpMethod::Get,
            apis::ArrService::Sonarr,
            format!("/api/v3/series/lookup?term={}", term),
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
        media_strings.push(series_to_plain_english(search_results, 5));
    }

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo".to_string(),
        media_strings.join("\n"),
        format!("Using very concise language\nBased on the given information and only this information give your best answer to, {query}"),
    )
    .await
    .unwrap_or_default();

    return PluginReturn {
        result: response,
        to_user: format!("🎬 Series lookup successful for {query}"),
    };
}

pub async fn media_add(media_type: MediaType, query: String) -> PluginReturn {
    let (mut media, id, quality_profile_id, quality) =
        match get_media_info(media_type.clone(), query.clone(), true).await {
            Ok(media_info) => media_info,
            Err(err) => {
                return PluginReturn {
                    result: String::from(""),
                    to_user: err,
                }
            }
        };

    // Check if media is already on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() != 0 {
            return PluginReturn {
                result: format!("{} with id {} is already on the server", media_type, id),
                to_user: format!(
                    "❌ Can't add movie {} with id {} is already on the server",
                    media_type, id
                ),
            };
        }
    };

    // Update media json with quality profile id
    media["qualityProfileId"] = quality_profile_id.into();
    media["monitored"] = true.into();
    media["minimumAvailability"] = "announced".into();
    match media_type {
        MediaType::Movie => {
            media["addOptions"] = serde_json::json!({ "searchForMovie": true });
            media["rootFolderPath"] = "/movies".into();
        }
        MediaType::Series => {
            media["addOptions"] = serde_json::json!({ "searchForMissingEpisodes": true });
            media["rootFolderPath"] = "/tv".into();
            media["languageProfileId"] = 1.into();
        }
    }

    // Media to json
    let media = serde_json::to_string(&media).unwrap();

    // Add the media to sonarr or radarr
    match media_type {
        MediaType::Movie => {
            apis::arr_request(
                apis::HttpMethod::Post,
                apis::ArrService::Radarr,
                "/api/v3/movie".to_string(),
                Some(media),
            )
            .await;
        }
        MediaType::Series => {
            apis::arr_request(
                apis::HttpMethod::Post,
                apis::ArrService::Sonarr,
                "/api/v3/series".to_string(),
                Some(media),
            )
            .await;
        }
    };

    return PluginReturn {
        result: String::from(""),
        to_user: format!("🎬 Added {} with id {} in {}", media_type, id, quality),
    };
}

pub async fn media_setres(media_type: MediaType, query: String) -> PluginReturn {
    let (mut media, id, quality_profile_id, quality) =
        match get_media_info(media_type.clone(), query.clone(), false).await {
            Ok(media_info) => media_info,
            Err(err) => {
                return PluginReturn {
                    result: String::from(""),
                    to_user: err,
                }
            }
        };

    // Check if media is on the server
    if let Value::Number(id) = &media["id"] {
        if id.as_u64().unwrap() == 0 {
            return PluginReturn {
                result: format!("{} with id {} isnt on the server", media_type, id),
                to_user: format!(
                    "❌ Couldn't change resolution, {} with id {} isnt on the server",
                    media_type, id
                ),
            };
        }
    } else {
        return PluginReturn {
            result: format!("{} with id {} isnt on the server", media_type, id),
            to_user: format!(
                "❌ Couldn't change resolution, {} with id {} isnt on the server",
                media_type, id
            ),
        };
    }

    // Update media json with quality profile id
    media["qualityProfileId"] = quality_profile_id.into();
    let media_id = media["id"].as_u64().unwrap().clone();
    // Media to json
    let media = serde_json::to_string(&media).unwrap();

    // Push the media to sonarr or radarr
    match media_type {
        MediaType::Movie => {
            apis::arr_request(
                apis::HttpMethod::Put,
                apis::ArrService::Radarr,
                format!("/api/v3/movie/{}", media_id),
                Some(media),
            )
            .await
        }
        MediaType::Series => {
            apis::arr_request(
                apis::HttpMethod::Put,
                apis::ArrService::Sonarr,
                format!("/api/v3/series/{}", media_id),
                Some(media),
            )
            .await
        }
    };

    return PluginReturn {
        result: String::from(""),
        to_user: format!("🎬 Updated {} with id {} to {}", media_type, id, quality),
    };
}

/// Get media info from sonarr or radarr based on a search query
async fn get_media_info(
    media_type: MediaType,
    query: String,
    is_tmdb: bool, // Is tmdb/tvdb id or sonarr/radarr id
) -> Result<(Value, u32, u8, String), String> {
    // Convert query string to id: u32, quality: String, split on ; and convert types, return if error
    // id is either tmdb_id or tvdb_id
    let mut query_split = query.split(";");
    let id: u32 = match query_split.next() {
        Some(id) => match id.parse::<u32>() {
            Ok(id) => id,
            Err(_) => {
                return Err(format!("❌ {} search failed, id not a number", media_type));
            }
        },
        None => {
            return Err(format!("❌ {} search failed, id not found", media_type));
        }
    };
    let quality: String = match query_split.next() {
        Some(quality) => quality.to_string(),
        None => {
            return Err(format!(
                "❌ {} search failed, quality not found",
                media_type
            ));
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
    .cloned()
    .collect();
    // Default to 4 which is 1080p if none found
    let quality_profile_id: u8 = *quality_profiles.get(quality.as_str()).unwrap_or(&4);

    // Get media info based on media type (Movie or Series)
    let (lookup_path, service) = match is_tmdb {
        true => match media_type {
            MediaType::Movie => (
                format!("/api/v3/movie/lookup/tmdb?tmdbId={}", id),
                apis::ArrService::Radarr,
            ),
            MediaType::Series => (
                format!("/api/v3/series/lookup/tmdb?tmdbId={}", id),
                apis::ArrService::Sonarr,
            ),
        },
        false => match media_type {
            MediaType::Movie => (format!("/api/v3/movie/{}", id), apis::ArrService::Radarr),
            MediaType::Series => (format!("/api/v3/series/{}", id), apis::ArrService::Sonarr),
        },
    };

    let mut media =
        apis::arr_request(apis::HttpMethod::Get, service.clone(), lookup_path, None).await;
    // If is series, get first result
    if let MediaType::Series = media_type {
        media = media[0].clone();
    };

    Ok((media, id, quality_profile_id, quality))
}

/// Convert number size to string with units
fn sizeof_fmt(num: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut index = 0;
    let mut size = num as f64;
    while size >= 1024.0 && index < units.len() - 1 {
        size /= 1024.0;
        index += 1;
    }
    format!("{:.2} {}", size, units[index])
}

/// Convert json of movies data to plain english
fn movies_to_plain_english(response: Value, num: usize) -> String {
    let quality_profiles: HashMap<u8, &str> = [
        (2, "SD"),
        (3, "720p"),
        (4, "1080p"),
        (5, "2160p"),
        (6, "720p/1080p"),
        (7, "Any"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut results = Vec::new();
    if let Value::Array(movies) = response {
        for movie in movies {
            let mut result = Vec::new();
            // Get title and year
            if let (Value::String(title), Value::Number(year)) = (&movie["title"], &movie["year"]) {
                result.push(format!("{title} ({year})"));
            }
            // Get id and availability
            if let Value::Number(id) = &movie["id"] {
                if id.as_u64().unwrap() != 0 {
                    result.push("available on the server".to_string());
                    result.push(format!("radarr id {}", id));
                } else {
                    result.push("unavailable on the server".to_string());
                }
            } else {
                result.push("unavailable on the server".to_string());
            }
            // Get quality
            if let Value::Number(quality_profile_id) = &movie["qualityProfileId"] {
                if let Some(quality_profile_id_u8) = quality_profile_id.as_u64().map(|id| id as u8)
                {
                    if let Some(quality) = quality_profiles.get(&quality_profile_id_u8) {
                        result.push(format!("requested at quality {}", quality));
                    }
                }
            }
            // Get tmdbId
            if let Value::Number(tmdb_id) = &movie["tmdbId"] {
                result.push(format!("tmdbId {}", tmdb_id));
            }
            // Get file info
            if movie["hasFile"].as_bool().unwrap_or(false) {
                if let Value::Number(size_on_disk) = &movie["sizeOnDisk"] {
                    result.push(format!(
                        "file size {}",
                        sizeof_fmt(size_on_disk.as_u64().unwrap())
                    ));
                }
                let movie_file = &movie["movieFile"];
                if let Value::String(resolution) = &movie_file["mediaInfo"]["resolution"] {
                    result.push(format!("file resolution {}", resolution));
                }
                if let Value::String(edition) = &movie_file["edition"] {
                    if !edition.is_empty() {
                        result.push(format!("file edition {}", edition));
                    }
                }
            } else {
                result.push("no file on disk".to_string());
            }
            results.push(result.join(";"));

            if results.len() >= num {
                break;
            }
        }
    }
    results.join("\n")
}

/// Convert json of series data to plain english
fn series_to_plain_english(response: Value, num: usize) -> String {
    let quality_profiles: HashMap<u8, &str> = [
        (2, "SD"),
        (3, "720p"),
        (4, "1080p"),
        (5, "2160p"),
        (6, "720p/1080p"),
        (7, "Any"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut results = Vec::new();
    if let Value::Array(movies) = response {
        for movie in movies {
            let mut result = Vec::new();
            // Get title and year
            if let (Value::String(title), Value::Number(year)) = (&movie["title"], &movie["year"]) {
                result.push(format!("{title} ({year})"));
            }
            // Get id and availability
            if let Value::Number(id) = &movie["id"] {
                if id.as_u64().unwrap() != 0 {
                    result.push("available on the server".to_string());
                    result.push(format!("sonarr id {}", id));
                } else {
                    result.push("unavailable on the server".to_string());
                }
            } else {
                result.push("unavailable on the server".to_string());
            }
            // Get quality
            if let Value::Number(quality_profile_id) = &movie["qualityProfileId"] {
                if let Some(quality_profile_id_u8) = quality_profile_id.as_u64().map(|id| id as u8)
                {
                    if let Some(quality) = quality_profiles.get(&quality_profile_id_u8) {
                        result.push(format!("requested at quality {}", quality));
                    }
                }
            }
            // Get tvdbId
            if let Value::Number(tmdb_id) = &movie["tvdbId"] {
                result.push(format!("tvdbId {}", tmdb_id));
            }
            results.push(result.join(";"));

            if results.len() >= num {
                break;
            }
        }
    }
    results.join("\n")
}
