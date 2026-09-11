use serde_json::Value;

pub(crate) fn json_i32(value: &Value, key: &str) -> Option<i32> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

pub(crate) fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

// pub(crate) fn json_string(value: &Value, key: &str) -> Option<String> {
//     value.get(key).and_then(Value::as_str).map(str::to_owned)
// }

pub(crate) fn json_i32_array(value: &Value, key: &str) -> Vec<i32> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_i64)
                .filter_map(|item| i32::try_from(item).ok())
                .collect()
        })
        .unwrap_or_default()
}
