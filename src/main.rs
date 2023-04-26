use std::env;

use serenity::{
    async_trait,
    model::{channel::Message as DiscordMessage, gateway::Ready},
    prelude::*,
};

use async_openai::{
    types::{CreateChatCompletionResponse, ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
    Client as OpenAiClient,
};

struct OpenAiApi;
impl TypeMapKey for OpenAiApi {
    type Value = OpenAiClient;
}

use rand::seq::SliceRandom;
use regex::Regex;

struct Handler;

async fn process_chat(
    openai_client: &OpenAiClient,
    user_name: String,           // The users name
    user_id: String,             // The users id
    user_text: String,           // Users text to bot
    ctx: Context,                // The discord context
    mut bot_message: DiscordMessage, // The reply to the user
    bot_message_history: String, // The message history
    reply_text: String,          // The text used in the reply while processing
) {
    // TODO Don't reply to non media queries, compare user_text with the ai model
    // TODO add user_message_history before the user_text else it is only searching against the newest message
    let request = CreateChatCompletionRequestArgs::default()
        .max_tokens(4u16)
        .model("gpt-4")
        .n(3u8)
        .messages([
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content("You determine if a users message is irrelevant to you, is it related to movies, series, asking for recommendations, changing resolution, adding or removing media, checking disk space, viewing users memories etc? You reply with a single word answer, yes or no.")
                .build().unwrap(),
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(format!("{user_text}\nDo not respond to the above message, is the above text irrelevant? Reply with a single word answer, only say yes if certain"))
                .build().unwrap(),
        ])
        .build().unwrap();

    let mut tries = 0;
    let response = loop {
        let response = openai_client.chat().create(request.clone()).await;
        if let Ok(response) = response {
            break Ok(response);
        } else {
            tries += 1;
            if tries >= 3 {
                break response;
            }
        }
    };

    // TODO log the openai call and response

    // Return from errors
    if let Err(error) = response {
        println!("Error: {:?}", error);
        return;
    }
    let response: CreateChatCompletionResponse = response.unwrap();

    // Check each response choice for a yes
    let mut is_valid = false;
    for choice in response.choices {
        if !choice.message.content.to_lowercase().contains("yes") {
            is_valid = true;
        }
    }
    if !is_valid {
        // Edit the message to let the user know the message is not valid
        bot_message
            .edit(&ctx.http, |msg| {
                msg.content(format!("{bot_message_history}❌ Hi, I'm a media bot. I can help you with media related questions. What would you like to know or achieve?"))
            })
            .await
            .unwrap();
        return;
    }
}

#[async_trait]
impl EventHandler for Handler {
    // When the bot is ready
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    // When message is received
    async fn message(&self, ctx: Context, msg: DiscordMessage) {
        // Don't reply to bots
        if msg.author.bot {
            return;
        }

        // If in production, don't reply to messages that don't mention the bot
        // In debug, don't reply to messages that don't start with "!"
        if cfg!(debug_assertions) {
            if !msg.content.starts_with("!") {
                return;
            }
        } else {
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
        }

        // TODO If message is a reply to the bot, create a message history
        // let message_history: Vec<HashMap<&str, String>> = Vec::new();

        // Collect users id and name
        let user_id = msg.author.id.to_string();
        let user_name = msg.author.name.clone();
        println!("Message from {} ({}): {}", user_name, user_id, msg.content);

        // Remove new lines, mentions and trim whitespace
        let regex = Regex::new(r"(?m)<[@#]&?\d+>").unwrap();
        let mut user_text = msg.content.replace("\n", " ").to_string();
        user_text = regex.replace_all(&user_text, "").trim().to_string();
        if cfg!(debug_assertions) {
            // Remove the first char "!" in debug
            user_text = user_text[1..].trim().to_string();
        }

        if user_text == "" {
            return;
        }

        let mut bot_message_history = String::new();
        // for msg in message_history {
        //     bot_start_message.push_str(&format!("{}\n", msg["content"]));
        // }
        // Add the users message to the message history
        bot_message_history.push_str(&format!("💬 {}\n", user_text));

        let reply_messages = vec![
            "Hey there! Super excited to process your message, give me just a moment... 🎬",
            "Oh, a message! Can't wait to dive into this one - I'm on it... 🎥",
            "Hey, awesome! A new message to explore! Let me work my media magic... 📺",
            "Woo-hoo! A fresh message to check out! Let me put my CineMatic touch on it... 🍿",
            "Yay, another message! Time to unleash my media passion, be right back... 📼",
            "Hey, a message! I'm so excited to process this one, just a moment... 🎞",
            "Aha! A message has arrived! Let me roll out the red carpet for it... 🎞️",
            "Ooh, a new message to dissect! Allow me to unleash my inner film buff... 🎦",
            "Lights, camera, action! Time to process your message with a cinematic twist... 📽️",
            "Hooray, a message to dig into! Let's make this a blockbuster experience... 🌟",
            "Greetings! Your message has caught my eye, let me give it the star treatment... 🎟️",
            "Popcorn's ready! Let me take a closer look at your message like a true film fanatic... 🍿",
            "Woohoo! A message to analyze! Let me work on it while humming my favorite movie tunes... 🎶",
            "A new message to dive into! Let me put on my director's hat and get to work... 🎩",
            "And... action! Time to process your message with my media expertise... 📹",
            "Sending your message to the cutting room! Let me work on it like a skilled film editor... 🎞️",
            "A message has entered the scene! Let me put my media prowess to work on it... 🎭",
            "Your message is the star of the show! Let me process it with the passion of a true cinephile... 🌟",
            "Curtain up! Your message takes center stage, and I'm ready to give it a standing ovation... 🎦",
        ];
        // Choose a random reply message
        let reply_text = reply_messages
            .choose(&mut rand::thread_rng())
            .expect("Failed to choose reply message")
            .to_string();
        // Send a reply message to the user
        let bot_message = msg
            .reply(
                &ctx.http,
                format!("{}⌛ 1/3 {}", bot_message_history, reply_text),
            )
            .await
            .expect("Failed to send message");

        // Get the openai client from the context
        let data = (&ctx.data).read().await;
        let openai_client = data.get::<OpenAiApi>().unwrap().clone();
        let ctx_clone = (&ctx).clone();

        // Spawn a new thread to process the message
        tokio::spawn(async move {
            process_chat(
                &openai_client,
                user_name,
                user_id,
                user_text,
                ctx_clone,
                bot_message,
                bot_message_history,
                reply_text,
            )
            .await;
        });
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your openai token in the environment.
    let openai_api_key: String =
        env::var("OPENAI_API_KEY").expect("Expected a openai token in the environment");
    let openai_client = OpenAiClient::new().with_api_key(openai_api_key);

    // Configure the client with your Discord bot token in the environment.
    let discord_token: String =
        env::var("DISCORD_TOKEN").expect("Expected a discord token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents: GatewayIntents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot
    let mut client: Client = Client::builder(&discord_token, intents)
        .event_handler(Handler)
        .type_map_insert::<OpenAiApi>(openai_client)
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
