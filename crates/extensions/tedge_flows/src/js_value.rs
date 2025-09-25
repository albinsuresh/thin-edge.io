use crate::flow::DateTime;
use crate::flow::FlowError;
use crate::flow::Message;
use rquickjs::Ctx;
use rquickjs::FromJs;
use rquickjs::IntoJs;
use rquickjs::Value;
use std::collections::BTreeMap;

/// Akin to serde_json::Value with an extra case for binary data
#[derive(Clone, Debug)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Bytes(Vec<u8>), // <= This case motivates the use of JsonValue vs serde_json::Value
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl Default for JsonValue {
    fn default() -> Self {
        JsonValue::Object(Default::default())
    }
}

impl JsonValue {
    fn string(value: impl ToString) -> Self {
        JsonValue::String(value.to_string())
    }

    fn number(value: impl Into<serde_json::Number>) -> Self {
        JsonValue::Number(value.into())
    }

    fn option(value: Option<impl Into<JsonValue>>) -> Self {
        value.map(|v| v.into()).unwrap_or(JsonValue::Null)
    }

    fn object<T, K>(values: T) -> Self
    where
        T: IntoIterator<Item = (K, JsonValue)>,
        K: ToString,
    {
        let object = values
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        JsonValue::Object(object)
    }
}

impl From<Message> for JsonValue {
    fn from(value: Message) -> Self {
        let raw_payload = JsonValue::Bytes(value.payload.clone());
        let payload = match String::from_utf8(value.payload) {
            Ok(utf8) => JsonValue::string(utf8),
            Err(_) => JsonValue::Null,
        };
        JsonValue::object([
            ("topic", JsonValue::string(value.topic)),
            ("payload", payload),
            ("raw_payload", raw_payload),
            ("timestamp", JsonValue::option(value.timestamp)),
        ])
    }
}

impl From<DateTime> for JsonValue {
    fn from(value: DateTime) -> Self {
        JsonValue::object([
            ("seconds", JsonValue::number(value.seconds)),
            ("nanoseconds", JsonValue::number(value.nanoseconds)),
        ])
    }
}

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValue::Null,
            serde_json::Value::Bool(b) => JsonValue::Bool(b),
            serde_json::Value::Number(n) => JsonValue::Number(n),
            serde_json::Value::String(s) => JsonValue::String(s),
            serde_json::Value::Array(a) => {
                JsonValue::Array(a.into_iter().map(JsonValue::from).collect())
            }
            serde_json::Value::Object(o) => {
                JsonValue::object(o.into_iter().map(|(k, v)| (k, JsonValue::from(v))))
            }
        }
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => serde_json::Value::Null,
            JsonValue::Bool(b) => serde_json::Value::Bool(b),
            JsonValue::Number(n) => serde_json::Value::Number(n),
            JsonValue::String(s) => serde_json::Value::String(s),
            JsonValue::Bytes(b) => serde_json::Value::String(format!("0x {b:?}")),
            JsonValue::Array(a) => {
                serde_json::Value::Array(a.into_iter().map(serde_json::Value::from).collect())
            }
            JsonValue::Object(o) => serde_json::Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect(),
            ),
        }
    }
}

impl TryFrom<BTreeMap<String, JsonValue>> for Message {
    type Error = FlowError;

    fn try_from(value: BTreeMap<String, JsonValue>) -> Result<Self, Self::Error> {
        let Some(JsonValue::String(topic)) = value.get("topic") else {
            return Err(anyhow::anyhow!("Missing message topic").into());
        };
        let payload = match value.get("payload") {
            Some(JsonValue::String(payload)) => payload.to_owned().into_bytes(),
            Some(JsonValue::Bytes(payload)) => payload.to_owned(),
            _ => return Err(anyhow::anyhow!("Missing message payload").into()),
        };
        let timestamp = value
            .get("timestamp")
            .map(|t| DateTime::try_from(t.clone()))
            .transpose()?;

        Ok(Message {
            topic: topic.to_owned(),
            payload,
            timestamp,
        })
    }
}

impl TryFrom<JsonValue> for Message {
    type Error = FlowError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        let JsonValue::Object(object) = value else {
            return Err(
                anyhow::anyhow!("Expect a message object with a topic and a payload").into(),
            );
        };
        Message::try_from(object)
    }
}

impl TryFrom<JsonValue> for DateTime {
    type Error = FlowError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        let JsonValue::Object(object) = value else {
            return Err(
                anyhow::anyhow!("Expect a timestamp object with seconds and nanoseconds").into(),
            );
        };
        DateTime::try_from(object)
    }
}

impl TryFrom<BTreeMap<String, JsonValue>> for DateTime {
    type Error = FlowError;

    fn try_from(value: BTreeMap<String, JsonValue>) -> Result<Self, Self::Error> {
        let Some(JsonValue::Number(seconds)) = value.get("seconds") else {
            return Err(anyhow::anyhow!("Missing timestamp seconds").into());
        };
        let Some(JsonValue::Number(nanoseconds)) = value.get("nanoseconds") else {
            return Err(anyhow::anyhow!("Missing timestamp nanoseconds").into());
        };

        Ok(DateTime {
            seconds: seconds.as_u64().unwrap_or_default(),
            nanoseconds: nanoseconds.as_u64().unwrap_or_default() as u32,
        })
    }
}

impl TryFrom<JsonValue> for Vec<Message> {
    type Error = FlowError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value {
            JsonValue::Array(array) => array.into_iter().map(Message::try_from).collect(),
            JsonValue::Object(object) => Message::try_from(object).map(|message| vec![message]),
            JsonValue::Null => Ok(vec![]),
            _ => Err(
                anyhow::anyhow!("Flow scripts are expected to return an array of messages").into(),
            ),
        }
    }
}

struct JsonValueRef<'a>(&'a JsonValue);

impl<'js> IntoJs<'js> for JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(&self).into_js(ctx)
    }
}

impl<'js> IntoJs<'js> for &JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(self).into_js(ctx)
    }
}

impl<'js> IntoJs<'js> for JsonValueRef<'_> {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self.0 {
            JsonValue::Null => Ok(Value::new_null(ctx.clone())),
            JsonValue::Bool(value) => Ok(Value::new_bool(ctx.clone(), *value)),
            JsonValue::Number(value) => {
                if let Some(n) = value.as_i64() {
                    if let Ok(n) = i32::try_from(n) {
                        return Ok(Value::new_int(ctx.clone(), n));
                    }
                }
                if let Some(f) = value.as_f64() {
                    return Ok(Value::new_float(ctx.clone(), f));
                }
                let nan = rquickjs::String::from_str(ctx.clone(), "NaN")?;
                Ok(nan.into_value())
            }
            JsonValue::String(value) => {
                let string = rquickjs::String::from_str(ctx.clone(), value)?;
                Ok(string.into_value())
            }
            JsonValue::Bytes(value) => {
                let bytes = rquickjs::TypedArray::new(ctx.clone(), value.clone())?;
                Ok(bytes.into_value())
            }
            JsonValue::Array(values) => {
                let array = rquickjs::Array::new(ctx.clone())?;
                for (i, value) in values.iter().enumerate() {
                    array.set(i, JsonValueRef(value))?;
                }
                Ok(array.into_value())
            }
            JsonValue::Object(values) => {
                let object = rquickjs::Object::new(ctx.clone())?;
                for (key, value) in values.iter() {
                    object.set(key, JsonValueRef(value))?;
                }
                Ok(object.into_value())
            }
        }
    }
}

impl<'js> FromJs<'js> for JsonValue {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        JsonValue::from_js_value(value)
    }
}

impl JsonValue {
    fn from_js_value(value: Value<'_>) -> rquickjs::Result<Self> {
        if let Some(promise) = value.as_promise() {
            // Beware checking the value is a promise must be done first
            // as a promise can also be used as an object
            return promise.finish();
        }
        if let Some(b) = value.as_bool() {
            return Ok(JsonValue::Bool(b));
        }
        if let Some(n) = value.as_int() {
            return Ok(JsonValue::Number(n.into()));
        }
        if let Some(n) = value.as_float() {
            let js_n = serde_json::Number::from_f64(n)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null);
            return Ok(js_n);
        }
        if let Some(object) = value.as_object() {
            if let Some(bytes) = object.as_typed_array::<u8>() {
                let bytes = bytes.as_bytes().unwrap_or_default().to_vec();
                return Ok(JsonValue::Bytes(bytes));
            }
        }
        if let Some(string) = value.as_string() {
            return Ok(JsonValue::String(string.to_string()?));
        }
        if let Some(array) = value.as_array() {
            let mut js_array = Vec::new();
            for v in array.iter() {
                js_array.push(JsonValue::from_js_value(v?)?)
            }
            return Ok(JsonValue::Array(js_array));
        }
        if let Some(object) = value.as_object() {
            let mut js_object = BTreeMap::new();
            for key in object.keys::<String>().flatten() {
                if let Ok(v) = object.get(&key) {
                    js_object.insert(key, JsonValue::from_js_value(v)?);
                }
            }
            return Ok(JsonValue::Object(js_object));
        }

        Ok(JsonValue::Null)
    }

    pub(crate) fn display(value: Value<'_>) -> String {
        let json = serde_json::Value::from(JsonValue::from_js_value(value).unwrap_or_default());
        serde_json::to_string_pretty(&json).unwrap()
    }
}
