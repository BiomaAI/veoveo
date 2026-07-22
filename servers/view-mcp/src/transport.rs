use serde::{Deserialize, Deserializer, de::DeserializeOwned};

pub(crate) fn deserialize_structured<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Structured<T> {
        Value(T),
        Json(String),
    }

    match Structured::<T>::deserialize(deserializer)? {
        Structured::Value(value) => Ok(value),
        Structured::Json(json) => serde_json::from_str(&json).map_err(serde::de::Error::custom),
    }
}
