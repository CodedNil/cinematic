use anyhow::Context;
use async_openai::types::{
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role,
};
use async_openai::Client as OpenAiClient;
use reqwest::{Client, Method};
use std::env;

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

pub fn get_env_variable(key: &str) -> String {
    match env::var(key) {
        Ok(value) => value,
        Err(e) => match e {
            env::VarError::NotPresent => panic!("Environment variable {key} not found."),
            env::VarError::NotUnicode(oss) => {
                panic!("Environment variable {key} contains invalid data: {oss:?}")
            }
        },
    }
}

/// Get from the names file the users name if it exists, cleaned up string
pub async fn user_name_from_id(user_id: &String, user_name_dirty: &str) -> anyhow::Result<String> {
    // Create names.toml file if doesnt exist
    if !std::path::Path::new("names.toml").exists() {
        std::fs::File::create("names.toml").context("Failed to create names file")?;
    }
    let contents = std::fs::read_to_string("names.toml").context("Failed to read names file")?;
    let mut parsed_toml: toml::Value = contents.parse().context("Failed to parse TOML content")?;

    // If doesn't have user, add it
    if !parsed_toml.as_table().unwrap().contains_key(user_id) {
        parsed_toml.as_table_mut().unwrap().insert(
            user_id.to_string(),
            toml::Value::Table(toml::value::Table::new()),
        );
    }
    let user = parsed_toml
        .get(user_id)
        .context("Failed to get user data from parsed TOML")?;

    let user_name = {
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
            .await.map_err(|e| anyhow::anyhow!(e)).context("Failed GPT query")?;

            // Write file
            let name = response;
            let mut user = user.as_table().unwrap().clone();
            user.insert("name".to_string(), toml::Value::String(name.clone()));
            let mut parsed_toml = parsed_toml.as_table().unwrap().clone();
            parsed_toml.insert(user_id.to_string(), toml::Value::Table(user));
            let toml_string =
                toml::to_string(&parsed_toml).context("Failed to serialize TOML data")?;
            std::fs::write("names.toml", toml_string).context("Failed to write to names file")?;

            name
        }
    };

    // Return clean name
    Ok(user_name)
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
        let response = OpenAiClient::new().chat().create(request.clone()).await;
        match response {
            Ok(response) => break Ok(response),
            Err(error) => {
                println!("Failed to get response from openai: {error:?}");
                tries += 1;
                if tries >= 3 {
                    break Err(error);
                }
            }
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
    method: Method,
    service: ArrService,
    url: String,
    data: Option<String>,
) -> anyhow::Result<serde_json::Value> {
    let temp_service = service.to_string().to_uppercase();
    let service_name = temp_service.as_str();

    let arr_api_key = get_env_variable(format!("{service_name}_API").as_str());
    let arr_url = get_env_variable(format!("{service_name}_URL").as_str());
    let username = get_env_variable(format!("{service_name}_AUTHUSER").as_str());
    let password = get_env_variable(format!("{service_name}_AUTHPASS").as_str());

    let client = Client::builder().build()?;
    let last_sep = if url.contains('?') { "&" } else { "?" };
    let mut request = client
        .request(
            method,
            format!("{arr_url}{url}{last_sep}apikey={arr_api_key}"),
        )
        .basic_auth(username, Some(password));
    println!("{arr_url}{url}{last_sep}apikey={arr_api_key}");

    if let Some(body_data) = data {
        request = request
            .header("Content-Type", "application/json")
            .body(body_data);
    }

    let response: serde_json::Value = request.send().await?.json().await?;
    Ok(response)
}
