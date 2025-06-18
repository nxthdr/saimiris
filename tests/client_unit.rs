//! Unit tests for client utilities (CSV parsing, batching)
use caracat::models::Probe;
use saimiris::client::handler::read_probes_from_csv;
use saimiris::client::producer::create_messages;
use std::io::Cursor;

#[test]
fn test_read_probes_from_csv_valid() {
    // CSV for a single probe: dst_addr,src_port,dst_port,ttl,protocol
    let csv = "::1,1234,4321,64,ICMP\n";
    let cursor = Cursor::new(csv);
    let result = read_probes_from_csv(cursor);
    assert!(result.is_ok(), "CSV parse error: {:?}", result);
    let probes = result.unwrap();
    assert!(!probes.is_empty());
}

#[test]
fn test_read_probes_from_csv_empty() {
    let csv = "";
    let cursor = Cursor::new(csv);
    let result = read_probes_from_csv(cursor);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_read_probes_from_csv_malformed() {
    let csv = "not,a,probe\n";
    let cursor = Cursor::new(csv);
    let result = read_probes_from_csv(cursor);
    assert!(result.is_err());
}

#[test]
fn test_create_messages_empty() {
    let probes: Vec<Probe> = vec![];
    let batches = create_messages(probes, 100);
    assert!(batches.is_empty());
}
