CREATE DATABASE IF NOT EXISTS saimiris;
CREATE TABLE saimiris.from_kafka
(
    timestamp DateTime64,
    prober_id UInt16,
    reply_src_addr IPv6,
    reply_dst_addr IPv6,
    reply_id UInt16,
    reply_size UInt16,
    reply_ttl UInt8,
    reply_protocol UInt8,
    reply_icmp_type UInt8,
    reply_icmp_code UInt8,
    reply_mpls_labels Array(Tuple(UInt32, UInt8, UInt8, UInt8)),
    probe_src_addr IPv6,
    probe_dst_addr IPv6,
    probe_id UInt16,
    probe_size UInt16,
    probe_protocol UInt8,
    quoted_ttl UInt8,
    probe_src_port UInt16,
    probe_dst_port UInt16,
    probe_ttl UInt8,
    rtt UInt16
)
ENGINE = Kafka()
SETTINGS
    kafka_broker_list = '10.0.0.100:9093',
    kafka_topic_list = 'saimiris-results',
    kafka_group_name = 'clickhouse-saimiris-group',
    kafka_format = 'CSV';

CREATE TABLE saimiris.results
(
    timestamp DateTime64,
    prober_id UInt16,
    reply_src_addr IPv6,
    reply_dst_addr IPv6,
    reply_id UInt16,
    reply_size UInt16,
    reply_ttl UInt8,
    reply_protocol UInt8,
    reply_icmp_type UInt8,
    reply_icmp_code UInt8,
    reply_mpls_labels Array(Tuple(UInt32, UInt8, UInt8, UInt8)),
    probe_src_addr IPv6,
    probe_dst_addr IPv6,
    probe_id UInt16,
    probe_size UInt16,
    probe_protocol UInt8,
    quoted_ttl UInt8,
    probe_src_port UInt16,
    probe_dst_port UInt16,
    probe_ttl UInt8,
    rtt UInt16
)
ENGINE = MergeTree()
ORDER BY (
    probe_protocol,
    probe_src_addr,
    probe_dst_addr,
    probe_src_port,
    probe_dst_port,
    probe_ttl
);

CREATE MATERIALIZED VIEW saimiris.from_kafka_mv TO saimiris.results
AS SELECT * FROM saimiris.from_kafka;