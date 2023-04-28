pub mod examples;
pub mod relevance;
pub mod websearch;

// Get data for plugins
pub fn get_data() -> String {
    let mut data = String::new();
    data.push_str("You have access to the following plugins:\n\n");
    data.push_str(&websearch::get_plugin_data());
    data.push_str("\nYou can query a plugin with the syntax [plugin~query]\nFor example [WEB~how far is the moon?]\nSystem will respond with results [WEB~how far is the moon?~The average distance between the Earth and the Moon is 384,400km]\nAlways use plugins to get or set data when needed, after a plugin is queried write a message to let the user know its being processed eg [WEB~how far is the moon?]I'm looking this up");
    data
}
