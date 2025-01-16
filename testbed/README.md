# Testbed

Docker Compose setup to facilitate the tests of Osiris.

The testbed consists in a Redpanda and ClickHouse instance. Required ClickHouse [tables](config/clickhouse/docker-entrypoint-initdb.d/init.sql) are created on startup. The `osiris.results_broker` is using the ClickHouse [Kafka engine](https://clickhouse.com/docs/en/engines/table-engines/integrations/kafka) to fetch the results from Redpanda. The `osiris.results` table is used to store the results.


## Usage

* Start the testbed

```sh
docker compose up -d --force-recreate --renew-anon-volumes
```

* Run Osiris (from the root of the repository)

```sh
cargo run
```

* Stop the testbed

```sh
docker compose down
```
