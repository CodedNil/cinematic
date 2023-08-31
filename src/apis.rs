use async_openai::types::{
    ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role,
};
use async_openai::Client as OpenAiClient;
use reqwest::Method;
use std::env;
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
        let response = OpenAiClient::new().chat().create(request.clone()).await;
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

    let client = reqwest::Client::new();
    let mut request = client
        .request(method, format!("{arr_url}{url}?apikey={arr_api_key}"))
        .basic_auth(username, Some(password));

    if let Some(body_data) = data {
        request = request
            .header("Content-Type", "application/json")
            .body(body_data);
    }

    let response: serde_json::Value = request.send().await?.json().await?;
    Ok(response)
}
