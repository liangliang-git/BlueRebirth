#![allow(dead_code)]

use super::*;

pub(crate) fn encode_equip_enhance_response(equip_id: u64, level: i32, exp: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, equip_id);
    append_varint_field(&mut output, 2, level.max(0) as u64);
    append_varint_field(&mut output, 3, exp.max(0) as u64);
    output
}
