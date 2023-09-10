use crate::apis;
use crate::plugins;
use anyhow::{anyhow, Context};
use async_openai::types::{
    ChatCompletionFunctions, ChatCompletionFunctionsArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role,
};
use chrono::Local;
use futures::Future;
use regex::Regex;
use serde_json::json;
use serenity::{
    async_trait,
    model::{channel::Message as DiscordMessage, gateway::Ready, user::CurrentUser},
    prelude::{Context as DiscordContext, EventHandler},
};
use std::{collections::HashMap, pin::Pin};

/// How long a thread of replies can be
const MAX_THREAD_LIMIT: usize = 3;

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
            || tick_count > MAX_THREAD_LIMIT
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
    #[allow(clippy::cast_possible_truncation)]
    let index = (msg.id.0 as usize) % REPLY_MESSAGES.len();
    let reply_text = REPLY_MESSAGES[index];

    // Send a reply message to the user
    let bot_message = msg
        .reply(&ctx.http, format!("{message_history_text}⌛ {reply_text}"))
        .await
        .context("Failed to send message")?;

    let ctx_clone = ctx.clone();

    // Spawn a new thread to process the message further
    tokio::spawn(async move {
        process_chat(
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

#[derive(Debug)]
pub struct Func {
    name: String,
    description: String,
    parameters: Vec<Param>,
    call_func: FuncType,
}

impl Func {
    pub fn new(name: &str, description: &str, parameters: Vec<Param>, call_func: FuncType) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            parameters,
            call_func,
        }
    }
}

type FuncType =
    fn(&HashMap<String, String>) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>;

#[derive(Debug, Clone)]
pub struct Param {
    name: String,
    description: String,
    required: bool,
    enum_values: Option<Vec<String>>,
}
impl Param {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            required: true,
            enum_values: None,
        }
    }

    pub fn with_enum_values(mut self, enum_values: &[&str]) -> Self {
        self.enum_values = Some(enum_values.iter().map(|&val| val.to_owned()).collect());
        self
    }

    fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("description".to_owned(), json!(self.description));
        map.insert("type".to_owned(), json!("string"));
        if let Some(ref enum_values) = self.enum_values {
            map.insert("enum".to_owned(), json!(enum_values));
        }
        json!(map)
    }
}

/// Get available functions data
fn get_functions() -> Vec<Func> {
    let mut functions = Vec::new();
    functions.extend(plugins::websearch::get_functions());
    functions.extend(plugins::media::get_functions());

    functions
}

/// Run function
async fn run_function(
    name: String,
    args: serde_json::Value,
    user_name: &str,
) -> anyhow::Result<String> {
    let functions = get_functions();

    for func in functions {
        if func.name == name {
            let mut args_map = HashMap::new();
            args_map.insert("user_name".to_string(), user_name.to_string());
            for (key, value) in args.as_object().unwrap() {
                args_map.insert(key.clone(), value.as_str().unwrap().to_string());
            }
            println!("Running function {name} with args {args_map:?}");
            let response = (func.call_func)(&args_map).await;
            println!("Function {name} response: {response:?}",);
            return response;
        }
    }

    Err(anyhow!("Function not found"))
}

/// Wraps any async function into a pinned Boxed Future with the correct return type
pub fn box_future<F>(fut: F) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>
where
    F: Future<Output = anyhow::Result<String>> + Send + 'static,
{
    Box::pin(fut)
}

fn func_to_chat_completion(func: &Func) -> ChatCompletionFunctions {
    let properties: serde_json::Map<String, _> = func
        .parameters
        .iter()
        .map(|param| (param.name.clone(), param.to_json()))
        .collect();

    let required: Vec<_> = func
        .parameters
        .iter()
        .filter(|&param| param.required)
        .map(|param| param.name.clone())
        .collect();

    ChatCompletionFunctionsArgs::default()
        .name(&func.name)
        .description(&func.description)
        .parameters(json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }))
        .build()
        .unwrap()
}

/// Process the chat message from the user
#[allow(clippy::too_many_lines)]
async fn process_chat(
    user_name: String,
    users_text: String,
    ctx: DiscordContext,
    mut bot_message: DiscordMessage,
    message_history_text: String,
) {
    // Get current date and time in DD/MM/YYYY and HH:MM:SS format
    let date = Local::now().format("%d/%m/%Y").to_string();
    let time = Local::now().format("%H:%M").to_string();

    let mut chat_query: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestMessageArgs::default()
            .role(Role::System)
            .content(format!("You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media\nYou always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need you reply with the truthful answer of unknown, responses should all be on one line (with comma separation) and compact language, use emojis to express emotion to the user. The current date is {date}, the current time is {time}"))
            .build()
            .unwrap(),
    ];
    // Add message history minus the most recent line
    let mut just_history = if message_history_text.is_empty() {
        String::new()
    } else {
        message_history_text[..message_history_text.len() - 1].to_string()
    };
    // If it contains a \n then it has history
    if just_history.contains('\n') {
        // Remove the last line
        just_history =
            just_history[..just_history.rfind('\n').unwrap_or(just_history.len())].to_string();
        chat_query.push(
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content(format!(
                    "Message history:\n{}",
                    just_history
                        .replace("💬 ", "User: ")
                        .replace("☑️ ", "CineMatic: ")
                ))
                .build()
                .unwrap(),
        );
    }
    // Add users message
    chat_query.push(
        ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content(users_text.clone())
            .build()
            .unwrap(),
    );

    // The initial messages to send to the API
    let mut chat_query: Vec<ChatCompletionRequestMessage> = chat_query;

    // Rerun the chat completition until either no function calls left, or max iterations reached
    let mut extra_history_text: String = String::new();
    let mut final_response: String = String::new();
    let mut counter = 0;
    while counter < 10 {
        let chat_completitions: Vec<ChatCompletionFunctions> = get_functions()
            .iter()
            .map(func_to_chat_completion)
            .collect();

        let request = CreateChatCompletionRequestArgs::default()
            .max_tokens(512u16)
            .model("gpt-4")
            .messages(chat_query.clone())
            .functions(chat_completitions)
            .function_call("auto")
            .build()
            .unwrap();

        let response_message = async_openai::Client::new()
            .chat()
            .create(request)
            .await
            .unwrap()
            .choices
            .get(0)
            .unwrap()
            .message
            .clone();

        if let Some(function_call) = response_message.function_call {
            let function_name = function_call.name;
            let function_args: serde_json::Value = function_call.arguments.parse().unwrap();

            // Edit the discord message with function call in progress
            let ctx_c = ctx.clone();
            let mut bot_message_c = bot_message.clone();
            let new_message = format!(
                "{message_history_text}{extra_history_text}⌛ Running function {function_name} with arguments {function_args}"
            );
            tokio::spawn(async move {
                bot_message_c
                    .edit(&ctx_c.http, |msg| msg.content(new_message))
                    .await
                    .unwrap();
            });

            let function_response =
                run_function(function_name.clone(), function_args, &user_name).await;
            // Get function response as string if either ok or error
            let function_response_message =
                function_response.map_or_else(|error| error.to_string(), |response| response);
            // Truncate the function response to 100 characters
            let function_response_short = if function_response_message.len() > 150 {
                let trimmed_message = function_response_message
                    .chars()
                    .take(150)
                    .collect::<String>();
                format!("{trimmed_message}...")
            } else {
                function_response_message.clone()
            };

            // Edit the discord message with function call results
            extra_history_text.push_str(
                format!("🎬 Ran function {function_name} {function_response_short}\n",).as_str(),
            );
            let ctx_c = ctx.clone();
            let mut bot_message_c = bot_message.clone();
            let new_message = format!("{message_history_text}{extra_history_text}");
            tokio::spawn(async move {
                bot_message_c
                    .edit(&ctx_c.http, |msg| msg.content(new_message))
                    .await
                    .unwrap();
            });

            chat_query.push(
                ChatCompletionRequestMessageArgs::default()
                    .role(Role::Function)
                    .name(function_name)
                    .content(function_response_message)
                    .build()
                    .unwrap(),
            );
            counter += 1;
        } else {
            final_response = response_message.content.unwrap();
            break;
        }
    }

    // Edit the discord message finalised
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!(
                "{message_history_text}{extra_history_text}✅ {final_response}"
            ))
        })
        .await
        .unwrap();
}
