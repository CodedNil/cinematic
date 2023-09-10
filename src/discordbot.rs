use crate::{apis, plugins};
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
/// How many function calls can be made
const MAX_FUNCTION_CALLS: usize = 10;

pub struct DiscordHandler;

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, _: DiscordContext, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn message(&self, ctx: DiscordContext, msg: DiscordMessage) {
        if msg.author.bot
            || get_bot_user(&ctx).await.unwrap().is_none()
            || !should_process_message(&msg, cfg!(debug_assertions), &ctx)
                .await
                .unwrap()
        {
            return;
        }

        let user_text = clean_user_text(&msg, cfg!(debug_assertions));
        if user_text.is_empty() {
            return;
        }

        process_and_reply(
            user_text,
            get_message_history(&msg, &get_bot_user(&ctx).await.unwrap().unwrap(), &ctx)
                .await
                .unwrap()
                .unwrap(),
            &msg,
            &ctx,
        )
        .await
        .expect("Failed to process and reply");
    }
}

async fn get_bot_user(ctx: &DiscordContext) -> anyhow::Result<Option<CurrentUser>> {
    ctx.http
        .get_current_user()
        .await
        .map(Some)
        .context("Failed to get bot user")
}

async fn should_process_message(
    msg: &DiscordMessage,
    is_debug: bool,
    ctx: &DiscordContext,
) -> anyhow::Result<bool> {
    let debug_valid = is_debug && msg.content.starts_with('!');
    let release_valid =
        !is_debug && !msg.content.starts_with('!') && msg.mentions_me(&ctx.http).await?;

    Ok(debug_valid || release_valid)
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

        Ok(Some(
            replied_to.content.replace("✅ ", "☑️ ").trim().to_string() + "\n",
        ))
    } else {
        Ok(None)
    }
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
    let reply_messages: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string("reply_messages.json").context("Unable to read file")?,
    )
    .context("Unable to parse JSON data")?;
    #[allow(clippy::cast_possible_truncation)]
    let index = (msg.id.0 as usize) % reply_messages.len();
    let reply_text = &reply_messages[index];

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
        .await
        .unwrap();
    });

    Ok(())
}

/// Process the chat message from the user
async fn process_chat(
    user_name: String,
    users_text: String,
    ctx: DiscordContext,
    mut bot_message: DiscordMessage,
    message_history_text: String,
) -> anyhow::Result<()> {
    // Get current date and time in DD/MM/YYYY and HH:MM:SS format
    let date = Local::now().format("%d/%m/%Y").to_string();
    let time = Local::now().format("%H:%M").to_string();

    let mut chat_query: Vec<ChatCompletionRequestMessage> = Vec::new();
    chat_query.push(create_chat_completion_request_message(
        Role::System,
        "System Message",
        &format!("You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media\nYou always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need you reply with the truthful answer of unknown, responses should all be on one line (with comma separation) and compact language, use emojis to express emotion to the user. The current date is {date}, the current time is {time}"),
    ));

    // If it contains a \n then it has history
    if let Some(last_newline) = message_history_text.rfind('\n') {
        chat_query.push(create_chat_completion_request_message(
            Role::System,
            "Message History",
            &format!(
                "Message history:\n{}",
                message_history_text[..last_newline]
                    .replace("💬 ", "User: ")
                    .replace("☑️ ", "CineMatic: ")
            ),
        ));
    }

    // Add users message
    chat_query.push(create_chat_completion_request_message(
        Role::User,
        &user_name,
        &users_text,
    ));

    // Rerun the chat completition until either no function calls left, or max iterations reached
    let mut extra_history_text: String = String::new();
    let mut final_response: String = String::new();

    for _ in 0..MAX_FUNCTION_CALLS {
        let chat_completitions = get_functions()
            .iter()
            .map(func_to_chat_completion)
            .collect::<Vec<_>>();

        let response_message = async_openai::Client::new()
            .chat()
            .create(
                CreateChatCompletionRequestArgs::default()
                    .max_tokens(512u16)
                    .model("gpt-4")
                    .messages(chat_query.clone())
                    .functions(chat_completitions)
                    .function_call("auto")
                    .build()?,
            )
            .await?
            .choices
            .get(0)
            .unwrap()
            .message
            .clone();

        if let Some(function_call) = response_message.function_call {
            let function_name = function_call.name;
            let function_args: serde_json::Value = function_call.arguments.parse()?;

            // Edit the discord message with function call in progress
            edit_bot_message(
                &ctx,
                &mut bot_message,
                format!("{message_history_text}{extra_history_text}⌛ Running function {function_name} with arguments {function_args}"),
            )
            .await?;

            // Get function response as string if either ok or error
            let function_response =
                run_function(function_name.clone(), function_args, &user_name).await;
            let function_response_message = function_response.unwrap_or_else(|e| e.to_string());

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
            edit_bot_message(
                &ctx,
                &mut bot_message,
                format!("{message_history_text}{extra_history_text}"),
            )
            .await?;

            chat_query.push(create_chat_completion_request_message(
                Role::Function,
                &function_name,
                &function_response_message,
            ));
        } else {
            final_response = response_message.content.unwrap();
            break;
        }
    }

    // Edit the discord message finalised
    edit_bot_message(
        &ctx,
        &mut bot_message,
        format!("{message_history_text}{extra_history_text}✅ {final_response}"),
    )
    .await?;

    Ok(())
}

// Helper function for editing bot messages
async fn edit_bot_message(
    ctx: &DiscordContext,
    message: &mut DiscordMessage,
    content: String,
) -> anyhow::Result<()> {
    message.edit(&ctx.http, |msg| msg.content(content)).await?;
    Ok(())
}

// Helper function for creating ChatCompletionRequestMessage
fn create_chat_completion_request_message(
    role: Role,
    name: &str,
    content: &str,
) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessageArgs::default()
        .role(role)
        .name(name)
        .content(content)
        .build()
        .unwrap()
}

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
