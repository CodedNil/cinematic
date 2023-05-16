//! Access to the apis for getting api keys, making requests to openai, and sonarr/radarr

use reqwest::Method;
use std::fs::File;
use std::io::prelude::*;

use async_openai::types::{
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role,
};
use async_openai::Client as OpenAiClient;

#[derive(Clone)]
pub enum ArrService {
    Sonarr,
    Radarr,
}
impl std::fmt::Display for ArrService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sonarr => write!(f, "sonarr"),
            Self::Radarr => write!(f, "radarr"),
        }
    }
}

#[derive(Debug)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

fn get_credentials() -> toml::Value {
    // Read credentials.toml file to get keys
    let mut file = File::open("credentials.toml").expect("Failed to open credentials file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Failed to read credentials file");
    let cred: toml::Value = contents.parse().expect("Failed to parse credentials TOML");

    cred
}

/// Get discord token
pub fn get_discord_token() -> String {
    let cred = get_credentials();

    // Configure the client with your Discord bot token
    let discord_token: String = cred["discord_token"]
        .as_str()
        .expect("Expected a discord_token in the credentials.toml file")
        .to_owned();
    discord_token
}

/// Get openai client
pub fn get_openai() -> OpenAiClient {
    let cred = get_credentials();

    // Configure the client with your openai api key
    let openai_api_key = cred["openai_api_key"]
        .as_str()
        .expect("Expected a openai_api_key in the credentials.toml file")
        .to_string();
    OpenAiClient::new().with_api_key(openai_api_key)
}

/// Get from the memories file the users name if it exists, cleaned up string
pub async fn user_name_from_id(user_id: &String, user_name_dirty: &str) -> Option<String> {
    // Create memories.toml file if doesnt exist
    if !std::path::Path::new("memories.toml").exists() {
        let mut file = File::create("memories.toml").expect("Failed to create memories file");
        file.write_all("".as_bytes())
            .expect("Failed to write to memories file");
    }
    let contents = std::fs::read_to_string("memories.toml");
    if contents.is_err() {
        return None;
    }
    let mut parsed_toml: toml::Value = contents.unwrap().parse().unwrap();
    // If doesnt have user, add it and write the file
    if !parsed_toml.as_table().unwrap().contains_key(user_id) {
        parsed_toml.as_table_mut().unwrap().insert(
            user_id.to_string(),
            toml::Value::Table(toml::value::Table::new()),
        );
    }
    let user = parsed_toml.get(user_id)?;
    let user_name: String = {
        // If doesn't have the name, add it and write the file
        if user.as_table().unwrap().contains_key("name") {
            user.get("name").unwrap().as_str().unwrap().to_string()
        } else {
            // Convert name to plaintext alphanumeric only with gpt
            let response = gpt_info_query(
                "gpt-4".to_string(),
                user_name_dirty.to_string(),
                "Convert the above name to plaintext alphanumeric only".to_string(),
            )
            .await;
            if response.is_err() {
                return None;
            }
            // Write file
            let name = response.unwrap();
            let mut user = user.as_table().unwrap().clone();
            user.insert("name".to_string(), toml::Value::String(name.clone()));
            let mut parsed_toml = parsed_toml.as_table().unwrap().clone();
            parsed_toml.insert(user_id.to_string(), toml::Value::Table(user));
            let toml_string = toml::to_string(&parsed_toml).unwrap();
            std::fs::write("memories.toml", toml_string).unwrap();
            name
        }
    };
    // Return clean name
    Some(user_name)
}

/// Use gpt to query information
pub async fn gpt_info_query(model: String, data: String, prompt: String) -> Result<String, String> {
    let openai = get_openai();

    // Search with gpt through the memories to answer the query
    let request = CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages([
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content(data)
                .build()
                .unwrap(),
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(prompt)
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap();

    // Retry the request if it fails
    let mut tries = 0;
    let response = loop {
        let response = openai.chat().create(request.clone()).await;
        if let Ok(response) = response {
            break Ok(response);
        }
        tries += 1;
        if tries >= 3 {
            break response;
        }
    };
    // Return from errors
    if response.is_err() {
        return Err("Failed to get response from openai".to_string());
    }
    let result = response
        .unwrap()
        .choices
        .first()
        .unwrap()
        .message
        .content
        .clone();
    Ok(result)
}

/// Make a request to an arr service
pub async fn arr_request(
    method: HttpMethod,
    service: ArrService,
    url: String,
    data: Option<String>,
) -> serde_json::Value {
    let cred = get_credentials();
    let arr = cred[&service.to_string()]
        .as_table()
        .expect("Expected a section in credentials.toml");
    let arr_api_key = arr["api"]
        .as_str()
        .expect("Expected an api in credentials.toml")
        .to_string();
    let arr_url = arr["url"]
        .as_str()
        .expect("Expected a url in credentials.toml")
        .to_string();
    let username = arr["authuser"]
        .as_str()
        .expect("Expected an authuser in credentials.toml")
        .to_string();
    let password = arr["authpass"]
        .as_str()
        .expect("Expected an authpass in credentials.toml")
        .to_string();

    let client = reqwest::Client::new();
    let request = client
        .request(
            match method {
                HttpMethod::Get => Method::GET,
                HttpMethod::Post => Method::POST,
                HttpMethod::Put => Method::PUT,
                HttpMethod::Delete => Method::DELETE,
            },
            format!("{arr_url}{url}"),
        )
        .basic_auth(username, Some(password))
        .header("X-Api-Key", arr_api_key);

    let request = if let Some(data) = data {
        request
            .header("Content-Type", "application/json")
            .body(data)
    } else {
        request
    };

    let response = request
        .send()
        .await
        .expect("Failed to send request")
        .text()
        .await
        .expect("Failed to get response");

    serde_json::from_str(&response).expect("Failed to parse json")
}
