//! Unit tests for probe deserialization
use saimiris::probe::deserialize_probes;

#[test]
fn test_deserialize_probes_valid() {
    let probes = vec![0u8; 10]; // Not a real probe, but should not panic
    let _ = deserialize_probes(probes);
}

#[test]
fn test_deserialize_probes_empty() {
    let probes = vec![];
    let result = deserialize_probes(probes);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}
