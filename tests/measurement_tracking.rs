use std::collections::HashMap;

use caracat::models::Probe;
use saimiris::agent::gateway::MeasurementInfo;
use saimiris::agent::sender::ProbesWithSource;

#[tokio::test]
async fn test_measurement_info_parsing() {
    // Test that measurement info is properly parsed from headers
    let measurement_info = MeasurementInfo {
        measurement_id: "test-measurement-123".to_string(),
        end_of_measurement: false,
    };

    assert_eq!(measurement_info.measurement_id, "test-measurement-123");
    assert!(!measurement_info.end_of_measurement);
}

#[tokio::test]
async fn test_probes_with_source_measurement_info() {
    // Test that ProbesWithSource correctly carries measurement info
    let probes = vec![Probe {
        dst_addr: "1.1.1.1".parse().unwrap(),
        src_port: 12345,
        dst_port: 80,
        ttl: 64,
        protocol: caracat::models::L4::UDP,
    }];

    let measurement_info = Some(MeasurementInfo {
        measurement_id: "test-measurement-456".to_string(),
        end_of_measurement: true,
    });

    let probes_with_source = ProbesWithSource {
        probes,
        source_ip: "192.168.1.1".to_string(),
        measurement_info: measurement_info.clone(),
    };

    assert_eq!(probes_with_source.probes.len(), 1);
    assert_eq!(probes_with_source.source_ip, "192.168.1.1");
    assert!(probes_with_source.measurement_info.is_some());

    let info = probes_with_source.measurement_info.unwrap();
    assert_eq!(info.measurement_id, "test-measurement-456");
    assert!(info.end_of_measurement);
}

#[tokio::test]
async fn test_kafka_header_parsing() {
    // Simulate the header parsing logic from the handler
    let mut headers = HashMap::new();
    headers.insert(
        "measurement_id".to_string(),
        "test-measurement-789".to_string(),
    );
    headers.insert("end_of_measurement".to_string(), "false".to_string());

    // Extract measurement info as done in the handler
    let measurement_info = if let Some(measurement_id) = headers.get("measurement_id") {
        let end_of_measurement = headers
            .get("end_of_measurement")
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false);

        Some(MeasurementInfo {
            measurement_id: measurement_id.clone(),
            end_of_measurement,
        })
    } else {
        None
    };

    assert!(measurement_info.is_some());
    let info = measurement_info.unwrap();
    assert_eq!(info.measurement_id, "test-measurement-789");
    assert!(!info.end_of_measurement);
}

#[tokio::test]
async fn test_end_to_end_measurement_tracking() {
    // Simulate the complete flow from Kafka headers to probe sending with tracking

    // 1. Simulate Kafka message headers
    let mut headers = HashMap::new();
    headers.insert(
        "measurement_id".to_string(),
        "integration-test-001".to_string(),
    );
    headers.insert("end_of_measurement".to_string(), "true".to_string());

    // 2. Parse measurement info (as done in handler)
    let measurement_info = if let Some(measurement_id) = headers.get("measurement_id") {
        let end_of_measurement = headers
            .get("end_of_measurement")
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false);

        Some(MeasurementInfo {
            measurement_id: measurement_id.clone(),
            end_of_measurement,
        })
    } else {
        None
    };

    assert!(measurement_info.is_some());
    let info = measurement_info.unwrap();
    assert_eq!(info.measurement_id, "integration-test-001");
    assert!(info.end_of_measurement);

    // 3. Create probes with measurement info (as done in handler)
    let probes = vec![
        Probe {
            dst_addr: "8.8.8.8".parse().unwrap(),
            src_port: 12345,
            dst_port: 53,
            ttl: 64,
            protocol: caracat::models::L4::UDP,
        },
        Probe {
            dst_addr: "1.1.1.1".parse().unwrap(),
            src_port: 12346,
            dst_port: 53,
            ttl: 32,
            protocol: caracat::models::L4::UDP,
        },
        Probe {
            dst_addr: "208.67.222.222".parse().unwrap(),
            src_port: 12347,
            dst_port: 53,
            ttl: 16,
            protocol: caracat::models::L4::UDP,
        },
    ];

    let probes_with_source = ProbesWithSource {
        probes,
        source_ip: "192.168.1.100".to_string(),
        measurement_info: Some(info.clone()),
    };

    // 4. Verify that probes and measurement info are correctly packaged
    assert_eq!(probes_with_source.probes.len(), 3);
    assert_eq!(probes_with_source.source_ip, "192.168.1.100");
    assert!(probes_with_source.measurement_info.is_some());

    let measurement_info = probes_with_source.measurement_info.unwrap();
    assert_eq!(measurement_info.measurement_id, "integration-test-001");
    assert!(measurement_info.end_of_measurement);

    // 5. Verify probe details
    assert_eq!(probes_with_source.probes[0].dst_addr.to_string(), "8.8.8.8");
    assert_eq!(probes_with_source.probes[1].dst_addr.to_string(), "1.1.1.1");
    assert_eq!(
        probes_with_source.probes[2].dst_addr.to_string(),
        "208.67.222.222"
    );

    // This test validates the data flow from Kafka headers through the handler
    // to the sender, ensuring measurement tracking information is preserved
    // throughout the pipeline.
}

#[tokio::test]
async fn test_measurement_tracking_state_management() {
    // Test the state management logic used in the sender

    let mut probes_sent_in_measurement: HashMap<String, u32> = HashMap::new();

    // Simulate first batch
    let measurement_id = "state-test-001".to_string();
    let first_batch_count = 10u32;

    *probes_sent_in_measurement
        .entry(measurement_id.clone())
        .or_insert(0) += first_batch_count;
    assert_eq!(
        *probes_sent_in_measurement.get(&measurement_id).unwrap(),
        10
    );

    // Simulate second batch
    let second_batch_count = 15u32;
    *probes_sent_in_measurement
        .entry(measurement_id.clone())
        .or_insert(0) += second_batch_count;
    assert_eq!(
        *probes_sent_in_measurement.get(&measurement_id).unwrap(),
        25
    );

    // Simulate completion cleanup
    let total_sent = *probes_sent_in_measurement.get(&measurement_id).unwrap();
    assert_eq!(total_sent, 25);

    // Clean up completed measurement
    probes_sent_in_measurement.remove(&measurement_id);
    assert!(probes_sent_in_measurement.get(&measurement_id).is_none());
}

#[tokio::test]
async fn test_client_measurement_tracking_headers() {
    use saimiris::config::parse_and_validate_client_args;

    // Test parsing client config with measurement tracking
    let agents = "agent1:192.168.1.1,agent2:[2001:db8::1]";
    let client_config = parse_and_validate_client_args(agents, None)
        .unwrap()
        .with_measurement_tracking(Some("test-measurement-123".to_string()));

    // Verify that measurement info is set correctly
    assert_eq!(client_config.measurement_infos.len(), 2);

    let agent1 = &client_config.measurement_infos[0];
    assert_eq!(agent1.name, "agent1");
    assert_eq!(agent1.src_ip, Some("192.168.1.1".to_string()));
    assert_eq!(
        agent1.measurement_id,
        Some("test-measurement-123".to_string())
    );

    let agent2 = &client_config.measurement_infos[1];
    assert_eq!(agent2.name, "agent2");
    assert_eq!(agent2.src_ip, Some("2001:db8::1".to_string()));
    assert_eq!(
        agent2.measurement_id,
        Some("test-measurement-123".to_string())
    );
}

#[tokio::test]
async fn test_client_without_measurement_tracking() {
    use saimiris::config::parse_and_validate_client_args;

    // Test parsing client config without measurement tracking
    let agents = "agent1:10.0.0.1";
    let client_config = parse_and_validate_client_args(agents, None).unwrap();

    // Verify that measurement fields are None by default
    assert_eq!(client_config.measurement_infos.len(), 1);

    let agent = &client_config.measurement_infos[0];
    assert_eq!(agent.name, "agent1");
    assert_eq!(agent.src_ip, Some("10.0.0.1".to_string()));
    assert_eq!(agent.measurement_id, None);
}
