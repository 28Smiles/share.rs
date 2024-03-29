use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub host: String,
    pub port: i64,
    pub storage_folder: String,
    pub users: HashMap<String, UserData>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct UserData {
    pub key: String,
    pub folder: String,
}

impl Default for UserData {
    fn default() -> Self {
        UserData {
            key: String::from_utf8(
                rand::thread_rng()
                    .sample_iter(rand::distributions::Alphanumeric)
                    .take(512)
                    .collect(),
            )
            .unwrap(),
            folder: "default_user".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "localhost".to_string(),
            port: 8080,
            storage_folder: "store".to_string(),
            users: HashMap::from([("default_user".to_string(), UserData::default())]),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, serde_yaml::Error> {
        if let Ok(content) = fs::read_to_string("config.yml") {
            serde_yaml::from_str(&*content)
        } else {
            let config = Config::default();
            let content = serde_yaml::to_string(&config).unwrap();
            fs::write("config.yml", &*content)
                .expect("Could not create or write to file `config.yml`.");

            Ok(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Config;

    #[test]
    fn test_config_load() {
        let config = Config::load().unwrap();
        assert_eq!(&config.host, "localhost");
        assert_eq!(config.port, 8080);
        assert_eq!(&config.storage_folder, "store");
        assert_eq!(config.users.len(), 1);

        let (user, user_data) = *(&config.users).iter().peekable().peek().unwrap();
        assert_eq!(user, "user1");
        assert_eq!(&user_data.folder, "user1");
        assert_eq!(&user_data.key, "mysecret");
    }
}
