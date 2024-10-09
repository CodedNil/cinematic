#![allow(
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::struct_field_names,
    clippy::struct_excessive_bools
)]

use serenity::prelude::{Client as DiscordClient, GatewayIntents};

mod apis;
mod discordbot;
mod plugins;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let mut client: DiscordClient = DiscordClient::builder(
        apis::get_env_variable("DISCORD_TOKEN"),
        GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT,
    )
    .event_handler(discordbot::DiscordHandler)
    .await
    .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
