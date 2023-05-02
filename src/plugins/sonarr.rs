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
    "[SONARR_LOOKUP~series;query]: Looks on server for a series or movie, replies with found series, and ones not on the server that match, can specify what data to request back, such as resolution, file sizes etc\nExamples: [SONARR_LOOKUP~Stargate SG1,Stargate Atlantis;resolution,filesizes], if user is asking for example \"what mcu movies are on\" then you must do a web search first to get list of mcu movies, then lookup each in a format like this [SONARR_LOOKUP~Iron Man,Thor,Ant Man,Black Widow,....]".to_string()
}

/// Get processing message
pub async fn processing_message_lookup(query: String) -> String {
    return format!("🎬 Looking up series with query {query}");
}

/// Perform a lookup with ai processing to answer a prompt
pub async fn sonarr_lookup(openai_client: &OpenAiClient, search: String) -> PluginReturn {
    // Get the key and query
    let (term, query) = match search.split_once(";") {
        Some((term, query)) => (term, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid sonarr query"),
                to_user: String::from("❌ Sonarr lookup failed"),
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
