//! Unit tests for agent logic (saimiris)
use caracat::models::Probe;
use saimiris::agent::handler::determine_target_sender;
use saimiris::config::CaracatConfig;
use std::collections::HashMap;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn test_determine_target_sender_ip_in_prefix() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    let result = determine_target_sender(&map, &caracat_configs, Some(&"192.168.1.100".to_string()));
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

#[tokio::test]
async fn test_determine_target_sender_ip_not_in_prefix() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    let result = determine_target_sender(&map, &caracat_configs, Some(&"10.0.0.1".to_string()));
    assert!(result.is_err());
}

#[tokio::test]
async fn test_determine_target_sender_no_ip_provided() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    let result = determine_target_sender(&map, &caracat_configs, None);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_determine_target_sender_ipv6_in_prefix() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: None,
        src_ipv6_prefix: Some("2001:db8::/32".to_string()),
        ..Default::default()
    }];

    let result = determine_target_sender(&map, &caracat_configs, Some(&"2001:db8::1".to_string()));
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}
