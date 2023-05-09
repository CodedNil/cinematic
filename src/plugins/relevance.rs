use crate::apis;
use async_openai::types::{
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs,
    CreateChatCompletionResponse, Role,
};

/// Check if a message is relevant as a media query, returns true if relevant
pub async fn check_relevance(user_text_total: String) -> bool {
    // Check with a openai prompt if the user text is relevant
    let request = CreateChatCompletionRequestArgs::default()
        .max_tokens(4u16)
        .model("gpt-4")
        .n(3u8)
        .messages([
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content("You determine if a users message is irrelevant to you, is it related to media? Examples of relevant messages: Is movie on, Add series, Remove movie, Change movie to 1080p, Recommend me something, How much space am I using, What memories do you have for me. You reply with a single word answer, yes or no.")
                .build().unwrap(),
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(format!("{user_text_total}\nDo not respond to the above message, is the above text irrelevant? Reply with a single word answer, only say yes if certain"))
                .build().unwrap(),
        ])
        .build().unwrap();

    // Retry the request if it fails
    let mut tries = 0;
    let response = loop {
        let response = apis::get_openai().chat().create(request.clone()).await;
        if let Ok(r) = response {
            break Ok(r);
        } else {
            tries += 1;
            if tries >= 3 {
                break response;
            }
        }
    };
    // Return from errors
    if let Err(error) = response {
        println!("Error: {:?}", error);
        return false;
    }
    // TODO log the openai call and response
    let response: CreateChatCompletionResponse = response.unwrap();

    // Check each response choice for a yes
    let mut is_valid = false;
    for choice in response.choices {
        if !choice.message.content.to_lowercase().contains("yes") {
            is_valid = true;
        }
    }

    is_valid
}
