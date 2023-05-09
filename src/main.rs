//! Creates the discord client and starts listening for events.

use serenity::prelude::{Client as DiscordClient, GatewayIntents};

mod apis;
mod chatbot;
mod discordbot;
mod plugins;

#[tokio::main]
async fn main() {

    let user_names = vec!["user1", "user2", "user3"];
    apis::sync_user_tags(user_names).await;

    let discord_token: String = apis::get_discord_token();
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
