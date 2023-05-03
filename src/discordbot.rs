use serenity::{
    async_trait,
    model::{channel::Message as DiscordMessage, gateway::Ready},
    prelude::{Context as DiscordContext, EventHandler},
};

use rand::seq::SliceRandom;
use regex::Regex;

pub struct Handler;

use crate::chatbot;

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
            if msg.content.starts_with("!") {
                return;
            }
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

        // Remove new lines, mentions and trim whitespace, reject empty messages
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

        // If message is a reply to the bot, gather message history
        let mut message_history_text = String::new();
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
                message_history_text = replied_to
                    .content
                    .clone()
                    .replace("✅ ", "☑️ ")
                    .trim()
                    .to_string()
                    + "\n";
            }
        } else {
            valid_reply = true;
        }
        // If reply was not valid end
        if !valid_reply {
            return;
        }
        // Add the users message to the message history text
        message_history_text.push_str(&format!("💬 {user_text}\n"));

        // Collect users id and name
        let user_id = msg.author.id.to_string();
        let user_name = msg.author.name.clone();
        println!("Message from {} ({}): {}", user_name, user_id, msg.content);

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
                format!("{message_history_text}⌛ 1/2 {reply_text}"),
            )
            .await
            .expect("Failed to send message");

        let ctx_clone = (&ctx).clone();

        // Spawn a new thread to process the message
        tokio::spawn(async move {
            chatbot::process_chat(
                user_name,
                user_id,
                user_text,
                ctx_clone,
                bot_message,
                message_history_text,
                reply_text,
            )
            .await;
        });
    }
}
