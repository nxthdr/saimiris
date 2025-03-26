<p align="center">
  <img src="https://nxthdr.dev/saimiris/logo.png" height="256" width="256" alt="Project Logo" />
</p>

# Saimiris

> [!WARNING]
> Currently in early-stage development.

## Architecture

The probing system is composed of two main components: the **client** and the **agent**. Those components send and receive messages from Kafka topics, respectively. The measurements results can be handled in any way, such as storing them in a ClickHouse database.

Check the [integration](./integration/) tests for a local quick setup of the system.

### Agent

The agent performs the measurements. It is always listening for results. It consumes probes to send from Kafka topic, performs the measurements, and then produces the results to another Kafka topic.

```sh
samiris agent --config=saimiris.yml
```

### Client

The client is the agent that sends the measurements to the agent. It sends messages to a Kafka topic, which represents a set of probes to be sent consecutively. A measurement can be composed of multiple messages.


```sh
cat probes.txt | samiris client --config=saimiris.yml <comma-separated-agent-ids>
```

The probes to send are in the [caracal](https://dioptra-io.github.io/caracal/usage/) format.
