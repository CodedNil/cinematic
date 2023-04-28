pub mod examples;
pub mod relevance;
pub mod websearch;

use async_openai::Client as OpenAiClient;

/// Get data for plugins
pub fn get_data() -> String {
    let mut data = String::new();
    data.push_str("You have access to the following plugins:\n\n");
    data.push_str(format!("{}\n", &websearch::get_plugin_data()).as_str());
    data.push_str("\nYou can query a plugin with the syntax [plugin~query] a !plugin means it expects a result\nFor example [!WEB~how far is the moon?]I'm looking this up\nSystem will respond with results [!WEB~how far is the moon?~The average distance between the Earth and the Moon is 384,400km]\nAlways use plugins to get or set data when needed, after a plugin is queried write a message to let the user know its being processed");
    data
}

/// Run a command
pub async fn run_command(openai_client: &OpenAiClient, command: String) {
    let args: Vec<&str> = command.split('~').collect::<Vec<&str>>();
}

/// Run a command with a result
pub async fn run_command_result(openai_client: &OpenAiClient, command: &String) -> String {
    let args = command.split('~').collect::<Vec<&str>>();

    let result = match args[0] {
        "!WEB" => websearch::ai_search(&openai_client, args[1].to_string().clone()).await,
        _ => String::from("Unknown command"),
    };

    return result;
}
