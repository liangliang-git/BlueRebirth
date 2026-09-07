use serde_json::{json, Value};

use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let now = current_unix_seconds();
    let uid = account
        .get("character")
        .and_then(|value| json_u64(value, "uid"))
        .unwrap_or(1);

    match method {
        "chat.ChatInfo" => reply(method, chat_info_payload(account)),
        "chat.ChangeWorldChannel" => {
            let channel = match ChangeWorldChannelRequest::decode(request_args) {
                Ok(request) => request.channel,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest("chat channel"));
                }
            };
            let chat = chat_state_mut(account);
            chat["channel"] = json!(channel);
            reply(method, varint_payload(1, channel as u64))
        }
        "chat.SendMessage" => {
            let request = match SendMessageRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest("chat message"));
                }
            };
            let message = request.message;
            let channel = request.channel;
            let receive_uid = request.receive_uid;
            let msg_type = request.message_type;
            let voice = request.voice;
            let entry = json!({
                "uid": uid,
                "channel": channel,
                "receiveUid": receive_uid,
                "message": message,
                "msgType": msg_type,
                "voice": voice,
                "sendTime": now,
            });
            let chat = chat_state_mut(account);
            let messages = chat
                .get_mut("messages")
                .and_then(Value::as_array_mut)
                .expect("chat messages must be an array");
            messages.push(entry.clone());
            if messages.len() > 100 {
                let excess = messages.len() - 100;
                messages.drain(..excess);
            }
            let payload = chat_message_payload(account, &entry);
            append_method_push(context.post_pushes, "chat.NewMessage", payload);
            let mut response = Vec::new();
            append_bytes_field(&mut response, 1, message.as_bytes());
            reply(method, response)
        }
        "chat.SendBarrage" => {
            let request = match SendBarrageRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest("barrage"));
                }
            };
            let SendBarrageRequest {
                id,
                offset,
                content,
            } = request;
            let chat = chat_state_mut(account);
            let barrages = chat
                .get_mut("barrages")
                .and_then(Value::as_array_mut)
                .expect("chat barrages must be an array");
            barrages.push(json!({
                "id": id,
                "offset": offset,
                "content": content,
                "uid": uid,
                "time": now,
            }));
            reply(method, Vec::new())
        }
        "chat.GetBarrageById" => {
            let id = decode_varint_field(request_args, 1);
            let begin = decode_varint_field(request_args, 2).max(0) as usize;
            let len = decode_varint_field(request_args, 3).clamp(0, 100) as usize;
            reply(method, barrage_payload(account, id, begin, len))
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn chat_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("chat".to_owned())
        .or_insert_with(|| json!({"channel": 0, "messages": [], "barrages": []}))
}

fn chat_info_payload(account: &Value) -> Vec<u8> {
    let messages = account
        .get("chat")
        .and_then(|chat| chat.get("messages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, messages.len().min(i32::MAX as usize) as u64);
    for message in messages {
        append_message_field(&mut output, 2, &chat_message_payload(account, &message));
    }
    output
}

fn chat_message_payload(account: &Value, message: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &simple_user_payload(account));
    append_varint_field(
        &mut output,
        2,
        json_i64(message, "channel").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(&mut output, 3, 0);
    append_varint_field(
        &mut output,
        4,
        json_i64(message, "sendTime").unwrap_or_default().max(0) as u64,
    );
    append_bytes_field(
        &mut output,
        5,
        json_string(message, "message")
            .unwrap_or_default()
            .as_bytes(),
    );
    append_varint_field(
        &mut output,
        6,
        json_i64(message, "msgType").unwrap_or_default().max(0) as u64,
    );
    output
}

fn simple_user_payload(account: &Value) -> Vec<u8> {
    let character = account.get("character").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, json_u64(character, "uid").unwrap_or(1));
    append_varint_field(&mut output, 2, 0);
    append_bytes_field(
        &mut output,
        3,
        json_string(character, "name")
            .unwrap_or_default()
            .as_bytes(),
    );
    append_varint_field(
        &mut output,
        4,
        json_i32(character, "level").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        5,
        json_i32(character, "head").unwrap_or(1021051).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(character, "headFrame").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        8,
        json_i32(character, "fashioning").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        9,
        json_u64(account, "guildId").unwrap_or_default(),
    );
    append_bytes_field(&mut output, 10, b"");
    append_varint_field(
        &mut output,
        11,
        json_i32(character, "teacherPrestige")
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_bytes_field(&mut output, 12, b"");
    append_varint_field(
        &mut output,
        13,
        json_i32(character, "secretaryId").unwrap_or(1).max(0) as u64,
    );
    output
}

fn barrage_payload(account: &Value, id: i32, begin: usize, len: usize) -> Vec<u8> {
    let values = account
        .get("chat")
        .and_then(|chat| chat.get("barrages"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| json_i32(item, "id") == Some(id))
                .skip(begin)
                .take(len.max(1))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, id.max(0) as u64);
    for value in values {
        let mut item = Vec::new();
        append_bytes_field(&mut item, 1, b"content");
        append_bytes_field(
            &mut item,
            2,
            json_string(value, "content").unwrap_or_default().as_bytes(),
        );
        append_message_field(&mut output, 2, &item);
    }
    output
}

fn varint_payload(field: u8, value: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, field, value);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_payload_matches_jp_field_layout() {
        let account = json!({
            "character": {"uid": 7, "name": "司令", "level": 12, "head": 1021051}
        });
        let message = json!({"channel": 3, "message": "hello", "msgType": 1, "sendTime": 99});
        let payload = chat_message_payload(&account, &message);

        assert_eq!(decode_varint_field(&payload, 2), 3);
        assert_eq!(decode_string_field(&payload, 5).as_deref(), Some("hello"));
        assert_eq!(decode_varint_field(&payload, 6), 1);
        let user = decode_repeated_message_field(&payload, 1);
        assert_eq!(user.len(), 1);
        assert_eq!(decode_varint_u64_field(&user[0], 1), 7);
    }

    #[test]
    fn barrage_payload_returns_requested_id_and_content() {
        let account = json!({
            "chat": {"barrages": [{"id": 9, "content": "wave"}]}
        });
        let payload = barrage_payload(&account, 9, 0, 10);
        assert_eq!(decode_varint_field(&payload, 1), 9);
        let entries = decode_repeated_message_field(&payload, 2);
        assert_eq!(decode_string_field(&entries[0], 2).as_deref(), Some("wave"));
    }
}
