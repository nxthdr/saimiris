//! Unit tests for agent logic (saimiris)
use caracat::models::Probe;
use saimiris::agent::handler::determine_target_sender;
use std::collections::HashMap;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn test_determine_target_sender_ip_found() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("1.2.3.4".to_string(), tx.clone());
    let result = determine_target_sender(&map, Some(&"1.2.3.4".to_string()), None);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_determine_target_sender_ip_not_found_with_default() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let map = HashMap::new();
    let result = determine_target_sender(&map, None, Some(&tx));
    assert!(result.is_some());
}

#[tokio::test]
async fn test_determine_target_sender_ip_not_found_no_default() {
    let map = HashMap::new();
    let result = determine_target_sender(&map, None, None);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_determine_target_sender_ip_not_in_map() {
    let (tx, _rx) = channel::<Vec<Probe>>(1);
    let mut map = HashMap::new();
    map.insert("1.2.3.4".to_string(), tx.clone());
    let result = determine_target_sender(&map, Some(&"5.6.7.8".to_string()), Some(&tx));
    assert!(result.is_none());
}
