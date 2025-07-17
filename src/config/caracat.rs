// --- Caracat config ---
// Constants
const DEFAULT_CARACAT_BATCH_SIZE: u64 = 100;
const DEFAULT_CARACAT_INSTANCE_ID: u16 = 0;
const DEFAULT_CARACAT_PACKETS: u64 = 1;
const DEFAULT_CARACAT_PROBING_RATE: u64 = 100;
const DEFAULT_RATE_LIMITING_METHOD: &str = "auto";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CaracatConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_caracat_batch_size")]
    pub batch_size: u64,
    #[serde(default = "default_caracat_instance_id")]
    pub instance_id: u16,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub min_ttl: Option<u8>,
    #[serde(default)]
    pub max_ttl: Option<u8>,
    #[serde(default)]
    pub integrity_check: bool,
    #[serde(default = "default_caracat_interface")]
    pub interface: String,
    #[serde(default)]
    pub src_ipv4_prefix: Option<String>,
    #[serde(default)]
    pub src_ipv6_prefix: Option<String>,
    #[serde(default = "default_caracat_packets")]
    pub packets: u64,
    #[serde(default = "default_caracat_probing_rate")]
    pub probing_rate: u64,
    #[serde(default = "default_rate_limiting_method")]
    pub rate_limiting_method: String,
}

pub fn default_caracat_batch_size() -> u64 {
    DEFAULT_CARACAT_BATCH_SIZE
}

pub fn default_caracat_instance_id() -> u16 {
    DEFAULT_CARACAT_INSTANCE_ID
}

pub fn default_caracat_interface() -> String {
    caracat::utilities::get_default_interface()
}

pub fn default_caracat_packets() -> u64 {
    DEFAULT_CARACAT_PACKETS
}

pub fn default_caracat_probing_rate() -> u64 {
    DEFAULT_CARACAT_PROBING_RATE
}

pub fn default_rate_limiting_method() -> String {
    DEFAULT_RATE_LIMITING_METHOD.to_string()
}

impl CaracatConfig {
    /// Validates and normalizes the configuration, setting defaults for zero values
    pub fn validate_and_normalize(&mut self) {
        if self.batch_size == 0 {
            self.batch_size = default_caracat_batch_size();
        }
        if self.instance_id == 0 {
            self.instance_id = default_caracat_instance_id();
        }
        if self.interface.is_empty() {
            self.interface = default_caracat_interface();
        }
        if self.packets == 0 {
            self.packets = default_caracat_packets();
        }
        if self.probing_rate == 0 {
            self.probing_rate = default_caracat_probing_rate();
        }
        if self.rate_limiting_method.is_empty() {
            self.rate_limiting_method = default_rate_limiting_method();
        }
    }
}
