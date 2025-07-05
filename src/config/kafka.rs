// --- Constants ---
const DEFAULT_KAFKA_BROKERS: &str = "localhost:9092";
const DEFAULT_KAFKA_AUTH_PROTOCOL: &str = "PLAINTEXT";
const DEFAULT_KAFKA_AUTH_SASL_USERNAME: &str = "saimiris";
const DEFAULT_KAFKA_AUTH_SASL_PASSWORD: &str = "saimiris";
const DEFAULT_KAFKA_AUTH_SASL_MECHANISM: &str = "SCRAM-SHA-512";
const DEFAULT_KAFKA_MESSAGE_MAX_BYTES: usize = 990_000;
const DEFAULT_KAFKA_IN_TOPICS: &str = "saimiris-probes";
const DEFAULT_KAFKA_IN_GROUP_ID: &str = "saimiris-agent";
const DEFAULT_KAFKA_OUT_TOPIC: &str = "saimiris-replies";
const DEFAULT_KAFKA_OUT_BATCH_WAIT_TIME: u64 = 1000;
const DEFAULT_KAFKA_OUT_BATCH_WAIT_INTERVAL: u64 = 100;

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct KafkaConfig {
    #[serde(default = "default_kafka_brokers")]
    pub brokers: String,
    #[serde(default = "default_kafka_auth_protocol")]
    pub auth_protocol: String,
    #[serde(default = "default_kafka_auth_sasl_username")]
    pub auth_sasl_username: String,
    #[serde(default = "default_kafka_auth_sasl_password")]
    pub auth_sasl_password: String,
    #[serde(default = "default_kafka_auth_sasl_mechanism")]
    pub auth_sasl_mechanism: String,
    #[serde(default = "default_kafka_message_max_bytes")]
    pub message_max_bytes: usize,
    #[serde(default = "default_kafka_in_topics")]
    pub in_topics: String,
    #[serde(default = "default_kafka_in_group_id")]
    pub in_group_id: String,
    #[serde(default = "default_kafka_out_enable")]
    pub out_enable: bool,
    #[serde(default = "default_kafka_out_topic")]
    pub out_topic: String,
    #[serde(default = "default_kafka_out_batch_wait_time")]
    pub out_batch_wait_time: u64,
    #[serde(default = "default_kafka_out_batch_wait_interval")]
    pub out_batch_wait_interval: u64,
}

// --- Default value functions ---
fn default_kafka_brokers() -> String {
    DEFAULT_KAFKA_BROKERS.to_string()
}

fn default_kafka_auth_protocol() -> String {
    DEFAULT_KAFKA_AUTH_PROTOCOL.to_string()
}

fn default_kafka_auth_sasl_username() -> String {
    DEFAULT_KAFKA_AUTH_SASL_USERNAME.to_string()
}

fn default_kafka_auth_sasl_password() -> String {
    DEFAULT_KAFKA_AUTH_SASL_PASSWORD.to_string()
}

fn default_kafka_auth_sasl_mechanism() -> String {
    DEFAULT_KAFKA_AUTH_SASL_MECHANISM.to_string()
}

fn default_kafka_message_max_bytes() -> usize {
    DEFAULT_KAFKA_MESSAGE_MAX_BYTES
}

fn default_kafka_in_topics() -> String {
    DEFAULT_KAFKA_IN_TOPICS.to_string()
}

fn default_kafka_in_group_id() -> String {
    DEFAULT_KAFKA_IN_GROUP_ID.to_string()
}

fn default_kafka_out_enable() -> bool {
    true
}

fn default_kafka_out_topic() -> String {
    DEFAULT_KAFKA_OUT_TOPIC.to_string()
}

fn default_kafka_out_batch_wait_time() -> u64 {
    DEFAULT_KAFKA_OUT_BATCH_WAIT_TIME
}

fn default_kafka_out_batch_wait_interval() -> u64 {
    DEFAULT_KAFKA_OUT_BATCH_WAIT_INTERVAL
}
