<p align="center">
  <img src="logo/logo.png" height="256" width="256" alt="Project Logo" />
</p>

# Saimiris

> [!WARNING]
> Currently in early-stage development.

## Architecture

Right now the probing system is composed of two main components: the **client** and the **agent**. Those components send and receive messages from Kafka topics, respectively. The results are stored in a ClickHouse table.

Check the [testbed](testbed/README.md) for a quick setup to test the system.

### Agent

The agent performs the measurements. It listens for measurements to be made from a Kafka topic, performs the measurements, and then produces the results to another Kafka topic. The results will eventually be inserted into a ClickHouse table.

```sh
samiris agent --config=saimiris.yml
```

### Client

The client is the agent that sends the measurements to the agent. It sends a message to a Kafka topic, which represents a set of probes to be sent consecutively. A measurement can be composed of multiple messages.


```sh
samiris client --config=saimiris.yml <comma-separated-agent-id> <target>
```

A target is a network to probe. It must follow this format:

```
network,protocol,min_ttl,max_ttl,n_flows
```

where:
- the network is a IPv4/IPv6 prefix.
- the prococol can be `icmp` or `udp`.
- the min_ttl and max_ttl are the minimum and maximum TTL values to probe.
- the n_flows is the number of flows to use.


