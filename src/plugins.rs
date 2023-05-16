//! Plugins interface for the chatbot

pub mod media;
pub mod memories;
pub mod websearch;

/// Get data for plugins
pub fn get_data() -> String {
    let mut data = String::new();
    data.push_str("You have access to the following plugins:\n\n");
    data.push_str(format!("{}\n", &websearch::get_plugin_data()).as_str());
    // data.push_str(format!("{}\n", &memories::get_plugin_data()).as_str());
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

    let second_arg: String = (*args.get(1).unwrap_or(&"")).to_string();
    let result: String = match *args.first().unwrap() {
        "WEB" => websearch::processing_message(&second_arg),
        "MEM_GET" => memories::processing_message_get(&second_arg),
        "MEM_SET" => memories::processing_message_set(&second_arg),
        "MOVIES_LOOKUP" | "SERIES_LOOKUP" => media::processing_message_lookup(&second_arg),
        "MOVIES_ADD" | "SERIES_ADD" => media::processing_message_add(&second_arg),
        "MOVIES_SETRES" | "SERIES_SETRES" => media::processing_message_setres(&second_arg),
        "MOVIES_REMOVE" | "SERIES_REMOVE" => media::processing_message_remove(&second_arg),
        "MOVIES_WANTED" | "SERIES_WANTED" => media::processing_message_wanted(&second_arg),
        _ => String::from("❌ Unknown command"),
    };

    result
}

/// Run a command with a result
pub async fn run_command(command: &str, user_id: &String, user_name: &str) -> PluginReturn {
    let args = command.split('~').collect::<Vec<&str>>();
    println!("Running command: {args:?}");

    let first_arg = *args.first().unwrap();
    let second_arg: String = (*args.get(1).unwrap_or(&"")).to_string();
    let format: media::Format = if first_arg.starts_with("SERIES") {
        media::Format::Series
    } else {
        media::Format::Movie
    };
    let result: PluginReturn = match first_arg {
        "WEB" => websearch::ai_search(second_arg).await,
        "MEM_GET" => memories::memory_get(&second_arg, user_id).await,
        "MEM_SET" => memories::memory_set(&second_arg, user_id, user_name).await,
        "MOVIES_LOOKUP" | "SERIES_LOOKUP" => media::lookup(format, second_arg).await,
        "MOVIES_ADD" | "SERIES_ADD" => media::add(format, second_arg, user_name).await,
        "MOVIES_SETRES" | "SERIES_SETRES" => media::setres(format, second_arg).await,
        "MOVIES_REMOVE" | "SERIES_REMOVE" => media::remove(format, second_arg, user_name).await,
        "MOVIES_WANTED" | "SERIES_WANTED" => media::wanted(format, second_arg, user_name).await,
        _ => PluginReturn {
            result: String::from("Unknown command"),
            to_user: String::from("❌ Attempted invalid command"),
        },
    };
    println!("Command result: {result:?}");

    result
}
