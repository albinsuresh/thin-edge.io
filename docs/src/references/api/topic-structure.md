---
title: MQTT Topic Structure
tags: [Documentation]
sidebar_position: 1
---

# Examples

The following examples will demonstrate how the flexible topic structure can be used to provide rich modelling tools to publish data which is more observable to other components.

## Example: Default device/service topic semantics

### Publish to the main device

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///m/environment' '{
  "temperature": 23.4
}'
```

If the there is no measurement type, then the type can be left empty, but it must have the trailing slash `/` (so that the number of topic segments is the same).

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///m/' '{
  "temperature": 23.4
}'
```

### Publish to a child device

```sh te2mqtt
tedge mqtt pub -r 'te/child/child01///m/environment' '{
  "temperature": 23.4
}'
```

### Publish to a service on the main device

```sh te2mqtt
tedge mqtt pub -r 'te/device/main/service/nodered/m/environment' '{
  "temperature": 23.4
}'
```

Any MQTT client can subscribe to all measurements for all entities (devices and services) using the following MQTT topic:

```sh te2mqtt
tedge mqtt sub 'te/+/+/+/+/m/+'
```

If you want to be more specific and only subscribe to the main device, then you can used fixed topic names rather than wildcards:

```sh te2mqtt
tedge mqtt sub 'te/device/main/+/+/m/+'
```

Or to subscribe to a specific type of measurement published to an services on the main device, then use:

```sh te2mqtt
tedge mqtt sub 'te/device/main/service/+/m/memory'
```

### Publish to a service on a child device

```sh te2mqtt
tedge mqtt pub -r 'te/device/child01/service/nodered/m/environment' '{
  "temperature": 23.4
}'
```

### Check which entities have been registered

A list of registered devices can be retrieved by using a simple subscription:

```sh te2mqtt
tedge mqtt sub 'te/+/+'
```

Or if you want to listen to all devices and services, then use the following:

```sh te2mqtt
tedge mqtt sub 'te/+/+/+/+'
```

## Advanced

This section is only intended for usage for complex scenarios where the default MQTT topic structure is not suitable.

Check out the [Case Studies](./case-studies) page for more detailed examples in a real-world context.

### Using topic structure for custom grouping

Since the topic structure is flexible, it means that you can use it to represent anything you would like.

For instance, if you could use the topic names to represent the information about the equipment to ensure it is unique within the thin-edge.io root topic (`te/`) by using the manufacturer, device serial number (or model number), optional application name and application instance.

```text
te/{family}/{serial}/{application}/{instance}/{channel}
```


```sh te2mqtt
tedge mqtt pub -r 'te/flowserve/abcdef01234///e/pump_status' '{
  "text": "Pump is running"
}'
```

However for specific telemetry data you might want to be more specific.

```sh te2mqtt
tedge mqtt pub -r 'te/flowserve/abcdef01234/pumps/monitoring/e/pump_status' '{
  "text": "Pump is running"
}'
```

### Using component namespace to group data

Let's say you wanted to use an analytics engine to listen to specific measurements which should not be sent to the cloud as the analytics application will transform the raw values to an average which will then be published to the cloud.

ACL rules can be used to control which telemetry data is received by the analytics engine.

```sh te2mqtt
tedge mqtt pub -r 'te/flowserve/abcdef01234/analytics/average/m/flow_rate' '{
  "flow_rate": 0.01
}'
```

The analytics engine can easily subscribe to all measurements which are marked to be processed by the engine and also the type of aggregate that should be done (e.g. average, max or min).

```sh te2mqtt
tedge mqtt sub 'te/+/+/analytics/average/m/flow_rate' '{
  "flow_rate": 0.01
}'
```

Then the engine can publish the average flow rate to a new measurement type as follows:

```sh te2mqtt
tedge mqtt pub -r 'te/flowserve/abcdef01234///m/flow_rate_average' '{
  "flow_rate": 0.015
}'
```

## Message Transformations

The following message transformations are demonstrated using a device certificate with the 

```text title="Device certificate common name"
tedge_001
```

### Cumulocity External Identity

Assuming the following topic name:

```text
te/device/main///m/environment
```


The above topic can be converted to an external identity using the following steps:

1. Split the topic string by slash `/`

    ```json title="New result"
    ["te", "device", "main", "", "", "m", "environment"]
    ```

2. Only take the first 5 items of the array

    ```json title="New result"
    ["te", "device", "main", "", ""]
    ```

3. Remove any array items with an empty strings

    ```json title="New result"
    ["te", "device", "main"]
    ```

4. Replace the first item with the device certificate's common name (e.g. `te` &rarr; `tedge_001`)

    ```json title="New result"
    ["tedge_001", "device", "main"]
    ```

5. Create a string by joining each array item with a colon `:`

    ```text title="New result"
    tedge_001:device:main
    ```

6. Shorten specific identities (if necessary)

    There is a special case where the identity of `tedge_001:device:main` is shorted to just `tedge_001`, as this is done so that it matches the Common Name included in the certificate.

#### Example: Converting topic to external id using python

The transform can also be presented as a simple python function which takes the given topic name and the common name (from the device certificate), and returns a string with the external id used to represent the topic.

```py
def get_external_id(topic: str, common_name: str = "tedge_001") -> str:
    """Get the external id from the topic name
    """
    return ":".join([
        part
        for part in [common_name, *topic.split("/")[1:5]]
        if part
    ])
```

The function about can be used to verify the topic to name transformation, using the following assertions.

```py
# Main device
assert get_external_id("te/device/main///m/environment") == "tedge_001"

# Service on main device
assert get_external_id("te/device/main/service/nodered/m/environment") == "tedge_001:device:main:service:nodered"

# Child device
assert get_external_id("te/child/child01///m/environment") == "tedge_001:child:child01"

# Service on child device
assert get_external_id("te/child/child01/service/nodered/m/environment") == "tedge_001:child:child01:service:nodered"
```

### Measurements

#### Main device

```sh te2mqtt
tedge mqtt pub 'te/device/main///m/environment' '{"temperature":23.4}'
```

```json title="Output Topic: c8y/measurement/measurements/create"
{
  "externalSource": {
    "externalId": "tedge_001",
    "type": "c8y_Serial"
  },
  "temperature": {
    "temperature": {
      "unit": "",
      "value": 23.4
    }
  },
  "time": "2023-07-07T14:30:15.179701+02:00",
  "type": "environment"
}
```


<details>
<summary>Child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01///m/environment' '{"temperature":23.4}'
```

```json title="Output Topic: c8y/measurement/measurements/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01",
    "type": "c8y_Serial"
  },
  "temperature": {
    "temperature": {
      "unit": "",
      "value": 23.4
    }
  },
  "time": "2023-07-07T13:52:40.927797+02:00",
  "type": "environment"
}
```

</details>


<details>
<summary>Service of a main device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/m/environment' '{"temperature":23.4}'
```

```json title="Output Topic: c8y/measurement/measurements/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:main:service:nodered",
    "type": "c8y_Serial"
  },
  "temperature": {
    "temperature": {
      "unit": "",
      "value": 23.4
    }
  },
  "time": "2023-07-07T13:52:40.927797+02:00",
  "type": "environment"
}
```

</details>


<details>
<summary>Service of a child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01/service/nodered/m/environment' '{"temperature":23.4}'
```

```json title="Output Topic: c8y/measurement/measurements/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01:service:nodered",
    "type": "c8y_Serial"
  },
  "temperature": {
    "temperature": {
      "unit": "",
      "value": 23.4
    }
  },
  "time": "2023-07-07T13:52:40.927797+02:00",
  "type": "environment"
}
```
</details>

### Events

#### Main device

```sh te2mqtt
tedge mqtt pub 'te/device/main///e/flow_status' '{"text":"Low flow detected"}'
```

```json title="Output Topic: c8y/event/events/create"
{
  "externalSource": {
    "externalId": "tedge_001",
    "type": "c8y_Serial"
  },
  "text": "Low flow detected",
  "time": "2023-07-07T16:50:35.935371+02:00",
  "type": "flow_status"
}
```

<details>
<summary>Child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01///e/flow_status' '{"text":"Low flow detected"}'
```

```json title="Output Topic: c8y/event/events/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01",
    "type": "c8y_Serial"
  },
  "text": "Low flow detected",
  "time": "2023-07-07T16:50:35.935371+02:00",
  "type": "flow_status"
}
```

</details>

<details>
<summary>Service of a main device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/e/flow_status' '{"text":"Low flow detected"}'
```

```json title="Output Topic: c8y/event/events/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:main:service:nodered",
    "type": "c8y_Serial"
  },
  "text": "Low flow detected",
  "time": "2023-07-07T16:51:42.525765+02:00",
  "type": "flow_status"
}
```

</details>

<details open>
<summary>Service of a child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01/service/nodered/e/flow_status' '{"text":"Low flow detected"}'
```

```json title="Output Topic: c8y/event/events/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01:service:nodered",
    "type": "c8y_Serial"
  },
  "text": "Low flow detected",
  "time": "2023-07-07T16:51:42.525765+02:00",
  "type": "flow_status"
}
```

</details>

### Alarms

#### Main device

```sh te2mqtt
tedge mqtt pub 'te/device/main///a/disk_usage' '{"text":"Disk space is low"}'
```

```json title="Output Topic: c8y/alarm/alarms/create"
{
  "externalSource": {
    "externalId": "tedge_001",
    "type": "c8y_Serial"
  },
  "severity": "CRITICAL",
  "text": "Disk space is low",
  "time": "2023-07-07T16:57:48.193314+02:00",
  "type": "disk_usage"
}
```


<details>
<summary>Child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01///a/health' '{"text":"Service is stopped"}'
```

```json title="Output Topic: c8y/alarm/alarms/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01",
    "type": "c8y_Serial"
  },
  "severity": "CRITICAL",
  "text": "Service is stopped",
  "time": "2023-07-07T17:01:12.553921+02:00",
  "type": "health"
}
```

</details>


<details>
<summary>Service of a main device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/a/health' '{"text":"Service is stopped"}'
```

```json title="Output Topic: c8y/alarm/alarms/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:main:service:nodered",
    "type": "c8y_Serial"
  },
  "severity": "CRITICAL",
  "text": "Service is stopped",
  "time": "2023-07-07T17:01:12.553921+02:00",
  "type": "health"
}
```

</details>

<details>
<summary>Service of a child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01/service/nodered/a/health' '{"text":"Service is stopped"}'
```

```json title="Output Topic: c8y/alarm/alarms/create"
{
  "externalSource": {
    "externalId": "tedge_001:device:child01:service:nodered",
    "type": "c8y_Serial"
  },
  "severity": "CRITICAL",
  "text": "Service is stopped",
  "time": "2023-07-07T17:01:12.553921+02:00",
  "type": "health"
}
```

</details>

### Data (inventory data)

#### Main device

```sh te2mqtt
tedge mqtt pub 'te/device/main///data/os_information' '{
  "family":"Debian",
  "codename":"bullseye",
  "version":"11"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:main"
{
  "os_information": {
    "codename": "bullseye",
    "family": "Debian",
    "version": "11"
  }
}
```


<details>
<summary>Child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01///data/os_information' '{
  "family":"Debian",
  "codename":"bullseye",
  "version":"11"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:child01"
{
  "os_information": {
    "codename": "bullseye",
    "family": "Debian",
    "version": "11"
  }
}
```

</details>


<details>
<summary>Service of a main device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/data/os_information' '{
  "family":"Debian",
  "codename":"bullseye",
  "version":"11"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:main:service:nodered"
{
  "os_information": {
    "codename": "bullseye",
    "family": "Debian",
    "version": "11"
  }
}
```

</details>


<details>
<summary>Service of a child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01/service/nodered/data/os_information' '{
  "family":"Debian",
  "codename":"bullseye",
  "version":"11"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:child01:service:nodered"
{
  "os_information": {
    "codename": "bullseye",
    "family": "Debian",
    "version": "11"
  }
}
```

</details>


#### Updating root fragment

If a fragments on the root level need to be updated, then leave the `type` topic segment blank, and it will apply all of the given property on the root level.

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/data/' '{"displayName":"My Custom Name"}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:main"
{
  "displayName": "My Custom Name"
}
```

<details>
<summary>Child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01///data/container_runtime' '{
  "status":"running"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:child01"
{
  "container_runtime": {
    "status": "running"
  }
}
```

</details>


<details>
<summary>Service of a main device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/main/service/nodered/data/container_runtime' '{
  "status":"running"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:main:service:nodered"
{
  "container_runtime": {
    "status": "running"
  }
}
```

</details>


<details>
<summary>Service of a child device</summary>

```sh te2mqtt
tedge mqtt pub 'te/device/child01/service/nodered/data/container_runtime' '{
  "status":"running"
}'
```

```json title="Output Topic: c8y/inventory/managedObjects/update/tedge_001:device:child01:service:nodered"
{
  "container_runtime": {
    "status": "running"
  }
}
```

</details>

### Operations

The operation translation is cloud specific. The following example demonstrate how operations from Cumulocity IoT can be transformed to local thin-edge operations.

The cloud specific mapper will have to translate known operation types to the local types. The examples use the following operation type mapping.

|Cloud type|thin-edge type|
|----|-----|
|`c8y_Command`|`execute_shell`|

#### Main device

```sh te2mqtt
tedge mqtt pub 'c8y/devicecontrol/notifications' '{
  "c8y_Command":{
    "text":"ls -l"
  },
  "externalSource":{
    "externalId":"tedge_001"
  },
  "id":"12345",
  "status":"PENDING"
}'
```

```json title="Output Topic: te/device/main///cmd/execute_shell/12345"
{
  "id": "12345",
  "command": "ls -l",
  "status":"pending"
}
```

#### Child device

```sh te2mqtt
tedge mqtt pub 'c8y/devicecontrol/notifications' '{
  "c8y_Command":{
    "text":"ls -l"
  },
  "externalSource":{
    "externalId":"tedge_001:device:child01"
  },
  "id":"12345",
  "status":"PENDING"
}'
```

```json title="Output Topic: te/device/child01///cmd/execute_shell/12345"
{
  "id": "12345",
  "command": "ls -l",
  "status":"pending"
}
```

#### Service of a main device

```sh te2mqtt
tedge mqtt pub 'c8y/devicecontrol/notifications' '{
  "c8y_Command":{
    "text":"ls -l"
  },
  "externalSource":{
    "externalId":"tedge_001:device:main:service:nodered"
  },
  "id":"12345",
  "status":"PENDING"
}'
```

```json title="Output Topic: te/device/main/service/nodered/cmd/execute_shell/12345"
{
  "id": "12345",
  "command": "ls -l",
  "status":"pending"
}
```

#### Service of a child device

```sh te2mqtt
tedge mqtt pub 'c8y/devicecontrol/notifications' '{
  "c8y_Command":{
    "text":"ls -l"
  },
  "externalSource":{
    "externalId":"tedge_001:device:child01:service:nodered"
  },
  "id":"12345",
  "status":"PENDING"
}'
```

```json title="Output Topic: te/device/child01/service/nodered/cmd/execute_shell/12345"
{
  "id": "12345",
  "command": "ls -l",
  "status":"pending"
}
```

### Other operations

#### Restart

```sh te2mqtt
tedge mqtt pub 'c8y/devicecontrol/notifications' '{
  "c8y_Restart":{},
  "externalSource":{
    "externalId":"tedge_001"
  },
  "id":"12345",
  "status":"PENDING"
}'
```

```json title="Output Topic: te/device/main///cmd/restart/12345"
{
  "id": "12345",
  "status":"pending"
}
```

## Summary

### Advantages

* Get the entity/component list out of the box
  * User just has to publish retain messages, e.g. publish to `te/device/main` or `te/device/main/service/nodered`

* Normalized topic structure. This makes it easier for other components to observe the data

* Decouple topic hierarchy from entity hierarchy
  * Multiple devices could publish to the same cloud entity (if you wanted), yet keeping the data separate on the local MQTT broker

* Enable users to define their own semantic meanings to the 4-segment topic hierarchy

* Also allows for entity/component inference when using configurable (magic) namespace names which will map a namespace to an entity type


### Disadvantages

* Longer topic structure

## Open Questions

1. What topic should `tedge/errors` messages be published to using the new topic structure?

2. How to handle the `tedge/health-check/<tedge-service-name>` topic which triggers

3. Where should the `tedge/health/<service>` topic be mapped to in the `te/` namespace?

    **Option 1: In registration topic**

    It only assumes that the data owner is the only client allowed to publish to it, because it needs to know information about itself, and it has to resend already known information (e.g. the "@parent" device, "@type" etc.)

    * displayName
    * @type
    * @parent

    **Option 2: Under health telemetry point, e.g. static information**

    Decouple runtime information from the registration so that the service updating the status does not have to keep publishing new information.
    
    For example, the last will message is a static message which is configured to send the status "down" information when the service's associated MQTT client is disconnected. Since the last will and testament message is sent by the server, the message contents have to be known at the time of the MQTT client connection, and therefore there is a greater chance that the message contains out-of-date information if the registration information changes over the service's runtime.

4. Example showing how to use custom segments (e.g. turn off auto-registration and allow users to configure their own setup)
    * Two entities/components publishing to the same cloud entity
    * Custom names

5. What format should the `@parent` device be referred to, the `@id` or the topic name?

  For example:
  * `te/device/nested_child01`
  or 
  * `te:device:nested_child01`

  ```json
  "te/device/nested_child01/service/nodered": {
    "@type": "service",
    "@parent": "te/device/nested_child01",
    "displayName": "nodered",
    "type": "systemd"
  }
  ```

6. Device registration: Should users be allows to register a component under `te/my/component`, a topic which is normally reserved for devices?

7. How to register supported operations

    ```sh te2mqtt
    tedge mqtt pub 'te/tedge/main///cmd/software_update/meta' '{}'
    ```

    Other clients can subscribe to the following topic to check which operations are supported by each entity/component.

    ```sh te2mqtt
    tedge mqtt sub 'te/+/+/+/+/cmd/+/meta' '{}'
    ```
