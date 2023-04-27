use serenity::{
    model::channel::Message as DiscordMessage,
    prelude::{Context as DiscordContext, TypeMapKey},
};

use async_openai::{
    types::{ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs, Role},
    Client as OpenAiClient,
};

struct OpenAiApi;
impl TypeMapKey for OpenAiApi {
    type Value = OpenAiClient;
}

use crate::examples;
use crate::relevance;

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
