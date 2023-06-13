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

## Requirements

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

### Out of scope

1. Routing different kinds of data to different clouds, e.g: all telemetry to azure and all commands from/to Cumulocity.
   Even though this requirement is realistic, thin-edge MQTT topics must not be corrupted with cloud specific semantics,
   and the same requirement will be handled with some external routing mechanism(e.g: routing via bridge topics)
1. 

## Proposals

### Dedicated topics for tedge device, services and child devices

For parent:  tedge/main/
For service:  tedge/service/<service-id>
For child: tedge/child/<child-id>

**Pros**

1. 

**Cons**

1.

### Unified topics for every "thing"

For everything: `tedge/<id>/...`

The `id` could be a device id, service id or child device id.
The relation between who's the parent/child/service is defined only during the bootstrapping phase.

`tedge/main` could be just used as an alias for `tedge/<tedge-device-id>`.

**Pros**

1. The same code works everywhere, irrespective of context

**Cons**

1. Not easy to differentiate the context easily from the message
1. Unable to filter messages like: only from child devices  or only from services, excluding parent
1. Bootstrapping becomes more critical
1. 

