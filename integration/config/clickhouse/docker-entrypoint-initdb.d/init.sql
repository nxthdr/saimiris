CREATE DATABASE IF NOT EXISTS saimiris;
CREATE TABLE saimiris.from_kafka
(
    timeReceivedNs UInt64,
    agentId String,
    replySrcAddr IPv6,
    replyDstAddr IPv6,
    replyId UInt16,
    replySize UInt16,
    replyTtl UInt8,
    replyQuotedTtl UInt8,
    replyProtocol UInt8,
    replyIcmpType UInt8,
    replyIcmpCode UInt8,
    replyMplsLabel Array(Tuple(UInt32, UInt8, UInt8, UInt8)),
    probeSrcAddr IPv6,
    probeDstAddr IPv6,
    probeId UInt16,
    probeSize UInt16,
    probeTtl UInt8,
    probeProtocol UInt8,
    probeSrcPort UInt16,
    probeDstPort UInt16,
    rtt UInt16
)
ENGINE = Kafka()
SETTINGS
    kafka_broker_list = '10.0.0.100:9093',
    kafka_topic_list = 'saimiris-replies',
    kafka_group_name = 'clickhouse-saimiris-group',
    kafka_format = 'CapnProto',
    kafka_schema = 'reply.capnp:Reply',
    kafka_max_rows_per_message = 1048576;

CREATE TABLE saimiris.replies
(
    date Date,
	time_inserted_ns DateTime64(9),
	time_received_ns DateTime64(9),
    agent_id String,
    reply_src_addr IPv6,
    reply_dst_addr IPv6,
    reply_id UInt16,
    reply_size UInt16,
    reply_ttl UInt8,
    reply_quoted_ttl UInt8,
    reply_protocol UInt8,
    reply_icmp_type UInt8,
    reply_icmp_code UInt8,
    reply_mpls_labels Array(Tuple(UInt32, UInt8, UInt8, UInt8)),
    probe_src_addr IPv6,
    probe_dst_addr IPv6,
    probe_id UInt16,
    probe_size UInt16,
    probe_ttl UInt8,
    probe_protocol UInt8,
    probe_src_port UInt16,
    probe_dst_port UInt16,
    rtt UInt16
)
ENGINE = MergeTree()
ORDER BY (
    time_received_ns,
    agent_id,
    probe_protocol,
    probe_src_addr,
    probe_dst_addr,
    probe_src_port,
    probe_dst_port,
    probe_ttl
)
PARTITION BY date
TTL date + INTERVAL 7 DAY DELETE;

CREATE MATERIALIZED VIEW saimiris.from_kafka_mv TO saimiris.replies
AS SELECT
	toDate(toDateTime64(timeReceivedNs/1000000000, 9)) AS date,
	now() AS time_inserted_ns,
	toDateTime64(timeReceivedNs/1000000000, 9) AS time_received_ns,
    agentId AS agent_id,
    replySrcAddr AS reply_src_addr,
    replyDstAddr AS reply_dst_addr,
    replyId AS reply_id,
    replySize AS reply_size,
    replyTtl AS reply_ttl,
    replyQuotedTtl AS reply_quoted_ttl,
    replyProtocol AS reply_protocol,
    replyIcmpType AS reply_icmp_type,
    replyIcmpCode AS reply_icmp_code,
    replyMplsLabel AS reply_mpls_labels,
    probeSrcAddr AS probe_src_addr,
    probeDstAddr AS probe_dst_addr,
    probeId AS probe_id,
    probeSize AS probe_size,
    probeTtl AS probe_ttl,
    probeProtocol AS probe_protocol,
    probeSrcPort AS probe_src_port,
    probeDstPort AS probe_dst_port,
    rtt
FROM saimiris.from_kafka;
