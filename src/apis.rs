use anyhow::anyhow;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role,
};
use async_openai::Client as OpenAiClient;
use reqwest::Method;
use std::fs::File;
use std::io::prelude::*;

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
pub fn get_openai() -> OpenAiClient<async_openai::config::OpenAIConfig> {
    let cred = get_credentials();

    // Configure the client with your openai api key
    let openai_api_key = cred["openai_api_key"]
        .as_str()
        .expect("Expected a openai_api_key in the credentials.toml file")
        .to_string();
    let config = OpenAIConfig::new().with_api_key(openai_api_key);
    OpenAiClient::with_config(config)
}

/// Get from the names file the users name if it exists, cleaned up string
pub async fn user_name_from_id(user_id: &String, user_name_dirty: &str) -> Option<String> {
    // Create names.toml file if doesnt exist
    if !std::path::Path::new("names.toml").exists() {
        let mut file = File::create("names.toml").expect("Failed to create names file");
        file.write_all("".as_bytes())
            .expect("Failed to write to names file");
    }
    let contents = std::fs::read_to_string("names.toml");
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
                "Convert the above name to plaintext alphanumeric only, if it is already alphanumeric just return the name".to_string(),
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
            std::fs::write("names.toml", toml_string).unwrap();
            name
        }
    };
    // Return clean name
    Some(user_name)
}

/// Use gpt to query information
pub async fn gpt_info_query(model: String, data: String, prompt: String) -> Result<String, String> {
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
        let response = get_openai().chat().create(request.clone()).await;
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
        .clone()
        .unwrap();
    Ok(result)
}

/// Make a request to an arr service
pub async fn arr_request(
    method: HttpMethod,
    service: ArrService,
    url: String,
    data: Option<String>,
) -> anyhow::Result<serde_json::Value> {
    let credentials = get_credentials();
    let service_credentials = credentials[&service.to_string()]
        .as_table()
        .ok_or_else(|| anyhow!("Expected a section in credentials.toml"))?;

    let get_str_value = |key: &str| -> anyhow::Result<String> {
        service_credentials[key]
            .as_str()
            .ok_or_else(|| anyhow!("Expected {} in credentials.toml", key))
            .map(std::string::ToString::to_string)
    };

    let arr_api_key = get_str_value("api")?;
    let arr_url = get_str_value("url")?;
    let username = get_str_value("authuser")?;
    let password = get_str_value("authpass")?;

    let client = reqwest::Client::new();
    let mut request = client
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

    if let Some(body_data) = data {
        request = request
            .header("Content-Type", "application/json")
            .body(body_data);
    }

    let response = request.send().await?.text().await?;
    serde_json::from_str(&response).map_err(Into::into)
}
