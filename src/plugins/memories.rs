use crate::{apis, plugins::PluginReturn};

// Plugins data
pub fn get_plugin_data() -> String {
    "[MEM_GET~key;query]: Looks in users memories for a [key;query], replies with the answered query
[MEM_SET~key;query]: Updates a users memories with a [key;query]
Valid keys are series (what series they want available), movies (what movies they want available), opinions (if user liked show, their rating etc, store lots here)
Examples: [MEM_SET~series;wants The Office] will add The Office to the users series wants [MEM_GET~opinions;liked avatar?] [MEM_SET~movies;Rated Iron Man 7/10]".to_string()
}

/// Get processing message
pub async fn processing_message_get(query: String) -> String {
    return format!("🧠 Looking in memories for query {query}");
}
pub async fn processing_message_set(query: String) -> String {
    return format!("🧠 Settings memories for query {query}");
}

/// Perform a search with ai processing to answer a prompt
pub async fn memory_get(search: String, user_id: &String) -> PluginReturn {
    // Get the key and query
    let (key, query) = match search.split_once(";") {
        Some((key, query)) => (key, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid memory query"),
                to_user: String::from("❌ Memory query failed"),
            }
        }
    };
    // Read memories.toml, parse it to toml::Value, then get the memories of the user_id, then get the key within that
    let memory_value = match get_memory_key(user_id, key) {
        Ok(value) => value.clone(),
        Err(error) => {
            return PluginReturn {
                result: error.to_string(),
                to_user: format!("❌ Couldn't get memory: {error}"),
            };
        }
    };
    // Search with gpt through the memories to answer the query
    let response = apis::gpt_info_query("gpt-3.5-turbo".to_string(), memory_value, format!("Your answers should be on one line and compact with lists having comma separations\nBased on the given information and only this information, user {query}")).await;
    // Return from errors
    if let Err(_) = response {
        return PluginReturn {
            result: String::from("Couldn't find an answer"),
            to_user: format!("❌ Memory lookup couldn't find an answer for query {query}"),
        };
    }
    return PluginReturn {
        result: response.unwrap(),
        to_user: format!("🧠 Memory lookup ran for query {query}"),
    };
}

/// Use ai processing to set a memory, remove or add to existing
pub async fn memory_set(search: String, user_id: &String, user_name: &String) -> PluginReturn {
    // Get the key and query
    let (key, query) = match search.split_once(";") {
        Some((key, query)) => (key, query),
        None => {
            return PluginReturn {
                result: String::from("Invalid memory query"),
                to_user: String::from("❌ Memory query failed"),
            }
        }
    };

    // Read memories.toml, parse it to toml::Value, then get the memories of the user_id, then get the key within that
    let memory_value = match get_memory_key(user_id, key) {
        Ok(value) => value.clone(),
        Err(_) => String::new(),
    };

    // Search with gpt through the memories to answer the query
    let response = apis::gpt_info_query("gpt-4".to_string(), memory_value, format!("Rewrite the memory with the new information\n{query}\nReturn the new memory in ; separated list format")).await;
    // Return from errors
    if let Err(_) = response {
        return PluginReturn {
            result: String::from("Couldn't find an answer"),
            to_user: format!("❌ Memory lookup couldn't find an answer for query {query}"),
        };
    }
    let new_memory = response.unwrap();
    // Write the new memory to memories.toml, user_id not be a valid key, key should be overwritten
    let mut contents = std::fs::read_to_string("memories.toml").unwrap();
    let mut parsed_toml: toml::Value = contents.parse().unwrap();
    let user_memories = parsed_toml
        .get_mut(user_id)
        .unwrap()
        .as_table_mut()
        .unwrap();
    user_memories.insert(String::from("name"), toml::Value::String(user_name.clone()));
    user_memories.insert(key.to_string(), toml::Value::String(new_memory));
    contents = toml::to_string(&parsed_toml).unwrap();
    std::fs::write("memories.toml", contents).unwrap();

    return PluginReturn {
        result: String::from("Users memory was set successfully with the query"),
        to_user: format!("🧠 Memory set with query {query}"),
    };
}

/// Get the memories for a user and key
fn get_memory_key(user_id: &String, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string("memories.toml");
    if contents.is_err() {
        return Err("Failed to read memories file".into());
    }
    let parsed_toml: toml::Value = contents?.parse()?;

    let memory_value = parsed_toml
        .get(user_id)
        .ok_or("User has no memories")?
        .get(key)
        .ok_or("User has no memories for this key")?;

    Ok(memory_value.to_string())
}
