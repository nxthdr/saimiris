# Integration tests

The integration tests setup consists in a Redpanda and ClickHouse instance. Required ClickHouse [tables](config/clickhouse/docker-entrypoint-initdb.d/init.sql) are created on startup. The `saimiris.from_kafka` table is using the ClickHouse [Kafka engine](https://clickhouse.com/docs/en/engines/table-engines/integrations/kafka) to fetch the results from Redpanda. The `saimiris.results` table is used to store the results.

## Automated Testing

### Test Scripts

An integration test script is provided to validate Saimiris functionality after major changes:

**Integration Tests** (`tests/test_integration.sh`):
- Duration: ~3-5 minutes
- Requires Docker and Docker Compose
- Tests: Full end-to-end pipeline including Kafka and ClickHouse
- Use case: Complete validation before releases

### Usage

```bash
# Full end-to-end test (from repository root)
./integration/tests/test_integration.sh
```

### What Gets Tested

The integration test validates recent enhancements:
- ✅ **Config Refactor**: Moved client config to `src/config/` module structure
- ✅ **Agent:IP Format**: Simplified agent specification with embedded IP addresses
- ✅ **CLI Functionality**: Help commands, argument parsing, error handling

The integration test additionally verifies:
- ✅ **Docker Environment**: Redpanda (Kafka) and ClickHouse startup
- ✅ **Agent Operation**: Background agent startup and probe processing
- ✅ **Data Pipeline**: End-to-end probe flow from client through Kafka to ClickHouse

### Requirements

**Integration Tests**: Docker, Docker Compose, ports 9092 and 8123 available

## Manual Testing

For manual testing, you can start the environment and run components individually:

### Usage

* Start the environment

```sh
docker compose up -d --force-recreate --renew-anon-volumes
```

* Run Saimiris Agent (from the root of the repository)

```sh
cargo run -- agent --config=integration/config/saimiris/saimiris.yml
```

* Run Saimiris Client (from the root of the repository)

```sh
cat integration/probes.txt | cargo run -- client --config=integration/config/saimiris/saimiris.yml wbmwwp9vna:127.0.0.1
```

* Check ClickHouse for results

```sh
docker exec -it integration-clickhouse-1 clickhouse-client -q "SELECT * FROM saimiris.replies ORDER BY time_received_ns DESC LIMIT 10"
```

* Stop the environment

```sh
docker compose down
```
