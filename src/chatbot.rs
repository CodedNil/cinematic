use crate::{apis, plugins};
use async_openai::types::{
    ChatCompletionFunctionsArgs, ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
    CreateChatCompletionRequestArgs, Role,
};
use chrono::Local;
use serde_json::json;
use serenity::{model::channel::Message as DiscordMessage, prelude::Context as DiscordContext};
use std::error::Error;

/// Get available functions data
#[allow(clippy::too_many_lines)]
fn get_functions() -> Vec<async_openai::types::ChatCompletionFunctions> {
    vec![
        ChatCompletionFunctionsArgs::default()
            .name("web_search")
            .description("Search web for query")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A query for information to be answered",
                    },
                },
                "required": ["query"],
            }))
            .build().unwrap(),
        ChatCompletionFunctionsArgs::default()
            .name("media_lookup")
            .description("Search the media server for query information about a piece of media")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "description": "The format of the media to be searched for",
                        "enum": ["movie", "series"],
                    },
                    "query": {
                        "type": "string",
                        "description": "A query for information to be answered, query should be phrased as a question, for example \"Available on the server?\" \"Is series Watchmen available on the server in the Ultimate Cut?\" \"What is Cats movie tmdbId?\" \"Who added series Game of Thrones to the server?\", if multiple results are returned, ask user for clarification",
                    }
                },
                "required": ["format", "query"],
            }))
            .build().unwrap(),
            ChatCompletionFunctionsArgs::default()
                .name("media_add")
                .description("Adds media to the server, perform a lookup first to get the tmdbId or tvdbId")
                .parameters(json!({
                    "type": "object",
                    "properties": {
                        "format": {
                            "type": "string",
                            "description": "The format of the media to be searched for",
                            "enum": ["movie", "series"],
                        },
                        "db_id": {
                            "type": "string",
                            "description": "The tmdb or tvdb id of the media item",
                        },
                        "quality": {
                            "type": "string",
                            "description": "The quality to set the media to, default to 1080p if not specified",
                            "enum": ["SD", "720p", "1080p", "2160p", "720p/1080p", "Any"],
                        },
                    },
                    "required": ["format", "id", "quality"],
                }))
                .build().unwrap(),
        ChatCompletionFunctionsArgs::default()
            .name("media_setres")
            .description("Change the targeted resolution of a piece of media on the server, perform a lookup first to get the id")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "description": "The format of the media to be searched for",
                        "enum": ["movie", "series"],
                    },
                    "id": {
                        "type": "string",
                        "description": "The id of the media item",
                    },
                    "quality": {
                        "type": "string",
                        "description": "The quality to set the media to",
                        "enum": ["SD", "720p", "1080p", "2160p", "720p/1080p", "Any"],
                    },
                },
                "required": ["format", "id", "quality"],
            }))
            .build().unwrap(),
        ChatCompletionFunctionsArgs::default()
            .name("media_remove")
            .description("Removes media from users requests, media items remain on the server if another user has requested also, perform a lookup first to get the id")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "description": "The format of the media to be searched for",
                        "enum": ["movie", "series"],
                    },
                    "id": {
                        "type": "string",
                        "description": "The id of the media item",
                    },
                },
                "required": ["format", "id"],
            }))
            .build().unwrap(),
            ChatCompletionFunctionsArgs::default()
                .name("media_wanted")
                .description("Returns a list of series that user has requested, user can be self for the user that spoke, or none to get a list of series that noone has requested, if user asks have they requested x or what they have requested etc")
                .parameters(json!({
                    "type": "object",
                    "properties": {
                        "format": {
                            "type": "string",
                            "description": "The format of the media to be searched for",
                            "enum": ["movie", "series"],
                        },
                        "user": {
                            "type": "string",
                            "description": "The id of the media item",
                            "enum": ["self", "none"],
                        },
                    },
                    "required": ["format", "user"],
                }))
                .build().unwrap(),
    ]
}

/// Run function
async fn run_function(
    name: String,
    args: serde_json::Value,
    user_name: &str,
) -> Result<String, Box<dyn Error>> {
    match name.as_str() {
        "web_search" => {
            plugins::websearch::ai_search(args["query"].as_str().unwrap().to_string()).await
        }
        "media_lookup" => {
            plugins::media::lookup(
                match args["format"].as_str().unwrap() {
                    "series" => plugins::media::Format::Series,
                    _ => plugins::media::Format::Movie,
                },
                args["query"].as_str().unwrap().to_string(),
            )
            .await
        }
        "media_add" => {
            plugins::media::add(
                match args["format"].as_str().unwrap() {
                    "series" => plugins::media::Format::Series,
                    _ => plugins::media::Format::Movie,
                },
                args["db_id"].as_str().unwrap().to_string(),
                user_name,
                args["quality"].as_str().unwrap().to_string(),
            )
            .await
        }
        "media_setres" => {
            plugins::media::setres(
                match args["format"].as_str().unwrap() {
                    "series" => plugins::media::Format::Series,
                    _ => plugins::media::Format::Movie,
                },
                args["id"].as_str().unwrap().to_string(),
                args["quality"].as_str().unwrap().to_string(),
            )
            .await
        }
        "media_remove" => {
            plugins::media::remove(
                match args["format"].as_str().unwrap() {
                    "series" => plugins::media::Format::Series,
                    _ => plugins::media::Format::Movie,
                },
                args["id"].as_str().unwrap().to_string(),
                user_name,
            )
            .await
        }
        "media_wanted" => {
            plugins::media::wanted(
                match args["format"].as_str().unwrap() {
                    "series" => plugins::media::Format::Series,
                    _ => plugins::media::Format::Movie,
                },
                args["user"].as_str().unwrap().to_string(),
                user_name,
            )
            .await
        }
        _ => Err("Function not found".into()),
    }
}

/// Run the chat completition
#[allow(clippy::too_many_lines)]
pub async fn run_chat_completition(
    ctx: DiscordContext,             // The discord context
    mut bot_message: DiscordMessage, // The reply to the user
    message_history_text: String,    // The message history text
    users_text: String,              // The users text
    user_name: String,               // The user name
) {
    // Get current date and time in DD/MM/YYYY and HH:MM:SS format
    let date = Local::now().format("%d/%m/%Y").to_string();
    let time = Local::now().format("%H:%M").to_string();

    // The initial messages to send to the API
    let mut chat_query: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestMessageArgs::default()
            .role(Role::System)
            .content(format!("You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media\nYou always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need you reply with the truthful answer of unknown, responses should all be on one line (with comma separation) and compact language, use emojis to express emotion to the user. The current date is {date}, the current time is {time}"))
            .build()
            .unwrap(),
    ];
    // Add message history minus the most recent line
    let mut just_history = message_history_text[..message_history_text.len() - 1].to_string();
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
            .content(users_text)
            .build()
            .unwrap(),
    );

    // Rerun the chat completition until either no function calls left, or max iterations reached
    let mut extra_history_text: String = String::new();
    let mut final_response: String = String::new();
    let mut counter = 0;
    while counter < 10 {
        let request = CreateChatCompletionRequestArgs::default()
            .max_tokens(512u16)
            .model("gpt-4-0613")
            .messages(chat_query.clone())
            .functions(get_functions())
            .function_call("auto")
            .build()
            .unwrap();

        let response_message = apis::get_openai()
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

            // Edit the discord message with function call results
            extra_history_text.push_str(
                format!("🎬 Ran function {function_name} {function_response_message}\n",).as_str(),
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

/// Process the chat message from the user
pub async fn process_chat(
    user_name: String,            // The users name
    users_text: String,           // Users text to bot
    ctx: DiscordContext,          // The discord context
    bot_message: DiscordMessage,  // The message reply to the user
    message_history_text: String, // The message history text, each starts with emoji identifying role
) {
    // Go through each line of message_history_text, if it starts with 💬 add it to user_text_total
    let mut user_text_total = String::new();
    for line in message_history_text.lines() {
        if line.starts_with("💬 ") {
            user_text_total.push_str(line.replace("💬 ", "").as_str());
        }
    }
    // Add the users latest message
    user_text_total.push_str(&users_text);

    // Run chat completion
    run_chat_completition(
        ctx,
        bot_message,
        message_history_text,
        users_text,
        user_name,
    )
    .await;
}
