# Testbed

Docker Compose setup to facilitate the tests of Osiris.

The testbed consists in a Redpanda and ClickHouse instance. Required ClickHouse [tables](config/clickhouse/docker-entrypoint-initdb.d/init.sql) are created on startup. The `osiris.from_kafka` is using the ClickHouse [Kafka engine](https://clickhouse.com/docs/en/engines/table-engines/integrations/kafka) to fetch the results from Redpanda. The `osiris.results` table is used to store the results.

As an example, Redpanda is configured with SASL authentication, and uses the default Osiris SASL credentials.

## Usage

* Start the testbed

```sh
docker compose up -d --force-recreate --renew-anon-volumes
```

* Run Osiris Prober (from the root of the repository)

```sh
cargo run -- prober --config=testbed/config/osiris/osiris.yml
```

* Run Osiris Client (from the root of the repository)

```sh
cargo run -- prober --config=testbed/config/osiris/osiris.yml  2606:4700:4700::1111/128,1,32,1
```

* Stop the testbed

```sh
docker compose down
```
