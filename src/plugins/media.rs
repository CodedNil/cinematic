use async_openai::{
    types::{
        ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs,
        CreateChatCompletionResponse, Role,
    },
    Client as OpenAiClient,
};

use crate::plugins::PluginReturn;

// Plugins data
pub fn get_plugin_data() -> String {
    "[SERIES_LOOKUP~series;query]: Looks on server for a series or movie, replies with found series, and ones not on the server that match, can specify what data to request back, such as resolution, file sizes etc
[SERIES_ADD~series;quality]: Adds a series to the server from the name, can specify resolution, add to users memory that they want this series [MEM_SET~series;wants The Office]
[SERIES_SETRES~series;quality]: Sets the resolution of a series on the server
If user wants to remove a series, set the memory that they dont want it [MEM_SET~series;doesnt want The Office]
Equivalent commands exist for movies [MOVIES_LOOKUP~movies;query], [MOVIES_ADD~movies;quality], [MOVIES_SETRES~movies;quality]
Examples: [SERIES_LOOKUP~Stargate SG1,Stargate Atlantis;resolution,filesizes], if user is asking for example \"what mcu movies are on\" then you must do a web search first to get list of mcu movies, then lookup each in a format like this [SERIES_LOOKUP~Iron Man,Thor,Ant Man,Black Widow,....]".to_string()
}

/// Get processing message
pub async fn processing_message_series_lookup(query: String) -> String {
    return format!("🎬 Looking up series with query {query}");
}

/// Perform a lookup with ai processing to answer a prompt
pub async fn series_lookup(openai_client: &OpenAiClient, search: String) -> PluginReturn {
    // Get the key and query
    let (term, query) = match search.split_once(";") {
        Some((term, query)) => (term, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid series query"),
                to_user: String::from("❌ Series lookup failed"),
            }
        }
    };

    // Create two lists, series on server that match search, and series not on server that match search
    // Ones on server get details, ones not on server are name and year only and maybe tmdbid

    return PluginReturn {
        result: String::from(""),
        to_user: format!("🎬 Looking up series with query {query}"),
    };
}
