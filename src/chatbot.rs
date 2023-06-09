//! Manages the main chat loop

use serenity::{model::channel::Message as DiscordMessage, prelude::Context as DiscordContext};

use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
    CreateChatCompletionRequestArgs, Role,
};

use chrono::Local;
use futures::StreamExt;
use regex::Regex;
use std::sync::{Arc, Mutex};

use crate::{apis, plugins};

/// Helper function to handle command execution
async fn execute_command(
    command: String,
    user_id: String,
    user_name: String,
    command_replies_lock: Arc<Mutex<Vec<String>>>,
    extra_message_history_lock: Arc<Mutex<String>>,
) {
    let command_replies_lock_c = Arc::clone(&command_replies_lock);
    let extra_message_history_lock_c = Arc::clone(&extra_message_history_lock);

    tokio::spawn(async move {
        // Add the processing message in the discord message
        let processing_msg = plugins::get_processing_message(&command);
        extra_message_history_lock
            .lock()
            .unwrap()
            .push_str(format!("{processing_msg}\n").as_str());

        let reply = plugins::run_command(&command, &user_id, &user_name).await;
        let command_reply = format!("{command}~{result}", result = reply.result);
        // Push the command plus reply into the mutex
        let mut command_replies: std::sync::MutexGuard<Vec<String>> =
            command_replies_lock_c.lock().unwrap();
        command_replies.push(command_reply);

        // Get a clone of the history String within the Mutex
        let mut history = {
            let history_lock = extra_message_history_lock_c.lock().unwrap();
            history_lock.clone()
        };

        // Remove processing message, {processing_msg}\n with replace
        history = history.replace(format!("{processing_msg}\n").as_str(), "");

        // Add the reply to user in the discord message
        history.push_str(format!("{}\n", reply.to_user).as_str());

        // Update the Mutex with the new history:
        {
            let mut history_lock = extra_message_history_lock_c.lock().unwrap();
            *history_lock = history.clone();
        }
    });
}

/// Run a ai chat completition to process commands
pub async fn chat_completition_step(
    ctx: DiscordContext,                            // The discord context
    mut bot_message: DiscordMessage,                // The reply to the user
    message_history_text: String,                   // The message history text
    chat_query: Vec<ChatCompletionRequestMessage>,  // The chat query to send to gpt
    extra_message_history_lock: Arc<Mutex<String>>, // The extra message history mutex
    user_id: String,                                // The user id
    user_name: String,                              // The user name
) -> (Vec<String>, String) {
    // Create the gpt request
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-4")
        .max_tokens(1024u16)
        .messages(chat_query)
        .build()
        .unwrap();

    // Stream the data
    let mut stream = apis::get_openai()
        .chat()
        .create_stream(request)
        .await
        .unwrap();
    let mut full_text = String::new();
    let mut user_text = String::new();
    let mut last_user_text = String::new();
    let mut last_edit = Local::now();
    // Collect commands, and replies in a mutex
    let mut commands: Vec<String> = Vec::new();
    let command_replies_lock: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    // Stream the reply
    while let Some(result) = stream.next().await {
        // Get the result of this chunk
        match result {
            Ok(response) => {
                // Get the first choice of the response
                let chat_choice = response.choices.first().unwrap();
                // If the content is not valid, continue
                if chat_choice.delta.content.is_none() {
                    continue;
                }
                let content = chat_choice.delta.content.as_ref().unwrap();
                // Add chunk to full text
                full_text.push_str(content);

                // Get commands and user text out of the full text, commands are within []
                let re_command = Regex::new(r"\[(.*?)\]").unwrap();
                for cap in re_command.captures_iter(&full_text) {
                    // If command is not in commands, add it
                    if !commands.contains(&cap[1].to_string()) {
                        commands.push(cap[1].to_string().clone());
                        // Run in a thread
                        execute_command(
                            cap[1].to_string(),
                            user_id.clone(),
                            user_name.clone(),
                            Arc::clone(&command_replies_lock),
                            Arc::clone(&extra_message_history_lock),
                        )
                        .await;
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
                    let extra_history_text_c = extra_message_history_lock.lock().unwrap().clone();
                    let user_text_c = user_text.clone();
                    tokio::spawn(async move {
                        bot_message_c
                            .edit(&ctx_c.http, |msg| {
                                msg.content(format!("{message_history_text_c}{extra_history_text_c}⌛ {user_text_c}"))
                            })
                            .await
                            .unwrap();
                    });
                    last_edit = Local::now();
                }
            }
            Err(err) => {
                println!("error: {err}");
            }
        }
    }
    // Edit the discord message with the new content
    let extra_history_text_c = extra_message_history_lock.lock().unwrap().clone();
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!(
                "{message_history_text}{extra_history_text_c}⌛ {user_text}"
            ))
        })
        .await
        .unwrap();

    // Wait until command replies are same length as commands, or timeout
    let start_time = Local::now();
    while commands.len() != command_replies_lock.lock().unwrap().len() {
        if Local::now().timestamp_millis() > start_time.timestamp_millis() + 30000 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // Get the command replies
    let command_replies = command_replies_lock.lock().unwrap().clone();

    // Edit the discord message with the new content
    let extra_history_text_c = extra_message_history_lock.lock().unwrap().clone();
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!(
                "{message_history_text}{extra_history_text_c}⌛ {user_text}"
            ))
        })
        .await
        .unwrap();

    (command_replies, user_text)
}

/// Run the chat completition
pub async fn run_chat_completition(
    ctx: DiscordContext,             // The discord context
    mut bot_message: DiscordMessage, // The reply to the user
    message_history_text: String,    // The message history text
    users_text: String,              // The users text
    user_id: String,                 // The user id
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

    // Extra message history mutex
    let extra_message_history_lock: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    // Process the chat with running commands until either no data lookups left, or max iterations reached
    let mut iteration = 0;
    let max_iterations = 5;
    let mut latest_user_text = String::new();
    while iteration < max_iterations {
        let (command_results, cmd_user_text) = chat_completition_step(
            ctx.clone(),
            bot_message.clone(),
            message_history_text.clone(),
            chat_query.clone(),
            extra_message_history_lock.clone(),
            user_id.clone(),
            user_name.clone(),
        )
        .await;
        if !cmd_user_text.is_empty() {
            latest_user_text = cmd_user_text.clone();
        }

        // Check if there are any command returns with outputs
        let mut has_output = false;
        for result in &command_results {
            // If there is any text after the second ~ then there is output
            if result.split('~').count() > 2 {
                has_output = true;
                break;
            }
        }

        // If there are no output results, break the loop
        if !has_output {
            break;
        }

        // Process the command results if there are outputs
        // Get all commands in one string
        let mut commands = String::new();
        for result in &command_results.clone() {
            commands.push_str(format!("[{result}]").as_str());
        }
        commands.push_str(cmd_user_text.as_str());
        // Add system message with results
        chat_query.push(
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content(commands)
                .build()
                .unwrap(),
        );

        // Increment the iteration counter
        iteration += 1;
    }
    // Edit the discord message finalised
    let extra_history_text_c = extra_message_history_lock.lock().unwrap().clone();
    bot_message
        .edit(&ctx.http, |msg| {
            msg.content(format!(
                "{message_history_text}{extra_history_text_c}✅ {latest_user_text}"
            ))
        })
        .await
        .unwrap();
}

/// Process the chat message from the user
pub async fn process_chat(
    user_name: String,            // The users name
    user_id: String,              // The users id
    user_text: String,            // Users text to bot
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
    user_text_total.push_str(&user_text);

    // Run chat completion
    run_chat_completition(
        ctx,
        bot_message,
        message_history_text,
        user_text,
        user_id,
        user_name,
    )
    .await;
}
