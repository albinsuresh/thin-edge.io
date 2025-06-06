---
title: The Mapper
tags: [Concept, Cloud, MQTT]
sidebar_position: 6
description: Overview of the core component which interfaces with the cloud
---

The tedge-mapper is a key concept to support multiple cloud providers.
The purpose is to translate
messages written using the cloud-agnostic [%%te%% JSON format](thin-edge-json.md),
into cloud-specific messages.

The tedge-mapper is composed of multiple cloud-specific mappers, such as Cumulocity mapper and Azure mapper.
Each mapper is responsible for its dedicated cloud.
These specific mappers are launched by the respective `tedge connect` command.
For instance, `tedge connect c8y` establishes a bridge to Cumulocity and launches a Cumulocity mapper
that translates the messages in the background.

A mapper subscribes to the reserved MQTT topic `te/+/+/+/+/m/+` with the QoS level 1 (at least once).
The messages that arrive in the mapper should be formed in the [%%te%% JSON](thin-edge-json.md) format.
The mapper verifies whether the arrived messages are correctly formatted,
in case the verification fails, the mapper publishes a corresponded error message
on the topic `te/errors` with the QoS level 1 (at least once).

When the mapper receives a correctly formatted message,
the message will be translated into a cloud-specific format.

## Cumulocity mapper

The Cumulocity mapper translates [%%te%% JSON](thin-edge-json.md) into Cumulocity's [JSON via MQTT](https://cumulocity.com/docs/smartrest/json-via-mqtt/).
The translated messages are published on the topic `c8y/measurement/measurements/create` from where they are forwarded to Cumulocity.
This mapper is launched by the `tedge connect c8y` command, and stopped by the `tedge disconnect c8y` command.

Example in %%te%% JSON:

```json
{
  "temperature": 23
}
```

Translated into JSON via MQTT by the Cumulocity mapper:

```json
{
  "type": "ThinEdgeMeasurement",
  "time": "2021-04-22T17:05:26.958340390+00:00",
  "temperature": {
    "temperature": {
      "value": 23
    }
  }
}
```

You can see the Cumulocity mapper added the three things which are not defined before translation.

1. `type` is added.
2. `time` is added.
3. Another hierarchy level is added, as required by the cumulocity data model.
String `temperature` is used as fragment and series.

(1) The `type` is a mandatory field in the Cumulocity's JSON via MQTT manner,
therefore, the Cumulocity mapper uses the user provided type from the topic, and if it is empty then it uses a default value of `ThinEdgeMeasurement`.

(2) `time` will be added by the mapper **only when it is not specified in a received %%te%% JSON message**.
In this case, the mapper uses the device's local timezone. If you want another timezone, specify the time filed in %%te%% JSON.

(3) The mapper uses a measurement name ("temperature" in this example)
as both a fragment type and a fragment series in [Cumulocity's measurements](https://cumulocity.com/docs/concepts/domain-model/#measurements).

After the mapper publishes a message on the topic `c8y/measurement/measurements/create`,
the message will be transferred to the topic `measurement/measurements/create` by [the MQTT bridge](../references/mappers/mqtt-topics.md).

### For child devices

The Cumulocity mapper collects measurements not only from the main device but also from child devices.
These measurements are collected under the `te/device/<child-id>///m/+` topics and forwarded to Cumulocity to corresponding child devices created under the %%te%% parent device.
(`<child-id>` is your desired child device ID.)

The mapper works in the following steps.

1. When the mapper receives a %%te%% JSON message on the `te/device/<child-id>///m/+` topic,
   the mapper sends a request to create a child device under the %%te%% parent device.
   The child device is named after the `<child-id>` topic name, and the type is `thin-edge.io-child`.
2. Publish corresponded Cumulocity JSON measurements messages over MQTT.
3. The child device is created on receipt of the very first measurement for that child device.

If the incoming %%te%% JSON message (published on `te/device/child1///m/`) is as follows,

```json
{
  "temperature": 23
}
```

it gets translated into JSON via MQTT by the Cumulocity mapper.

```json
{
  "type":"ThinEdgeMeasurement",
  "externalSource":{
    "externalId":"child1",
    "type":"c8y_Serial"
  },
  "time":"2013-06-22T17:03:14+02:00",
  "temperature":{
    "temperature":{
      "value":23
    }
  }
}
```

## Azure IoT Hub mapper

The Azure IoT Hub mapper takes messages formatted in the [%%te%% JSON](thin-edge-json.md) as input.
It validates if the incoming message is correctly formatted %%te%% JSON, then outputs the message.
The validated messages are published on the topic `az/messages/events/` from where they are forwarded to Azure IoT Hub.
This mapper is launched by the `tedge connect az` command, and stopped by the `tedge disconnect az` command.

The Azure IoT Hub Mapper processes a message in the following ways.

1. Validates if it is a correct %%te%% JSON message or not.
2. Validates the incoming message size is below 255 KB.
[The size of all device-to-cloud messages must be up to 256 KB](https://learn.microsoft.com/en-us/azure/iot-hub/iot-hub-devguide-d2c-guidance).
The mapper keeps 1 KB as a buffer for the strings added by Azure.
3. (default) Adds a current timestamp if a timestamp is not included in an incoming message. To stop this behavior, please refer to the following instruction.

So, if the input is below,

```json
{
  "temperature": 23
}
```

the output of the mapper is:

```json title="Transformed message"
{
  "temperature": 23,
  "time": "2021-06-01T17:24:48.709803664+02:00"
}
```

### Configure whether adding a timestamp or not

However, if you don't want to add a timestamp in the output of Azure IoT Hub Mapper, you can change the behavior by running this:

```sh
sudo tedge config set az.mapper.timestamp false 
```

After changing the configuration, you need to restart the mapper service by:

```sh
sudo systemctl restart tedge-mapper-az
```

### Timestamp output format

The format that is used for the `time` field can be configured to either a UNIX timestamp (a number of seconds since the
UNIX epoch), or RFC-3339 using the configuration value `az.mapper.timestamp_format` to either `unix` or `rfc-3339`.
This setting affects not only the timestamps added by the mapper, but it will also transform the existing `time` field
to the specified format.

## AWS mapper

The AWS mapper takes messages formatted in the [%%te%% JSON](thin-edge-json.md) as input.
It validates if the incoming message is correctly formatted %%te%% JSON, then outputs the message.
The validated messages are published on the topic `aws/td/#` from where they are forwarded to AWS.
This mapper is launched by the `tedge connect aws` command, and stopped by the `tedge disconnect aws` command.

## Error cases

When some error occurs in a mapper process, the mapper publishes a corresponded error message
on the topic `te/errors` with the QoS level 1 (at least once).

Here is an example if you publish invalid %%te%% JSON messages on `te/+/+/+/+/m/+`:

```sh
tedge mqtt pub te/device/main///m/ '{"temperature": 23,"pressure": 220'
tedge mqtt pub te/device/main///m/ '{"temperature": 23,"time": "oops"}'
```

Then, you'll receive error messages from the mapper on the topic `te/errors`:

```sh te2mqtt formats=v1
tedge mqtt sub te/errors
```

```log title="Output"
[te/errors] Failed to convert a message on topic 'te/device/main///m/': Invalid JSON: EOF while parsing an object at line 1 column 34: `0`
[te/errors] Failed to convert a message on topic 'te/device/main///m/': Invalid JSON: Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): "oops": the 'year' component could not be parsed at line 1 column 33: `"}
```
