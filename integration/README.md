# Integration tests

The integration tests setup consists in a Redpanda and ClickHouse instance. Required ClickHouse [tables](config/clickhouse/docker-entrypoint-initdb.d/init.sql) are created on startup. The `saimiris.from_kafka` table is using the ClickHouse [Kafka engine](https://clickhouse.com/docs/en/engines/table-engines/integrations/kafka) to fetch the results from Redpanda. The `saimiris.results` table is used to store the results.

## Usage

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
cat integration/probes.txt | cargo run -- client --config=integration/config/saimiris/saimiris.yml wbmwwp9vna
```

* Stop the environment

```sh
docker compose down
```
