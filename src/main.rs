use std::collections::HashMap;
use std::env;

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    // When the bot is ready
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    // When message is received
    async fn message(&self, ctx: Context, msg: Message) {
        // Don't reply to bots
        if msg.author.bot {
            return;
        }

        // Don't reply to messages that don't mention the bot
        match msg.mentions_me(&ctx.http).await {
            Ok(is_mentioned) => {
                if !is_mentioned {
                    return;
                }
            }
            Err(error) => {
                println!("Error checking mentions: {:?}", error);
                return;
            }
        }

        // If message is a reply to the bot, create a message history
        let mut message_history: Vec<HashMap<&str, String>> = Vec::new();
        // Assuming message_reference and replied_to_message are provided by some external API
        if msg.referenced_message {
            if replied_to_message.author_id == self_user_id {
                // Check if the message is completed
                if !replied_to_message.content.contains("✅") {
                    return;
                }
                // Split message by lines
                let content: Vec<&str> = replied_to_message.content.split("\n").collect();
                for msg in content {
                    if msg.starts_with("✅") {
                        message_history.push(HashMap::from([
                            ("role", "assistant".to_string()),
                            ("content", msg.replace("✅ ", "☑️ ").trim().to_string()),
                        ]));
                    } else if msg.starts_with("☑️") {
                        message_history.push(HashMap::from([
                            ("role", "assistant".to_string()),
                            ("content", msg.trim().to_string()),
                        ]));
                    } else if msg.starts_with("💬") {
                        message_history.push(HashMap::from([
                            ("role", "user".to_string()),
                            ("content", msg.trim().to_string()),
                        ]));
                    }
                }
            }
        }

        if msg.content.starts_with("!") {
            // Get message text
            let msg_text: &str = &msg.content[1..];
            let reply_txt: String = format!("Pong! {}", msg_text);
            // Reply to the msg
            let reply_msg = msg.reply(&ctx.http, reply_txt).await;
            if let Err(why) = reply_msg {
                println!("Error sending message: {:?}", why);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token: String = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents: GatewayIntents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot
    let mut client: Client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
