use crate::apis;
use crate::chatbot;
use anyhow::Context;
use rand::seq::SliceRandom;
use regex::Regex;
use serenity::{
    async_trait,
    model::{channel::Message as DiscordMessage, gateway::Ready, user::CurrentUser},
    prelude::{Context as DiscordContext, EventHandler},
};

/// How long a thread of replies can be
const MAX_TICK_COUNT: usize = 3;

/// A list of messages to reply with while waiting for AI
static REPLY_MESSAGES: &[&str] = &[
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

async fn get_bot_user(ctx: &DiscordContext) -> anyhow::Result<Option<CurrentUser>> {
    (ctx.http.get_current_user().await).map_or_else(
        |_| Err(anyhow::anyhow!("Failed to get bot user")),
        |user| Ok(Some(user)),
    )
}

async fn should_process_message(
    msg: &DiscordMessage,
    is_debug: bool,
    ctx: &DiscordContext,
) -> anyhow::Result<bool> {
    if is_debug && !msg.content.starts_with('!') {
        return Ok(false);
    }
    if !is_debug && msg.content.starts_with('!') {
        return Ok(false);
    }

    if !is_debug {
        let mentions_me = msg.mentions_me(&ctx.http).await?;
        if !mentions_me {
            return Ok(false);
        }
    }

    Ok(true)
}

fn clean_user_text(msg: &DiscordMessage, is_debug: bool) -> String {
    let regex = Regex::new(r"(?m)<[@#]&?\d+>").unwrap();
    let mut user_text = msg.content.replace('\n', " ");
    user_text = regex.replace_all(&user_text, "").trim().to_string();
    if is_debug {
        user_text = user_text[1..].trim().to_string();
    }
    user_text
}

async fn get_message_history(
    msg: &DiscordMessage,
    bot_user: &CurrentUser,
    ctx: &DiscordContext,
) -> anyhow::Result<Option<String>> {
    let mut message_history_text = String::new();
    if let Some(message_reference) = &msg.message_reference {
        let replied_to = msg
            .channel_id
            .message(&ctx.http, message_reference.message_id.unwrap())
            .await?;

        let tick_count = replied_to.content.chars().filter(|&c| c == '☑').count();
        if replied_to.author.id != bot_user.id
            || !replied_to.content.contains('✅')
            || tick_count > MAX_TICK_COUNT
        {
            return Ok(None);
        }
        message_history_text = replied_to.content.replace("✅ ", "☑️ ").trim().to_string() + "\n";
    }

    Ok(Some(message_history_text))
}

async fn process_and_reply(
    user_text: String,
    message_history_text: String,
    msg: &DiscordMessage,
    ctx: &DiscordContext,
) -> anyhow::Result<()> {
    // Collect user's ID and name
    let user_id = msg.author.id.to_string();
    let user_name = msg.author.name.clone();
    let user_name_cleaned = apis::user_name_from_id(&user_id, &user_name)
        .await
        .context(format!("Failed to get user name from id: {user_id}"))?;
    println!(
        "Message from {} ({}): {}",
        user_name_cleaned, user_id, msg.content
    );

    // Choose a random reply message
    let reply_text = REPLY_MESSAGES
        .choose(&mut rand::thread_rng())
        .context("Failed to choose reply message")?;

    // Send a reply message to the user
    let bot_message = msg
        .reply(&ctx.http, format!("{message_history_text}⌛ {reply_text}"))
        .await
        .context("Failed to send message")?;

    let ctx_clone = ctx.clone();

    // Spawn a new thread to process the message further
    tokio::spawn(async move {
        chatbot::process_chat(
            user_name_cleaned,
            user_text,
            ctx_clone,
            bot_message,
            message_history_text,
        )
        .await;
    });

    Ok(())
}

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: DiscordContext, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn message(&self, ctx: DiscordContext, msg: DiscordMessage) {
        if msg.author.bot {
            return;
        }

        let is_debug = cfg!(debug_assertions);
        let Some(bot_user) = get_bot_user(&ctx).await.unwrap() else {
            return;
        };

        if !should_process_message(&msg, is_debug, &ctx).await.unwrap() {
            return;
        }

        let user_text = clean_user_text(&msg, is_debug);
        if user_text.is_empty() {
            return;
        }

        let Some(message_history_text) = get_message_history(&msg, &bot_user, &ctx).await.unwrap()
        else {
            return;
        };

        process_and_reply(user_text, message_history_text, &msg, &ctx)
            .await
            .expect("Failed to process and reply");
    }
}
