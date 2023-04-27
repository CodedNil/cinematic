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

use crate::examples;
use crate::relevance;

/// Run the chat completition
pub async fn run_chat_completition(
    openai_client: &OpenAiClient,
    relevant_examples: Option<Vec<ChatCompletionRequestMessage>>,
    message: Vec<ChatCompletionRequestMessage>,
) {
    // Get current date and time in DD/MM/YYYY and HH:MM:SS format
    let date = Local::now().format("%d/%m/%Y").to_string();
    let time = Local::now().format("%H:%M:%S").to_string();

    // The initial messages to send to the API
    let mut chat_query: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestMessageArgs::default()
            .role(Role::System)
            .content("You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media; always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need, you reply with the truthful answer of unknown, responses should all be on one line (with comma separation) and compact language")
            .build()
            .unwrap(),
        ChatCompletionRequestMessageArgs::default()
            .role(Role::Assistant)
            .content(format!("The current date is {date}, the current time is {time}, if needing data beyond 2021 training data you can use a web search"))
            .build()
            .unwrap(),
    ];
    // Add relevant examples
    if let Some(mut exm) = relevant_examples {
        chat_query.append(&mut exm)
    }
    // Add message
    chat_query.append(&mut message.clone());

    // Create the openai request
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .max_tokens(1024u16)
        .messages(chat_query)
        .build()
        .unwrap();

    // Stream the data
    let mut stream = openai_client.chat().create_stream(request).await.unwrap();

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                response.choices.iter().for_each(|chat_choice| {
                    if let Some(ref content) = chat_choice.delta.content {
                        println!("{}", content);
                    }
                });
            }
            Err(err) => {
                println!("error: {err}");
            }
        }
    }
}

/// Process the chat message from the user
pub async fn process_chat(
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
    let relevant_examples = examples::get_examples(openai_client, user_text_total).await;

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
            .content(&user_text)
            .build()
            .unwrap(),
    );

    // Run chat completion
    run_chat_completition(openai_client, relevant_examples, current_message).await;
}
