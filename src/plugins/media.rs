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
    "[MEDIA_LOOKUP~name;query]: Looks on server for a series or movie, replies with found info, can specify what data to request back, such as resolution, file sizes etc
[MEDIA_ADD~series;quality]: Adds a series or movie to the server from the name, can specify resolution, add to users memory that they want this series [MEM_SET~series;wants The Office]
[MEDIA_SETRES~series;quality]: Sets the resolution of a series or movie on the server
If user wants to remove a series, set the memory that they dont want it [MEM_SET~series;doesnt want The Office]
Examples: [MEDIA_LOOKUP~Stargate SG1,Stargate Atlantis;resolution,filesizes], if user is asking for example \"what mcu movies are on\" then you must do a [WEB~all mcu movies with release date] first to get list of mcu movies, then lookup each in a format like this [MEDIA_LOOKUP~Iron Man,Thor,Ant Man,Black Widow,...;title,year,...]".to_string()
}

/// Get processing message
pub async fn processing_message_lookup(query: String) -> String {
    return format!("🎬 Looking up media with query {query}");
}

/// Perform a lookup with ai processing to answer a prompt
pub async fn media_lookup(openai_client: &OpenAiClient, search: String) -> PluginReturn {
    // Get the key and query
    let (term, query) = match search.split_once(";") {
        Some((term, query)) => (term, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid media query"),
                to_user: String::from("❌ Media lookup failed"),
            }
        }
    };

    // Recreate as generic lookup, lookup both radarr and sonnar, then return results
    // Two lists of results for movies, 2 for series
    // List for on server, list for found with term but not on server which only gives basic details

    return PluginReturn {
        result: String::from(""),
        to_user: format!("🎬 Looking up media with query {query}"),
    };
}
