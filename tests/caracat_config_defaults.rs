// Test that CaracatConfig fields are set to defaults if missing or zero in config
use saimiris::config::app_config;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[tokio::test]
async fn test_caracat_config_defaults_enforced() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("test_config.yml");
    let mut file = File::create(&config_path).unwrap();
    // Add minimal agent section to ensure valid metrics_address
    writeln!(file, "agent:").unwrap();
    writeln!(file, "  metrics_address: '0.0.0.0:8080'").unwrap();
    // Intentionally set all fields to zero/empty/None
    writeln!(file, "caracat:").unwrap();
    writeln!(file, "  - batch_size: 0").unwrap();
    writeln!(file, "    instance_id: 0").unwrap();
    writeln!(file, "    dry_run: false").unwrap();
    writeln!(file, "    min_ttl: null").unwrap();
    writeln!(file, "    max_ttl: null").unwrap();
    writeln!(file, "    integrity_check: false").unwrap();
    writeln!(file, "    interface: ''").unwrap();
    writeln!(file, "    src_ipv4_addr: null").unwrap();
    writeln!(file, "    src_ipv6_addr: null").unwrap();
    writeln!(file, "    packets: 0").unwrap();
    writeln!(file, "    probing_rate: 0").unwrap();
    writeln!(file, "    rate_limiting_method: ''").unwrap();
    drop(file);

    let config = app_config(config_path.to_str().unwrap()).await.unwrap();
    let caracat = &config.caracat[0];
    assert_eq!(caracat.batch_size, 100);
    assert_eq!(caracat.instance_id, 0);
    assert_eq!(caracat.dry_run, false);
    assert_eq!(caracat.min_ttl, None);
    assert_eq!(caracat.max_ttl, None);
    assert_eq!(caracat.integrity_check, false);
    assert_eq!(caracat.src_ipv4_addr, None);
    assert_eq!(caracat.src_ipv6_addr, None);
    assert_eq!(caracat.packets, 1);
    assert_eq!(caracat.probing_rate, 100);
    assert_eq!(caracat.rate_limiting_method, "auto");
}

#[tokio::test]
async fn test_caracat_config_defaults_if_none_provided() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("test_config_empty.yml");
    let mut file = File::create(&config_path).unwrap();
    // Add minimal agent section to ensure valid metrics_address
    writeln!(file, "agent:").unwrap();
    writeln!(file, "  metrics_address: '0.0.0.0:8080'").unwrap();
    // No caracat section at all
    writeln!(file, "").unwrap();
    drop(file);

    let config = app_config(config_path.to_str().unwrap()).await.unwrap();
    let caracat = &config.caracat[0];
    assert_eq!(caracat.batch_size, 100);
    assert_eq!(caracat.instance_id, 0);
    assert_eq!(caracat.dry_run, false);
    assert_eq!(caracat.min_ttl, None);
    assert_eq!(caracat.max_ttl, None);
    assert_eq!(caracat.integrity_check, false);
    assert_eq!(caracat.interface, caracat::utilities::get_default_interface());
    assert_eq!(caracat.src_ipv4_addr, None);
    assert_eq!(caracat.src_ipv6_addr, None);
    assert_eq!(caracat.packets, 1);
    assert_eq!(caracat.probing_rate, 100);
    assert_eq!(caracat.rate_limiting_method, "auto");
}
