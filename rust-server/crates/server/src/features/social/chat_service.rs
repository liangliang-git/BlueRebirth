use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let now = current_unix_seconds() as u64;
    let uid = account.character.uid;
    match method {
        "chat.ChatInfo" => reply(method, chat_info_payload_typed(account)),
        "chat.ChangeWorldChannel" => {
            let request = match ChangeWorldChannelRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return HandlerResult::Error(GameError::InvalidRequest("chat channel")),
            };
            account.chat.channel = u32::try_from(request.channel).unwrap_or_default();
            reply(method, varint_payload(1, u64::from(account.chat.channel)))
        }
        "chat.SendMessage" => {
            let request = match SendMessageRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return HandlerResult::Error(GameError::InvalidRequest("chat message")),
            };
            let message = request.message;
            let entry = blueoath_domain::ChatMessageState {
                id: account
                    .chat
                    .messages
                    .last()
                    .map(|entry| entry.id.saturating_add(1))
                    .unwrap_or(1),
                uid,
                channel: u32::try_from(request.channel).unwrap_or_default(),
                receive_uid: request.receive_uid,
                message: message.clone(),
                message_type: u32::try_from(request.message_type).unwrap_or_default(),
                voice: request.voice,
                sent_at: now,
            };
            account.chat.messages.push(entry.clone());
            if account.chat.messages.len() > 100 {
                let excess = account.chat.messages.len() - 100;
                account.chat.messages.drain(..excess);
            }
            effects.push_post(Response::raw(
                "chat.NewMessage",
                chat_message_payload_typed(account, &entry),
            ));
            let mut response = Vec::new();
            append_bytes_field(&mut response, 1, message.as_bytes());
            reply(method, response)
        }
        "chat.SendBarrage" => {
            let request = match SendBarrageRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return HandlerResult::Error(GameError::InvalidRequest("barrage")),
            };
            account
                .chat
                .barrages
                .push(blueoath_domain::ChatBarrageState {
                    id: u32::try_from(request.id).unwrap_or_default(),
                    offset: u32::try_from(request.offset).unwrap_or_default(),
                    content: request.content,
                    uid,
                    sent_at: now,
                });
            reply(method, Vec::new())
        }
        "chat.GetBarrageById" => {
            let request = match GetBarrageByIdRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return HandlerResult::Error(GameError::InvalidRequest("barrage query")),
            };
            reply(
                method,
                barrage_payload_typed(
                    account,
                    u32::try_from(request.id).unwrap_or_default(),
                    usize::try_from(request.begin).unwrap_or_default(),
                    usize::try_from(request.len).unwrap_or_default(),
                ),
            )
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn chat_info_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        account.chat.messages.len().min(i32::MAX as usize) as u64,
    );
    for message in &account.chat.messages {
        append_message_field(
            &mut output,
            2,
            &chat_message_payload_typed(account, message),
        );
    }
    output
}

fn chat_message_payload_typed(
    account: &blueoath_domain::AccountState,
    message: &blueoath_domain::ChatMessageState,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &simple_user_payload_typed(account));
    append_varint_field(&mut output, 2, u64::from(message.channel));
    append_varint_field(&mut output, 3, 0);
    append_varint_field(&mut output, 4, message.sent_at);
    append_bytes_field(&mut output, 5, message.message.as_bytes());
    append_varint_field(&mut output, 6, u64::from(message.message_type));
    output
}

fn simple_user_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let character = &account.character;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, character.uid);
    append_varint_field(&mut output, 2, 0);
    append_bytes_field(&mut output, 3, character.name.as_bytes());
    append_varint_field(&mut output, 4, u64::from(character.level));
    append_varint_field(
        &mut output,
        5,
        u64::from(if character.head == 0 {
            1021051
        } else {
            character.head
        }),
    );
    append_varint_field(&mut output, 6, u64::from(character.head_frame));
    append_varint_field(
        &mut output,
        13,
        character.secretary_id.map(|id| id.get()).unwrap_or(1),
    );
    output
}

fn barrage_payload_typed(
    account: &blueoath_domain::AccountState,
    id: u32,
    begin: usize,
    len: usize,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, u64::from(id));
    for value in account
        .chat
        .barrages
        .iter()
        .filter(|value| value.id == id)
        .skip(begin)
        .take(len.max(1))
    {
        let mut item = Vec::new();
        append_bytes_field(&mut item, 1, b"content");
        append_bytes_field(&mut item, 2, value.content.as_bytes());
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
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("chat-payload").unwrap(),
            "司令",
        );
        account.character.uid = 7;
        account.character.level = 12;
        account
            .chat
            .messages
            .push(blueoath_domain::ChatMessageState {
                id: 1,
                channel: 3,
                uid: 7,
                receive_uid: 0,
                message: "hello".to_owned(),
                message_type: 1,
                voice: String::new(),
                sent_at: 99,
            });
        let payload = chat_message_payload_typed(&account, &account.chat.messages[0]);

        assert_eq!(decode_varint_field(&payload, 2), 3);
        assert_eq!(decode_string_field(&payload, 5).as_deref(), Some("hello"));
        assert_eq!(decode_varint_field(&payload, 6), 1);
        let user = decode_repeated_message_field(&payload, 1);
        assert_eq!(user.len(), 1);
        assert_eq!(decode_varint_u64_field(&user[0], 1), 7);
    }

    #[test]
    fn barrage_payload_returns_requested_id_and_content() {
        let mut account = blueoath_domain::AccountState::default();
        account
            .chat
            .barrages
            .push(blueoath_domain::ChatBarrageState {
                id: 9,
                offset: 0,
                content: "wave".to_owned(),
                uid: 0,
                sent_at: 0,
            });
        let payload = barrage_payload_typed(&account, 9, 0, 10);
        assert_eq!(decode_varint_field(&payload, 1), 9);
        let entries = decode_repeated_message_field(&payload, 2);
        assert_eq!(decode_string_field(&entries[0], 2).as_deref(), Some("wave"));
    }

    #[test]
    fn typed_chat_handler_updates_typed_state_and_emits_message_push() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("chat-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 3);
        append_bytes_field(&mut args, 3, b"hello");
        let mut effects = ResponseEffects::default();

        let result = handle_typed(&mut account, "chat.SendMessage", &args, &mut effects);

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.chat.messages.len(), 1);
        assert_eq!(account.chat.messages[0].message, "hello");
        let (pre, post, error) = effects.into_parts();
        assert!(pre.is_empty());
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].method, "chat.NewMessage");
        assert!(error.is_none());
    }
}
