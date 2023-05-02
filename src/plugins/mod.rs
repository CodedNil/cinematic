pub mod examples;
pub mod relevance;
pub mod websearch;

use async_openai::Client as OpenAiClient;

/// Get data for plugins
pub fn get_data() -> String {
    let mut data = String::new();
    data.push_str("You have access to the following plugins:\n\n");
    data.push_str(format!("{}\n", &websearch::get_plugin_data()).as_str());
    data.push_str("\nYou can query a plugin with the syntax [plugin~query]\nFor example [WEB~how far is the moon?]I'm looking this up\nSystem will respond with results [WEB~how far is the moon?~The average distance between the Earth and the Moon is 384,400km]\nAlways use plugins to get or set data when needed, after a plugin is queried write a message to let the user know its being processed, plugin calls should always be first in the message");
    data
}

pub struct PluginReturn {
    pub result: String,
    pub to_user: String,
}

/// Get command processing message
pub async fn get_processing_message(command: &String) -> String {
    let args = command.split('~').collect::<Vec<&str>>();

    let result: String = match args[0] {
        "WEB" => websearch::processing_message(args[1].to_string()).await,
        _ => String::from("Unknown command"),
    };

    return result;
}

/// Run a command with a result
pub async fn run_command(openai_client: &OpenAiClient, command: &String) -> PluginReturn {
    let args = command.split('~').collect::<Vec<&str>>();

    let result: PluginReturn = match args[0] {
        "WEB" => websearch::ai_search(&openai_client, args[1].to_string()).await,
        _ => PluginReturn {
            result: String::from("Unknown command"),
            to_user: String::from("Attempted invalid command"),
        },
    };

    return result;
}
