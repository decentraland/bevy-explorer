use bevy::platform::sync::Arc;
use serde::{Deserialize, Deserializer};
use wasm_bindgen::JsValue;

pub trait GetFromJsValue {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self>
    where
        Self: Sized;
}

impl GetFromJsValue for String {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .and_then(|topic| topic.as_string())
    }
}

impl GetFromJsValue for Arc<Vec<u8>> {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        js_sys::Reflect::get(js_value, &JsValue::from(key))
            .ok()
            .and_then(|payload| serde_wasm_bindgen::from_value::<PayloadIntermediate>(payload).ok())
            .map(|payload| payload.0)
    }
}

impl<T: From<JsValue>> GetFromJsValue for Vec<T> {
    fn get_from_js_value(js_value: &JsValue, key: &str) -> Option<Self> {
        let value = js_sys::Reflect::get(js_value, &JsValue::from(key)).ok()?;
        let array = js_sys::Array::from(&value).to_vec();
        Some(array.into_iter().map(|v| T::from(v)).collect::<Self>())
    }
}

#[derive(Debug)]
struct PayloadIntermediate(Arc<Vec<u8>>);

impl<'de> Deserialize<'de> for PayloadIntermediate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = serde_bytes::ByteBuf::deserialize(deserializer)?;
        Ok(Self(Arc::new(buf.into_vec())))
    }
}
