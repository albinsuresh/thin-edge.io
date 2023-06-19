# MQTT Topic Guidelines

This document provides the guidelines for designing MQTT topic structures for thin-edge itself and its extensions.
The goal is to provide a consistent structure so that future extensions are easy and natural.
It should also help plugin/extension developers to define their topic schemes as well, inline with the tedge topics.

## Background

The inconsistency in the existing topic schemes of thin-edge has long been a problem for both users and the
thin-edge dev team to write new applications or new extensions of thin-edge.

Here are a few such examples:
1. Topic for events: `tedge/events/{event-type}/{child-id}`
1. Topic for firmware update commands: `tedge/{child-id}/commands/req/firmware_update`
1. Topic for software update commands: `tedge/commands/req/software/list`

... where there is inconsistency in the placement of child devices and how the commands are grouped.

There are a few other limitations like the lack of support for services on the thin-edge device,
difficulty in extending existing topics with additional sub-topics, etc
which are detailed in the requirements section.

## Use-cases

TBD

## Requirements

This section is divided into 3 parts:
1. Must-have: for the requirements that must be met by the proposed solutions
2. Nice-to-have: these requirements are not mandatory, but the solution addressing more of these requirements would be a plus
3. Out-of-scope: those that are relevant but out of scope for this design exercise

### Must-have

1. A topic structure to receive telemetry data from services running on the main device,
   which can't be associated to the main device or some child device,
   but must be associated to a logical service entity that is linked to the main device.
1. Consistency in the topic structures for the main device, child device and services
1. Support nested child devices so that telemetry data and commands can be received from/sent to child devices of child devices.
   deployments with at least 3 levels of nesting: a thin-edge device, its children and grand-children are not uncommon.
1. When device IDs are used in topics, it is expected that all nested child devices under a thin-edge device are unique.
   They need not be globally unique or even unique across multiple thin-edge devices.
1. If child devices can not ensure uniqueness in their IDs,
   a registration step with thin-edge can be used to get unique IDs under the thin-edge namespace.
   This registration step must be optional for devices with unique IDs.
1. Support the following kinds of filtering with minimal effort (ideally with a single wildcard subscription):
   * All data from thin-edge device excluding everything else(other services and child devices)
   * All data from all child devices excluding those from thin-edge device and services
   * All data from all services excluding those from thin-edge device and child devices
   * All data from a specific child device excluding everything else
   * All data from a specific service excluding everything else
1. The topic structure should be ACL friendly so that rules can be applied
   to limit child devices and services to access only the data meant for them.
1. Enable easier extension of topics with further topic suffixes in future:
   E.g: Support `type` in the topic for measurements like `tedge/measurements/{measurement-type}`

### Nice-to have

1. Dynamic creation/registration of child devices on receipt of the very first data that they send.
   This is desired at least for immediate child devices, if not for further nested child devices.
1. Easy to create static routing rules so that it is easy to map a local MQTT topic for a nested child device
   to the equivalent cloud topic for its twin.
1. All the existing topics like `tedge/measurements`, `tedge/events` imply that the data received on these
   must be forwarded to the cloud as well.
   Currently there is no way to tell thin-edge to just route some data internally and not forward those to the cloud.
   Since filtering and aggregation on the edge is a very common use-case, especially for local analytics, this is highly desired.
1. It would be ideal if the context/source of data (tedge device, service or child device)
   can be understood from the topic directly.
   For e.g: a topic scheme like `tedge/main/{id}`, `tedge/service/{id}` and `tedge/child/{id}` is more user-friendly
   than a simpler context agnostic scheme like `tedge/{id}` where `id` can be for any "thing".
1. Allowing the "things" in a local domain to use their user-friendly unique IDs (within the tedge namespace)
   in MQTT topics over their globally unique cloud IDs (typically very long and cumbersome) would be desired.
   Ideally, all translation between user-provided-ids and cloud-twin-ids should be done internally by thin-edge,
   as long as it doesn't adversely affect any data mapping cost at scale.
1. Limit the topic levels to 7 as AWS IoT core has a [max limit of 7](https://docs.aws.amazon.com/whitepapers/latest/designing-mqtt-topics-aws-iot-core/mqtt-design-best-practices.html)
1. Symmetric topics: It would be good to keep the subtopic levels symmetric so that the wildcard subscriptions like
   "subscribe to all measurements from all devices and services" are easier.
### Out of scope

1. Routing different kinds of data to different clouds, e.g: all telemetry to azure and all commands from/to Cumulocity.
   Even though this requirement is realistic, thin-edge MQTT topics must not be corrupted with cloud specific semantics,
   and the same requirement will be handled with some external routing mechanism(e.g: routing via bridge topics)
1. Services on child devices. Service support can be limited to thin-edge devices only.

## Proposals

### Dedicated topics for tedge device, services and child devices

The topics for the thin-edge device, the services running on it and child devices have different prefixes:

For parent: tedge/main/<device-id>
For services: tedge/service/<service-id>
For immediate child devices: tedge/child/<child-id>
For nested child devices: tedge/descendent/<child-id>

#### Telemetry

For telemetry data, the topics would be grouped under a `telemetry/` sub-topic as follows:
* Measurements: `tedge/main/<device-id>/telemetry/measurements`
* Events: `tedge/main/<device-id>/telemetry/events/<event-type>`
* Alarms: `tedge/main/<device-id>/telemetry/alarms/<alarm-type>/<severity>`

For child devices and services, a similar structure is followed like: `tedge/child/<child-id>/telemetry/measurements`,
`tedge/service/<service-id>/telemetry/events/<event-type>` etc

The subtopic levels `main/<device-id>` are really not required for the main device.
They are added just for the sake of consistency with child devices and services so that
it is easier to make queries like "subscribe to all measurements from all devices and services"
which can be achieved with a wildcard subscription on `tedge/+/<device-id>/telemetry/measurements`.
If this is not desired, then we can simplify it to just `tedge/telemetry/measurements`.

#### Commands

Similarly, all commands would be grouped under a `commands/` sub-topic as follows:
For requests: `tedge/main/<device-id>/commands/req/<operation-type>/<operation-specific-keys>...`
For responses: `tedge/main/<device-id>/commands/res/<operation-type>/<operation-specific-keys>...`

The `operation-specific-keys` are optional and the number of such keys (topic levels) could vary from one operation to another.

Examples:
* Software list operation: `tedge/main/<device-id>/commands/req/software_list` and `tedge/main/<device-id>/commands/res/software_list`
* Software update operation `tedge/main/<device-id>/commands/req/software_update` and `tedge/main/<device-id>/commands/res/software_update`
* Firmware update operation `tedge/main/<device-id>/commands/req/firmware_update` and `tedge/main/<device-id>/commands/res/firmware_update`
* Device restart operation `tedge/main/<device-id>/commands/req/device_restart` and `tedge/main/<device-id>/commands/res/device_restart`
* Configuration snapshot operation `tedge/main/<device-id>/commands/req/config_snapshot` and `tedge/main/<device-id>/commands/res/config_snapshot`
* Configuration update operation `tedge/main/<device-id>/commands/req/config_update` and `tedge/main/<device-id>/commands/res/config_update`

Although all the above examples maintain consistent structure by ending with the `<operation-type>`,
further additions are possible in future if desired for a given operation type.
For e.g: `tedge/main/<device-id>/commands/req/config_update/<config-type>` to address a specific `config-type`

Similarly, for the response topics, another variation that supports multiple response types is also feasible, as follows:
`tedge/main/<device-id>/commands/<res-type>/<op-type>`

Examples:
* `tedge/main/<device-id>/commands/executing/config_update`
* `tedge/main/<device-id>/commands/successful/config_update`
* `tedge/main/<device-id>/commands/failed/config_update`

Child devices follow a similar structure for commands as well:
* `tedge/child/<child-id>/commands/req/software_list`
* `tedge/child/<child-id>/commands/res/software_update`

#### Nested child devices

Immediate and nested child devices can be registered with thin-edge using its registration service,
by sending the following MQTT message to the topic: `tedge/main/init/child/req/<child-id>`:

```json
{ "parent": "<parent-device-id>" }
```

The `parent-device-id` is the device-id of the direct parent that the child device is connected to.
The payload can have other fields describing the capabilities of that device as well (config management, software management etc).

```admonish warning
The exact topic keys and payload format for this init contract can be discussed and refined separately.
The focus here is just on the MQTT topic structure.
```

Thin-edge needs to maintain the lineage (hierarchy of parent devices) of all descendent child devices in its internal state,
so that it can be looked up while receiving any data from them.
Even though the child device only declares its immediate parent in the registration message,
the entire lineage can be traced back with a recursive lookup on that `parent-device-id`.

The registration status, whether the device registration succeeded or not, is sent back on `tedge/main/init/child/res/<child-id>`,
with the internal device id used by thin-edge to uniquely identify that device as follows:

```json
{
   "id": "<internal-id>",
   "status": "successful" 
}
```

A failure is indicated with a failed status: `{ "status": "failed" }`.

Similarly, a service can register itself by sending the init message to `tedge/main/init/service/req/<service-id>`
and expect the response on `tedge/main/init/service/res/<service-id>`.

Once the registration is complete, these nested child devices can use the `tedge/descendent/<child-id>` topic prefix
to send telemetry data or receive commands as follows:

* Measurement: `tedge/descendent/<internal-id>/telemetry/measurements`
* Command: `tedge/descendent/<internal-id>/commands/req/software_list`

#### Automatic registration

Even though an explicit registration is always desired,
automatic registration is also supported for services and immediate child devices of the thin-edge device,
on the receipt of the very first message from them.
But, this automatic registration is not supported for descendent child devices.
An explicit registration is mandatory for them.

**Pros**

1. The context on whether the data is coming from the parent, a child or a service is clear from the topics.
1. Automatic registration is possible at least for child devices and main device services.

**Cons**

1. No support for services on child devices.
   Even though the support can be added, it would make the topics extremely long.
   E.g: `tedge/child/<child-id>/service/<service-id>/telemetry/measurements`
1. The topics are fairly long and with extensions might easily cross the 7 sub-topic limit fo AWS 

### Unified topics for every "thing"

Use just `id`s in the topic for everything: including parent device, child devices and services.
with just the distinction between device or service as follows:
`tedge/<context>/<id>/...`

For device(parent or child): `tedge/device/<id>/...`
For services(on parent or child): `tedge/service/<id>/...`

Examples:
* Main device measurements: `tedge/device/<tedge-device-id>/telemetry/measurements`
* Main device service measurements: `tedge/service/<service-id>/telemetry/measurements`
* Child device measurements: `tedge/device/<child-device-id>/telemetry/measurements`
* Child device service measurements: `tedge/service/<service-id>/telemetry/measurements`

The `id` could be the main device id, service id or child device id.
The relation between the "parent and its child device
s" or "devices and the services linked to them"
are defined only during the bootstrapping phase.

```admonish note
`tedge/device/main` could be just used as an alias for `tedge/device/<id>` for simplicity.
```admonish note

**Pros**

1. Due to the lack of distinction in the topics between parent devices and child devices,
   it is easier to write code for "the device" irrespective of whether it is deployed as a parent or child.

**Cons**

1. Not easy to differentiate the context(from parent or child) easily from the message.
1. The `id`s must be unique between all devices and services in a deployment
1. Not easy to do subscriptions like: "measurements only from child devices or only from services, excluding parent"
1. No automatic registration as bootstrapping is mandatory
