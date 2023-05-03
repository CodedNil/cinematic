use crate::{apis, plugins::PluginReturn};

use serde_json::Value;
use std::collections::HashMap;

// Plugins data
pub fn get_plugin_data() -> String {
    "[MOVIE_LOOKUP~query]: Searches for a movie or movies from a query for example \"is iron man on? is watchmen the ultimate cut?\"
[MEDIA_ADD~series;quality]: Adds a series or movie to the server from the name, can specify resolution, add to users memory that they want this series [MEM_SET~series;wants The Office]
[MEDIA_SETRES~series;quality]: Sets the resolution of a series or movie on the server
If user wants to remove a series, set the memory that they dont want it [MEM_SET~series;doesnt want The Office]
If user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MOVIE_LOOKUP~Are these movies on Iron Man 1,Thor 1,Black Widow,...]".to_string()
}
// Get size of movies, series, per user or total

/// Get processing message
pub async fn processing_message_lookup(query: String) -> String {
    return format!("🎬 Looking up media with query {query}");
}

/// Perform a lookup of movies with ai processing to answer a prompt
pub async fn movie_lookup(query: String) -> PluginReturn {
    // Use gpt to get a list of all movies to search for
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo".to_string(),
        query.clone(),
        format!("From the text above gather a list of movie titles mentioned, mention each movies title in its core form such as ('Avatar: The Way of Water' to 'Avatar', 'The Lord of the Rings: Return of the King Ultimate Cut' to just 'Lord of the Rings', 'Thor 2' to 'Thor'), return a list with ; divider on a single line"),
    )
    .await
    .unwrap_or_default();
    let terms: Vec<String> = response.split(";").map(|s| s.trim().to_string()).collect();

    // Get list of all movies, and searches per term
    let mut arr_searches = vec![apis::arr_get(
        apis::ArrService::Radarr,
        String::from("/api/v3/movie"),
    )];
    for term in terms.iter() {
        arr_searches.push(apis::arr_get(
            apis::ArrService::Radarr,
            format!("/api/v3/movie/lookup?term={}", term),
        ));
    }
    // Wait for all searches to finish parallel
    let arr_searches = futures::future::join_all(arr_searches).await;
    // let radarr_all = arr_searches[0].clone();
    // Gather human readable data for each item
    let mut media_strings: Vec<String> = Vec::new();
    for (index, search) in arr_searches.iter().enumerate() {
        if index == 0 {
            continue;
        }
        let search_results = search.clone();
        // Trim the searches so each only contains 5 results max and output plain english
        let media_string = movies_to_plain_english(search_results, 5);
        media_strings.push(media_string.clone());
    }

    // Search with gpt through series and movies to get ones of relevance
    let response = apis::gpt_info_query(
        "gpt-3.5-turbo".to_string(),
        media_strings.join("\n"),
        format!("Using very concise language\nBased on the given information and only this information answer, {query}"),
    )
    .await
    .unwrap_or_default();

    return PluginReturn {
        result: response,
        to_user: format!("🎬 Movie lookup successful for {query}"),
    };
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
