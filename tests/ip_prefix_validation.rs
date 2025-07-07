//! Unit tests for IP prefix validation
use saimiris::config::validate_ip_against_prefixes;

#[test]
fn test_validate_ipv4_in_prefix() {
    let result = validate_ip_against_prefixes(
        "192.168.1.100",
        &Some("192.168.1.0/24".to_string()),
        &None,
    );
    assert!(result.is_ok());
}

#[test]
fn test_validate_ipv4_not_in_prefix() {
    let result = validate_ip_against_prefixes(
        "10.0.0.1",
        &Some("192.168.1.0/24".to_string()),
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_validate_ipv6_in_prefix() {
    let result = validate_ip_against_prefixes(
        "2001:db8::1",
        &None,
        &Some("2001:db8::/32".to_string()),
    );
    assert!(result.is_ok());
}

#[test]
fn test_validate_ipv6_not_in_prefix() {
    let result = validate_ip_against_prefixes(
        "2001:db9::1",
        &None,
        &Some("2001:db8::/32".to_string()),
    );
    assert!(result.is_err());
}

#[test]
fn test_validate_ipv4_no_prefix_configured() {
    let result = validate_ip_against_prefixes(
        "192.168.1.100",
        &None,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_validate_invalid_ip_format() {
    let result = validate_ip_against_prefixes(
        "invalid-ip",
        &Some("192.168.1.0/24".to_string()),
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_validate_invalid_prefix_format() {
    let result = validate_ip_against_prefixes(
        "192.168.1.100",
        &Some("invalid-prefix".to_string()),
        &None,
    );
    assert!(result.is_err());
}
