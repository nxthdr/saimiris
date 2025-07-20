//! Unit tests for agent logic (saimiris)
use saimiris::agent::handler::determine_target_sender;
use saimiris::agent::sender::ProbesWithSource;
use saimiris::config::CaracatConfig;
use std::collections::HashMap;
use tokio::sync::mpsc::channel;

#[test]
fn test_determine_target_sender_ip_in_prefix() {
    let (tx, _rx) = channel::<ProbesWithSource>(100);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    let result =
        determine_target_sender(&map, &caracat_configs, Some(&"192.168.1.100".to_string()));
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(use_source_ip); // Should use source IP when prefix is configured
}

#[test]
fn test_determine_target_sender_ip_not_in_prefix() {
    let (tx, _rx) = channel::<ProbesWithSource>(100);
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

#[test]
fn test_determine_target_sender_no_ip_provided() {
    let (tx, _rx) = channel::<ProbesWithSource>(100);
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

#[test]
fn test_determine_target_sender_ipv6_in_prefix() {
    let (tx, _rx) = channel::<ProbesWithSource>(100);
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
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(use_source_ip); // Should use source IP when prefix is configured
}

#[test]
fn test_determine_target_sender_no_prefix() {
    let (tx, _rx) = channel::<ProbesWithSource>(100);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: None,
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    // When no prefix is configured, should return sender without requiring source IP
    let result = determine_target_sender(&map, &caracat_configs, None);
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(!use_source_ip); // Should NOT use source IP when no prefix is configured
}

#[test]
fn test_determine_target_sender_mixed_configs() {
    let (tx_default, _rx_default) = channel::<ProbesWithSource>(100);
    let (tx_prefix, _rx_prefix) = channel::<ProbesWithSource>(100);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx_default.clone()); // Default instance
    map.insert("instance_1".to_string(), tx_prefix.clone()); // Prefix instance

    let caracat_configs = vec![
        CaracatConfig {
            instance_id: 0,
            src_ipv4_prefix: None,
            src_ipv6_prefix: None,
            ..Default::default()
        },
        CaracatConfig {
            instance_id: 1,
            src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
            src_ipv6_prefix: None,
            ..Default::default()
        },
    ];

    // Test 1: Source IP matches prefix - should use prefix instance
    let result =
        determine_target_sender(&map, &caracat_configs, Some(&"192.168.1.100".to_string()));
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(use_source_ip); // Should use source IP

    // Test 2: Source IP doesn't match prefix - should use default instance
    let result = determine_target_sender(&map, &caracat_configs, Some(&"10.0.0.1".to_string()));
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(!use_source_ip); // Should NOT use source IP

    // Test 3: No source IP provided - should use default instance
    let result = determine_target_sender(&map, &caracat_configs, None);
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(!use_source_ip); // Should NOT use source IP
}

#[test]
fn test_determine_target_sender_only_prefix_no_default() {
    let (tx_prefix, _rx_prefix) = channel::<ProbesWithSource>(100);
    let mut map = HashMap::new();
    map.insert("instance_0".to_string(), tx_prefix.clone());

    let caracat_configs = vec![CaracatConfig {
        instance_id: 0,
        src_ipv4_prefix: Some("192.168.1.0/24".to_string()),
        src_ipv6_prefix: None,
        ..Default::default()
    }];

    // Test 1: Source IP matches prefix - should work
    let result =
        determine_target_sender(&map, &caracat_configs, Some(&"192.168.1.100".to_string()));
    assert!(result.is_ok());
    let (sender_option, use_source_ip) = result.unwrap();
    assert!(sender_option.is_some());
    assert!(use_source_ip);

    // Test 2: Source IP doesn't match prefix - should fail (no default available)
    let result = determine_target_sender(&map, &caracat_configs, Some(&"10.0.0.1".to_string()));
    assert!(result.is_err());

    // Test 3: No source IP provided - should fail (no default available)
    let result = determine_target_sender(&map, &caracat_configs, None);
    assert!(result.is_err());
}
