use serenity::{
    model::channel::Message as DiscordMessage,
    prelude::{Context as DiscordContext, TypeMapKey},
};

use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
        CreateChatCompletionRequestArgs, Role,
    },
    Client as OpenAiClient,
};
struct OpenAiApi;
impl TypeMapKey for OpenAiApi {
    type Value = OpenAiClient;
}

use chrono::Local;
use futures::StreamExt;
use regex::Regex;

use crate::plugins;

/// Run the chat completition
pub async fn run_chat_completition(
    openai_client: &OpenAiClient,    // The openai client
    ctx: DiscordContext,             // The discord context
    mut bot_message: DiscordMessage, // The reply to the user
    message_history_text: String,    // The message history text
    users_text: String,              // The users text
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
    // Add plugin data to the chat query as role system
    chat_query.push(
        ChatCompletionRequestMessageArgs::default()
            .role(Role::System)
            .content(plugins::get_data())
            .build()
            .unwrap(),
    );
    // Add message history minus the most recent line
    let mut just_history = message_history_text[..message_history_text.len() - 1].to_string();
    // If it contains a \n then it has history
    if just_history.contains("\n") {
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
    // chat_query.append(&mut message.clone());
    println!("chat_query: {:?}", chat_query);

    // Create the openai request
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-4")
        .max_tokens(1024u16)
        .messages(chat_query)
        .build()
        .unwrap();

    // Stream the data
    let mut stream = openai_client.chat().create_stream(request).await.unwrap();
    // TODO if this fails try again up to 3 times
    let mut full_text = String::new();
    let mut user_text = String::new();
    let mut last_user_text = String::new();
    let mut last_edit = Local::now();
    let mut commands: Vec<String> = Vec::new();
    let mut command_replies: Vec<String> = Vec::new();
    while let Some(result) = stream.next().await {
        // Get the result of this chunk
        match result {
            Ok(response) => {
                // Get the first choice of the response
                let chat_choice = response.choices.first().unwrap();
                if let Some(ref content) = chat_choice.delta.content {
                    // Add chunk to full text
                    full_text.push_str(content);
                    // Get commands and user text out of the full text, commands are within []
                    let re_command = Regex::new(r"\[(.*?)\]").unwrap();
                    for cap in re_command.captures_iter(&full_text) {
                        // If command is not in commands, add it
                        if !commands.contains(&cap[1].to_string()) {
                            commands.push(cap[1].to_string());
                            // TODO Run the command
                        }
                    }
                    // User text is outside [], opened [ that arent closed count everything past them as not user text
                    let re_user = Regex::new(r"(?m)(?:\[[^\]\[]*?\]|^)([^\[\]]+)").unwrap();
                    user_text = String::new();
                    for cap in re_user.captures_iter(&full_text) {
                        user_text.push_str(&cap[1]);
                    }

                    // Edit the discord message with the new content, edit it threaded so it doesnt interrupt the stream, max edits per second
                    if last_user_text != user_text
                        && last_edit.timestamp_millis() + 1000 < Local::now().timestamp_millis()
                    {
                        last_user_text = user_text.clone();
                        let ctx_c = ctx.clone();
                        let mut bot_message_c = bot_message.clone();
                        let message_history_text_c = message_history_text.clone();
                        let user_text_c = user_text.clone();
                        tokio::spawn(async move {
                            bot_message_c
                                .edit(&ctx_c.http, |msg| {
                                    msg.content(format!("{message_history_text_c}⌛ {user_text_c}"))
                                })
                                .await
                                .unwrap();
                        });
                        last_edit = Local::now();
                    }
                }
            }
            Err(err) => {
                println!("error: {err}");
            }
        }
    }
    // Edit the discord message with the new content
    if user_text.len() > 0 {
        bot_message
            .edit(&ctx.http, |msg| {
                msg.content(format!("{message_history_text}✅ {user_text}"))
            })
            .await
            .unwrap();
    }

    // TODO if there are system returns, process those
}

/// Process the chat message from the user
pub async fn process_chat(
    openai_client: &OpenAiClient,
    user_name: String,               // The users name
    user_id: String,                 // The users id
    user_text: String,               // Users text to bot
    ctx: DiscordContext,             // The discord context
    mut bot_message: DiscordMessage, // The message reply to the user
    message_history_text: String, // The message history text, each starts with emoji identifying role
    reply_text: String, // The text used in the reply while processing "Hey there I am processing your request"
) {
    // Go through each line of message_history_text, if it starts with 💬 add it to user_text_total
    let mut user_text_total = String::new();
    for line in message_history_text.lines() {
        if line.starts_with("💬 ") {
            user_text_total.push_str(line.replace("💬 ", "").as_str());
        }
    }
    // Add the users latest message
    user_text_total.push_str(&user_text);

    // // Don't reply to non media queries, compare user_text_total with the ai model
    // if !plugins::relevance::check_relevance(openai_client, user_text_total.clone()).await {
    //     // Edit the message to let the user know the message is not valid
    //     bot_message
    //         .edit(&ctx.http, |msg: &mut serenity::builder::EditMessage| {
    //             msg.content(format!("{message_history_text}❌ Hi, I'm a media bot. I can help you with media related questions. What would you like to know or achieve?"))
    //         })
    //         .await
    //         .unwrap();
    //     return;
    // }

    // // Edit the bot_message to let the user know the message is valid and it is progressing
    // bot_message
    //     .edit(&ctx.http, |msg| {
    //         msg.content(format!("{message_history_text}⌛ 2/3 {reply_text}"))
    //     })
    //     .await
    //     .unwrap();

    // Edit the bot_message to let the user know it is progressing
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!("{message_history_text}⌛ 3/3 {reply_text}"))
        })
        .await
        .unwrap();

    // Run chat completion
    run_chat_completition(
        openai_client,
        ctx,
        bot_message,
        message_history_text,
        user_text,
    )
    .await;
}
