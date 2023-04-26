use std::env;

use serenity::{
    async_trait,
    model::{channel::Message as DiscordMessage, gateway::Ready},
    prelude::{
        Client as DiscordClient, Context as DiscordContext, EventHandler, GatewayIntents,
        TypeMapKey,
    },
};

use async_openai::{
    types::{ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs, Role},
    Client as OpenAiClient,
};

struct OpenAiApi;
impl TypeMapKey for OpenAiApi {
    type Value = OpenAiClient;
}

use rand::seq::SliceRandom;
use regex::Regex;

struct Handler;

mod examples;
mod relevance;

async fn process_chat(
    openai_client: &OpenAiClient,
    user_name: String,                                  // The users name
    user_id: String,                                    // The users id
    user_text: String,                                  // Users text to bot
    ctx: DiscordContext,                                // The discord context
    mut bot_message: DiscordMessage,                    // The reply to the user
    message_history: Vec<ChatCompletionRequestMessage>, // The message history
    message_history_text: String,                       // The message history text
    reply_text: String, // The text used in the reply while processing
) {
    // Get messages from user, add their text plus a new line
    let mut user_text_total = String::new();
    for message in &message_history {
        if message.role == Role::User {
            user_text_total.push_str(&format!("{}\n", &message.content));
        }
    }
    // Add the users latest message
    user_text_total.push_str(&user_text);
    user_text_total = user_text_total
        .replace("\n", ", ")
        .replace("💬", "")
        .trim()
        .to_string();

    // Don't reply to non media queries, compare user_text_total with the ai model
    if !relevance::check_relevance(openai_client, user_text_total.clone()).await {
        // Edit the message to let the user know the message is not valid
        bot_message
            .edit(&ctx.http, |msg: &mut serenity::builder::EditMessage| {
                msg.content(format!("{message_history_text}❌ Hi, I'm a media bot. I can help you with media related questions. What would you like to know or achieve?"))
            })
            .await
            .unwrap();
        return;
    }

    // Edit the bot_message to let the user know the message is valid and it is progressing
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!("{message_history_text}⌛ 2/3 {reply_text}"))
        })
        .await
        .unwrap();

    // Get relevant examples
    let relevant_examples = examples::get_examples(openai_client, user_text_total);

    // Edit the bot_message to let the user know it is progressing
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!("{message_history_text}⌛ 3/3 {reply_text}"))
        })
        .await
        .unwrap();

    // Get current messages
    let mut current_message: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content(format!("Hi my name is {user_name}"))
            .build()
            .unwrap(),
        ChatCompletionRequestMessageArgs::default()
            .role(Role::Assistant)
            .content(format!("Hi, how can I help you?"))
            .build()
            .unwrap(),
    ];
    // Merge in message_history
    for message in &message_history {
        current_message.push(message.clone());
    }
    // Add users message
    current_message.push(
        ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content(user_text)
            .build()
            .unwrap(),
    );

    println!("current_message: {:?}", current_message);

    // # Run chat completion
    // await runChatCompletion(
    //     botsMessage,
    //     botsStartMessage,
    //     usersName,
    //     usersId,
    //     currentMessage,
    //     relevantExamples,
    //     0,
    // )
}

#[async_trait]
impl EventHandler for Handler {
    // When the bot is ready
    async fn ready(&self, _: DiscordContext, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    // When message is received
    async fn message(&self, ctx: DiscordContext, msg: DiscordMessage) {
        // Don't reply to bots or self
        if msg.author.bot {
            return;
        }
        // Get the bots user
        let bot_user = ctx
            .http
            .get_current_user()
            .await
            .expect("Failed to get bot user");

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

        // If message is a reply to the bot, create a message history
        let mut message_history: Vec<ChatCompletionRequestMessage> = Vec::new();
        let mut valid_reply = false;
        if let Some(message_reference) = &msg.message_reference {
            // Get the message replied to
            let replied_to = match msg
                .channel_id
                .message(&ctx.http, message_reference.message_id.unwrap())
                .await
            {
                Ok(replied_to) => replied_to,
                Err(error) => {
                    println!("Error getting replied to message: {:?}", error);
                    return;
                }
            };
            if replied_to.author.id == bot_user.id {
                // See if the message is completed
                if !replied_to.content.contains("✅") {
                    return;
                }
                // See if the message is passed the thread limit
                let mut count = 0;
                for c in replied_to.content.chars() {
                    if c == '☑' {
                        count += 1;
                    }
                }
                if count > 3 {
                    return;
                }
                valid_reply = true;
                // Split message by lines
                let content = replied_to.content.split("\n");
                for msg in content {
                    // If the line is a reply to the bot, add it to the message history
                    if msg.starts_with("✅") {
                        message_history.push(
                            ChatCompletionRequestMessageArgs::default()
                                .role(Role::Assistant)
                                .content(msg.replace("✅ ", "☑️ ").trim())
                                .build()
                                .unwrap(),
                        );
                    } else if msg.starts_with("☑️") {
                        message_history.push(
                            ChatCompletionRequestMessageArgs::default()
                                .role(Role::Assistant)
                                .content(msg.trim())
                                .build()
                                .unwrap(),
                        );
                    // If the line is a reply to the user, add it to the message history
                    } else if msg.starts_with("💬") {
                        message_history.push(
                            ChatCompletionRequestMessageArgs::default()
                                .role(Role::User)
                                .content(msg.trim())
                                .build()
                                .unwrap(),
                        );
                    }
                }
            }
        } else {
            valid_reply = true;
        }
        // If reply was not valid end
        if !valid_reply {
            return;
        }

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

        let mut message_history_text = String::new();
        for msg in &message_history {
            message_history_text.push_str(&format!("{}\n", msg.content));
        }
        // Add the users message to the message history
        message_history_text.push_str(&format!("💬 {user_text}\n"));

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
                format!("{message_history_text}⌛ 1/3 {reply_text}"),
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
                message_history,
                message_history_text,
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
    let mut client: DiscordClient = DiscordClient::builder(&discord_token, intents)
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
