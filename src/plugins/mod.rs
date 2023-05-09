pub mod media;
pub mod memories;
pub mod relevance;
pub mod websearch;

/// Get data for plugins
pub fn get_data() -> String {
    let mut data = String::new();
    data.push_str("You have access to the following plugins:\n\n");
    data.push_str(format!("{}\n", &websearch::get_plugin_data()).as_str());
    data.push_str(format!("{}\n", &memories::get_plugin_data()).as_str());
    data.push_str(format!("{}\n", &media::get_plugin_data()).as_str());
    data.push_str("\nYou can query a plugin with the syntax [plugin~query]\nFor example [WEB~how far is the moon?]I'm looking this up\nSystem will respond with results [WEB~how far is the moon?~The average distance between the Earth and the Moon is 384,400km]\nAlways use plugins to get or set data when needed, after a plugin is queried write a message to let the user know its being processed, plugin calls should always be first in the message");
    data
}

#[derive(Debug)]
pub struct PluginReturn {
    pub result: String,
    pub to_user: String,
}

/// Get command processing message
pub fn get_processing_message(command: &str) -> String {
    let args = command.split('~').collect::<Vec<&str>>();

    let result: String = match *args.first().unwrap() {
        "WEB" => websearch::processing_message(&args[1].to_string()),
        "MEM_GET" => memories::processing_message_get(&args[1].to_string()),
        "MEM_SET" => memories::processing_message_set(&args[1].to_string()),
        "MOVIE_LOOKUP" | "SERIES_LOOKUP" => media::processing_message_lookup(&args[1].to_string()),
        "MOVIE_ADD" | "SERIES_ADD" => media::processing_message_add(&args[1].to_string()),
        "MOVIE_SETRES" | "SERIES_SETRES" => media::processing_message_setres(&args[1].to_string()),
        _ => String::from("❌ Unknown command"),
    };

    result
}

/// Run a command with a result
pub async fn run_command(command: &str, user_id: &String, user_name: &str) -> PluginReturn {
    let args = command.split('~').collect::<Vec<&str>>();
    println!("Running command: {args:?}");

    let result: PluginReturn = match *args.first().unwrap() {
        "WEB" => websearch::ai_search(args[1].to_string()).await,
        "MEM_GET" => memories::memory_get(args[1].to_string(), user_id).await,
        "MEM_SET" => memories::memory_set(args[1].to_string(), user_id, user_name).await,
        "MOVIE_LOOKUP" => media::lookup(media::Format::Movie, args[1].to_string()).await,
        "SERIES_LOOKUP" => media::lookup(media::Format::Series, args[1].to_string()).await,
        "MOVIE_ADD" => media::add(media::Format::Movie, args[1].to_string()).await,
        "SERIES_ADD" => media::add(media::Format::Series, args[1].to_string()).await,
        "MOVIE_SETRES" => media::setres(media::Format::Movie, args[1].to_string()).await,
        "SERIES_SETRES" => media::setres(media::Format::Series, args[1].to_string()).await,
        _ => PluginReturn {
            result: String::from("Unknown command"),
            to_user: String::from("❌ Attempted invalid command"),
        },
    };
    println!("Command result: {result:?}");

    result
}
