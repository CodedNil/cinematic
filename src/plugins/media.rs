use crate::{apis, plugins::PluginReturn};

// Plugins data
pub fn get_plugin_data() -> String {
    "[MEDIA_LOOKUP~name;query]: Looks on server for a series or movie, replies with found info, can specify what data to request back, such as resolution, file sizes etc
[MEDIA_ADD~series;quality]: Adds a series or movie to the server from the name, can specify resolution, add to users memory that they want this series [MEM_SET~series;wants The Office]
[MEDIA_SETRES~series;quality]: Sets the resolution of a series or movie on the server
If user wants to remove a series, set the memory that they dont want it [MEM_SET~series;doesnt want The Office]
Examples: [MEDIA_LOOKUP~Stargate SG1,Stargate Atlantis;resolution,filesizes], if user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MEDIA_LOOKUP~Iron Man 1,Thor 1,Black Widow,...;title,year,...]".to_string()
}

/// Get processing message
pub async fn processing_message_lookup(query: String) -> String {
    return format!("🎬 Looking up media with query {query}");
}

/// Perform a lookup with ai processing to answer a prompt
pub async fn media_lookup(search: String) -> PluginReturn {
    // Get the key and query
    let (terms, query) = match search.split_once(";") {
        Some((terms, query)) => (terms, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid media query"),
                to_user: String::from("❌ Media lookup failed"),
            }
        }
    };
    // Search per term
    for term in terms.split(",").collect::<Vec<&str>>() {
        // Await all the results
        let results = tokio::join!(
            apis::arr_get(apis::ArrService::Sonarr, String::from("/api/v3/series")),
            apis::arr_get(apis::ArrService::Radarr, String::from("/api/v3/movie")),
            apis::arr_get(
                apis::ArrService::Sonarr,
                format!("/api/v3/series/lookup?term={}", term),
            ),
            apis::arr_get(
                apis::ArrService::Radarr,
                format!("/api/v3/movie/lookup?term={}", term),
            ),
        );
        let (sonarr_all, radarr_all, sonarr_search_r, radarr_search_r) = results;
        // Trim the searches so each only contains 5 results max
        let sonarr_search = trim_results(sonarr_search_r, 5);
        let radarr_search = trim_results(radarr_search_r, 5);

        // Get titles of the media items that are relevant
        let mut sonarr_titles: Vec<String> = Vec::new();
        let mut radarr_titles: Vec<String> = Vec::new();
        for media in sonarr_search.as_array().unwrap() {
            let title = media["title"].as_str().unwrap().to_string();
            let year: String = media["year"].as_u64().unwrap().to_string();
            let tvdb_id: String = media["tvdbId"].as_u64().unwrap().to_string();
            let full = format!("{} ({}) tvdbId{}", title, year, tvdb_id);
            if !sonarr_titles.contains(&full) {
                sonarr_titles.push(full);
            }
        }
        for media in radarr_search.as_array().unwrap() {
            let title = media["title"].as_str().unwrap().to_string();
            let year: String = media["year"].as_u64().unwrap().to_string();
            let tmdb_id: String = media["tmdbId"].as_u64().unwrap().to_string();
            let full = format!("{} ({}) tmdbId{}", title, year, tmdb_id);
            if !radarr_titles.contains(&full) {
                radarr_titles.push(full);
            }
        }
        let sonarr_titles = sonarr_titles.join(";");
        let radarr_titles = radarr_titles.join(";");
        let all_titles = format!("Series:{}|Movies:{}", sonarr_titles, radarr_titles);

        // Search with gpt through series and movies to get ones of relevance
        println!("Searching with gpt {term}");
        let relevances = apis::gpt_info_query("gpt-3.5-turbo".to_string(), all_titles, format!("Based on the given information and only this information query with \"{term}\"\nList media in order of relevance\nOutput on one line compact in this format: M~confidence~tmdbId Example:\nMOVIE_CONFIDENCE 78% M~80%~542;M~40%~788\nSERIES_CONFIDENCE 25% S~22%~772")).await.unwrap_or_default();
        // Split by ; then by ~ to get data
        println!("{:?}", relevances);
        let relevances: Vec<Vec<String>> = relevances
            .split(";")
            .map(|x| x.split("~").map(|x| x.to_string()).collect())
            .collect();
        println!("{:?}", relevances);
        let mut is_series = 0;
        let mut is_movie = 0;
        let mut series: Vec<(u8, String)> = Vec::new();
        let mut movies: Vec<(u8, String)> = Vec::new();
        for relevance in relevances {
            if relevance.len() == 3 {
                // Get percentage confidence, "50%" -> 50. to int u8
                let confidence: u8 = relevance[1].split("%").collect::<Vec<&str>>()[0]
                    .parse::<u8>()
                    .unwrap_or_default();
                match relevance[0].as_str() {
                    "SeriesLikelyhood" => {
                        is_series = confidence;
                    }
                    "MovieLikelyhood" => {
                        is_movie = confidence;
                    }
                    "S" => {
                        series.push((confidence, relevance[2].clone()));
                    }
                    "M" => {
                        movies.push((confidence, relevance[2].clone()));
                    }
                    _ => {}
                }
            }
        }
        println!(
            "Series?{}: {:?} Movies?{}: {:?}",
            is_series, series, is_movie, movies
        );
        // TODO return with basic details on these
    }

    return PluginReturn {
        result: String::from(""),
        to_user: format!("🎬 Looking up media with query {query}"),
    };
}

fn trim_results(mut search_results: serde_json::Value, max_results: usize) -> serde_json::Value {
    if let serde_json::Value::Array(ref mut results) = search_results {
        results.truncate(max_results);
    }
    search_results
}
