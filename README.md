# Saimiris

> [!WARNING]
> Currently in early-stage development.

## Structure

The probing system has two main components:

1. The **Controller**, which is an API that manage users, measurements and probers information.
2. The **Probers**, which do the heavy lifting of probing. They listen for measurements to be made from a Kafka topic, perform the measurements, and then store the results to a ClickHouse database.

### Controller

The controller is an API that manages users, measurements, and probers.
A user can authenticate to the controller, get probers information (e.g., the topic to which they are listening), and create measurements.
The controller hold the measurement state (running, paused, stopped) and also the rights (or credits) of the user to perform measurements.

The probers register themselves to the controller, and are able to check measurements status and user credits.

### Probers

The probers are the agents that perform the measurements. They listen to a Kafka topic for measurements to be made, perform the measurements, and then send the results to another Kaka topic. Those results will eventually be inserted to a ClickHouse table.
They will also check the controller for the status of the measurements and the user credits. If the user has no credits left, or if the measurement is stopped, the prober will discard the measurement requests.

The prober listen to Kafka messages, which represent a set of probes to be sent consecutively, belonging to the same measurement. A measurements can be composed of multiple messages.

There is no guaranties that a message will be consumed right away by the prober, as the message queue can be arbitrarily long. But there is the guaranty that all the probes to be send within a message will be sent consecutively.

A user must be logged to send messages to the Kafka topic.

## Benefits of this architecture

* The client decides which probes have to be consecutively sent: 1 message in the broker = a set of probes to send consecutively.
* The client decides when they want to temporarily pause the measurement, simply by controlling when the messages are sent to the Kafka topic.
* The prober is doing the heavy lifting of the measurement, and the controller is only managing the state of the measurements and the user credits. The communication between the controller and the prober is minimal.
* When an operator stops a prober, for an update for instance, the probes are temporarily stored in Kafka, and then consumed in order.
* When a prober is decommissioned, the topic disappears and that’s it.
* A client can have a credit of probes to send, a limit of probes per message, a limit of probes per measurement, … all of this is managed by the controller.




