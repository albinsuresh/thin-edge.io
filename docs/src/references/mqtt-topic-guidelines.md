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

## Domain model influence

The MQTT topics and interactions are modelled around the following entities:

1. **Device**
   A device can be viewed as a composition of hardware components and firmware/software components running on it.
   The device can extract data from these hardware components and emit that as telemetry data using some software on it.
   The device can also control these hardware components using some software.
   The device also manages the firmware and all the software running on it.
   A device could be connected to many other devices as well as multiple cloud providers/instances.
1. **Tedge device**
   The gateway device that is connected to the cloud, where thin-edge.io is installed,
   which emits its own telemetry data and can receive commands to perform operations on the device.
2. **Child device**
   Typically, a different device that is connected to the tedge device, which has its own identity
   which is separate from the tedge device itself.
   It also emits its own measurements and can receive commands.
   A child device need not always be a a physical device, but can also be a logical one,
   abstracting some parts or groups of parts of the main device as well, with its own identity.
   A child device can have further nested child devices as well.
3. **Service**
   A service can be a component or a piece of software running on a device (tedge device or child device).
   For example, a device can have a cloud connector/agent software that can be viewed as a service.
   Any software on the device can be modelled as a service, if monitoring them separately from the device makes sense.
   A service can have its own telemetry data which is separate from the device's telemetry data.
   For e.g: a service running on a device can report its own RAM or disk usage,
   which is separate from the device's overall RAM or disk usage.
   But services are managed (installed, updated etc) by the device that it is running on and hence
   services not support commands on their own.
   All commands meant for a service are received and managed by the device that it is running on.
   For e.g: It would be much easier for the device to update/uninstall a service than expecting the service to update itself.
   But, thin-edge does not completely rule out the possibility of services supporting commands as well, in future.
   Unlike devices that only has a connectivity status, services have a liveness aspect to them as well,
   which conveys if a service is up and running at any given point of time.
   The liveness of services could be critical to the functioning of that device.
   a service does not support nested services.
   When a service is removed from a device, all telemetry data associated with it are obsolete as well, and hence removed.

When a thin-edge device receives telemetry data for itself or child devices or services running on any of them,
it needs to identify their source so that they can be associated with their twins on the device as well as in the cloud.
For all the MQTT communication, this source info can be part of either the topic or the payload.
Having them in the topics is desired, as that enables easy filtering of messages for a given device or a subset of devices.

## Use-cases

1. Support for nested child devices.
   A deployment where a gateway device is connected to a PLC which is further connected to other sensors/devices is very common.
   There are 3 levels of devices even in this simple deployment which a user might want to replicate in his cloud view as well.
   More complex deployments where a PLC is further connected to more PLCs which are further connected to leaf sensors/actuators
   would require even more levels of nesting.
1. Monitor the liveness of a piece of software running on a device (tedge or child) from the cloud.
1. Gather the usage metrics of a software component(service) running on a device as measurements pushed to the cloud,
   associated to an entity representing that software linked to that device, and not the device itself.
   An identity separate from the device is key here to ease the management of that component from the cloud.
   It is also required so that when that component is removed from the device, all data associated with it is removed as well.
   It must be linked to a device as software component does not have independent existence and is managed by a device.
   When a device is removed, all services linked to it are removed as well, as they're obsolete without the device.
1. When data from services, connected child devices and even the tedge device itself, are flowing through the MQTT broker,
   it must be easy to identify and filter the messages based on the source.
   A few examples of filtering queries are:
   * All measurements from a specific service
   * All measurements from the tedge device only, excluding the ones from other services and child devices
   * All measurements from all connected child devices
   * All measurements from everything (the tedge device itself, its services and child devices and their services)
1. Service ids must be namespaced under each device as it is highly likely that
   the same type of service may have the same name on multiple/all devices.
   It would be really difficult to maintain uniqueness in service name across an large fleet of devices.
1. Child device ids also must be namespaced under their direct parent
   so that conflicts can be avoided even if another parent device has child devices with the same id.
   It is okay to expect all devices connected to a device to have unique ids,
   but expecting those to be unique across an entire fleet could be far-fetched.
1. When multiple child devices are connected to a tedge device,
   a given child device should only be able to send/receive data meant for itself and not a sibling child device.
   Thin-edge must provide this isolation in such a way that the peer child devices can not even view others' data.
1. Connect to multiple cloud instances at the same time.
   This is a common deployment model for SEMs, where the devices that they produce are labelled and sold by solution providers to end customers.
   The device will be connected to the solution provider's cloud to gather all business data from the customer.
   But, it might have to be connected simultaneously to the SEMs cloud as well so that the SEM can remotely manage that device (for maintenance).

## Requirements

This section is divided into 3 parts:
1. Must-have: for the requirements that must be met by the proposed solutions
2. Nice-to-have: these requirements are not mandatory, but the solution addressing more of these requirements would be a plus
3. Out-of-scope: those that are relevant but out of scope for this design exercise

### Must-have

1. The topics must capture the source of that data that is exchanged through that message:
   whether it came from the tedge device, a child device or a service on any one of them.
1. A topic structure to receive telemetry data from services running on a device (main or child),
   which can't be associated to the main device or some child device,
   but must be associated to a logical service entity that is linked to that device.
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
1. Ability to run multiple thin-edge instances connected to the same remote MQTT broker but managing their own set of isolated child devices.

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
1. The topics are fairly long and with extensions might easily cross the 7 sub-topic limit of AWS.

### Unified topics for every "thing"

Use just `id`s in the topic for everything: including parent device, child devices and services.
with just the distinction between device or service as follows:
`tedge/<context>/<id>/...`

For tedge device: `tedge/main-device/<id>/...`
For child device: `tedge/child-device/<id>/...`
For services on tedge device: `tedge/main-service/<id>/...`
For services on child device: `tedge/child-service/<id>/...`

Examples:
* Main device measurements: `tedge/device/<tedge-device-id>/telemetry/measurements`
* Main device service measurements: `tedge/service/<service-id>/telemetry/measurements`
* Child device measurements: `tedge/device/<child-device-id>/telemetry/measurements`
* Child device service measurements: `tedge/service/<service-id>/telemetry/measurements`

The `id` could be the main device id, service id or child device id.
The relation between the "parent and its child devices" or "devices and the services linked to them"
are defined only during the bootstrapping phase.

```admonish note
`tedge/device/main` could be just used as an alias for `tedge/device/<id>` for simplicity.
```admonish note

**Pros**

1. Due to the lack of distinction in the topics between parent devices and child devices,
   it is easier to write code for "the device" irrespective of whether it is deployed as a parent or child.

**Cons**

1. Not easy to differentiate the context(from parent or child) easily from the message.
1. The `id`s must be unique between all devices and services in a deployment.
1. Not easy to do subscriptions like: "measurements only from child devices or only from services, excluding parent"
1. Difficult to do any subscriptions like "measurements from all services on a given child device",
   without keeping track of all the `id`s of services registered with that child device.
   Wild card subscriptions are not possible at all.
1. No automatic registration as bootstrapping is mandatory
