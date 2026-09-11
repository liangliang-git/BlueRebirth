use super::super::*;

pub(crate) fn daily_copy_enter_payload(start_base_ret: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    append_message_field(&mut payload, 1, start_base_ret);
    payload
}
