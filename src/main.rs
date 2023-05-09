use serenity::prelude::{Client as DiscordClient, GatewayIntents};

use std::fs::File;
use std::io::prelude::*;
use toml::Value;

mod apis;
mod chatbot;
mod discordbot;
mod plugins;

#[tokio::main]
async fn main() {
    // Read credentials.toml file to get keys
    let mut file = File::open("credentials.toml").expect("Failed to open credentials file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Failed to read credentials file");
    let cred: Value = contents.parse().expect("Failed to parse credentials TOML");

    // Configure the client with your Discord bot token
    let discord_token: String = cred["discord_token"]
        .as_str()
        .expect("Expected a discord_token in the credentials.toml file")
        .to_string();
    // Set gateway intents, which decides what events the bot will be notified about
    let intents: GatewayIntents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot
    let mut client: DiscordClient = DiscordClient::builder(&discord_token, intents)
        .event_handler(discordbot::Handler)
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
