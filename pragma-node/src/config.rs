use serde::Deserialize;
use tokio::sync::OnceCell;

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    host: String,
    port: u16,
    metrics_port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            metrics_port: 8080,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct KafkaConfig {
    pub topic: String,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            topic: "pragma-data".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RedisConfig {
    redis_host: String,
    redis_port: u16,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            redis_host: "0.0.0.0".to_string(),
            redis_port: 6379,
        }
    }
}

#[derive(Default, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dev,
    #[default]
    Production,
}

#[derive(Default, Debug, Deserialize)]
pub struct ModeConfig {
    mode: Mode,
}

#[derive(Default, Debug, Deserialize)]
pub struct Config {
    mode: ModeConfig,
    server: ServerConfig,
    kafka: KafkaConfig,
    redis: RedisConfig,
}

impl Config {
    pub fn is_production_mode(&self) -> bool {
        self.mode.mode == Mode::Production
    }

    pub fn server_host(&self) -> &str {
        &self.server.host
    }

    pub fn server_port(&self) -> u16 {
        self.server.port
    }

    pub fn metrics_port(&self) -> u16 {
        self.server.metrics_port
    }

    pub fn kafka_topic(&self) -> &str {
        &self.kafka.topic
    }

    pub fn redis_host(&self) -> &str {
        &self.redis.redis_host
    }

    pub fn redis_port(&self) -> u16 {
        self.redis.redis_port
    }
}

pub static CONFIG: OnceCell<Config> = OnceCell::const_new();

async fn init_config() -> Config {
    let server_config = envy::from_env::<ServerConfig>().unwrap_or_default();
    let kafka_config = envy::from_env::<KafkaConfig>().unwrap_or_default();
    let redis_config = envy::from_env::<RedisConfig>().unwrap_or_default();
    let mode_config = envy::from_env::<ModeConfig>().unwrap_or_default();

    Config {
        server: server_config,
        kafka: kafka_config,
        redis: redis_config,
        mode: mode_config,
    }
}

pub async fn config() -> &'static Config {
    CONFIG.get_or_init(init_config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_server_config() {
        let server_config = ServerConfig::default();
        assert_eq!(server_config.host, "0.0.0.0");
        assert_eq!(server_config.port, 3000);
    }

    #[tokio::test]
    async fn test_default_kafka_config() {
        let kafka_config = KafkaConfig::default();
        assert_eq!(kafka_config.topic, "pragma-data");
    }

    #[tokio::test]
    async fn test_config_values() {
        let config = init_config().await;
        assert_eq!(config.server_host(), "0.0.0.0");
        assert_eq!(config.server_port(), 3000);
        assert_eq!(config.kafka_topic(), "pragma-data");
    }
}
