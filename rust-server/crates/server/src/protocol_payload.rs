use blueoath_protocol::Decode;

use super::*;

pub(super) fn decode_hero_exp_item(payload: &[u8]) -> Option<(i32, i32)> {
    let mut item_id = 0;
    let mut num = 0;
    let mut index = 0;
    while index < payload.len() {
        let (key, next) = read_varint(payload, index).ok()?;
        index = next;
        let field = key >> 3;
        let wire = key & 7;
        if wire == 0 && (field == 2 || field == 3) {
            let (value, next) = read_varint(payload, index).ok()?;
            let value = i32::try_from(value).ok()?;
            if field == 2 {
                item_id = value;
            } else {
                num = value;
            }
            index = next;
        } else {
            index = skip_wire(payload, index, wire)?;
        }
    }
    (item_id > 0).then_some((item_id, num))
}

pub(super) fn encode_hero_add_exp_response(hero_id: u64, items: &[(i32, i32)]) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, hero_id);
    for (item_id, num) in items {
        let mut item = Vec::new();
        if *item_id != 0 {
            append_varint_field(&mut item, 2, (*item_id).max(0) as u64);
        }
        if *num != 0 {
            append_varint_field(&mut item, 3, (*num).max(0) as u64);
        }
        append_message_field(&mut output, 2, &item);
    }
    output
}

pub(super) fn decode_varint_field(payload: &[u8], wanted_field: u8) -> i32 {
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            return 0;
        };
        index = next;
        let field = (key >> 3) as u8;
        let wire = key & 7;
        if field == wanted_field && wire == 0 {
            let Ok((value, _)) = read_varint(payload, index) else {
                return 0;
            };
            return i32::try_from(value).unwrap_or_default();
        }
        match wire {
            0 => {
                let Ok((_, next)) = read_varint(payload, index) else {
                    return 0;
                };
                index = next;
            }
            1 => index = index.saturating_add(8),
            2 => {
                let Ok((len, next)) = read_varint(payload, index) else {
                    return 0;
                };
                index = next.saturating_add(len as usize);
            }
            5 => index = index.saturating_add(4),
            _ => return 0,
        }
    }
    0
}

pub(super) fn decode_hero_intensify_request(payload: &[u8]) -> (u64, Vec<u64>, bool) {
    let mut hero_id = 0;
    let mut consumed = Vec::new();
    let mut super_intensify = false;
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = u8::try_from(key >> 3).unwrap_or_default();
        let wire = key & 7;
        match (field, wire) {
            (1, 0) => {
                if let Ok((value, next)) = read_varint(payload, index) {
                    hero_id = value;
                    index = next;
                } else {
                    break;
                }
            }
            (2, 0) | (3, 0) => {
                if let Ok((value, next)) = read_varint(payload, index) {
                    if field == 2 {
                        consumed.push(value);
                    } else {
                        super_intensify = value != 0;
                    }
                    index = next;
                } else {
                    break;
                }
            }
            (2, 2) => {
                let Ok((length, next)) = read_varint(payload, index) else {
                    break;
                };
                let Ok(length) = usize::try_from(length) else {
                    break;
                };
                let Some(end) = next.checked_add(length) else {
                    break;
                };
                let Some(packed) = payload.get(next..end) else {
                    break;
                };
                let mut packed_index = 0;
                while packed_index < packed.len() {
                    let Ok((value, next)) = read_varint(packed, packed_index) else {
                        break;
                    };
                    consumed.push(value);
                    packed_index = next;
                }
                index = end;
            }
            (_, 0) => {
                if let Ok((_, next)) = read_varint(payload, index) {
                    index = next;
                } else {
                    break;
                }
            }
            (_, 1) => index = index.saturating_add(8),
            (_, 2) => {
                let Ok((length, next)) = read_varint(payload, index) else {
                    break;
                };
                let Ok(length) = usize::try_from(length) else {
                    break;
                };
                index = next.saturating_add(length);
            }
            (_, 5) => index = index.saturating_add(4),
            _ => break,
        }
    }
    (hero_id, consumed, super_intensify)
}

pub(super) fn decode_hero_advance_mub_request(payload: &[u8]) -> (u64, Vec<(i32, i32)>) {
    let hero_id = decode_varint_u64_field(payload, 1);
    let items = decode_repeated_message_field(payload, 2)
        .into_iter()
        .filter_map(|item| {
            let item_id = decode_varint_field(&item, 1);
            let amount = decode_varint_field(&item, 2);
            (item_id > 0 && amount > 0).then_some((item_id, amount))
        })
        .collect();
    (hero_id, items)
}

pub(super) fn decode_varint_u64_field(payload: &[u8], wanted_field: u8) -> u64 {
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            return 0;
        };
        index = next;
        let field = u8::try_from(key >> 3).unwrap_or_default();
        let wire = key & 7;
        if field == wanted_field && wire == 0 {
            return read_varint(payload, index)
                .map(|(value, _)| value)
                .unwrap_or_default();
        }
        index = match wire {
            0 => read_varint(payload, index)
                .map(|(_, next)| next)
                .unwrap_or(payload.len()),
            1 => index.saturating_add(8),
            2 => {
                let Ok((length, next)) = read_varint(payload, index) else {
                    return 0;
                };
                next.saturating_add(length as usize)
            }
            5 => index.saturating_add(4),
            _ => return 0,
        };
    }
    0
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct BattleHeroResult {
    pub(super) hero_id: u64,
    pub(super) hp: i64,
}

#[derive(Default)]
pub(super) struct BattlePassResult {
    pub(super) grade: i32,
    pub(super) battle_time: i32,
    pub(super) mvp_hero_id: Option<u64>,
    pub(super) shipwrecked_ids: std::collections::HashSet<u64>,
    pub(super) heroes: Vec<BattleHeroResult>,
    pub(super) damage: i32,
}

pub(super) fn decode_battle_pass_result(payload: &[u8]) -> BattlePassResult {
    let Ok(request) = blueoath_protocol::CopyPassRequest::decode(payload) else {
        return BattlePassResult::default();
    };
    let mut result = BattlePassResult {
        grade: request.grade,
        battle_time: request.battle_time,
        mvp_hero_id: request.mvp_hero_id,
        damage: request.damage,
        ..BattlePassResult::default()
    };
    for hero in request.heroes {
        let hp = i64::try_from(hero.hp).unwrap_or(i64::MAX);
        result.heroes.push(BattleHeroResult {
            hero_id: hero.hero_id,
            hp,
        });
        if hp == 0 {
            result.shipwrecked_ids.insert(hero.hero_id);
        }
    }
    result
}

/// Decode TMopUpArg. Client wraps argument message as field 1 of request args;
/// retain direct-field fallback for older clients/tests.
pub(super) fn decode_mop_up_arg(payload: &[u8]) -> (u64, u64, u64) {
    let mut nested: Option<&[u8]> = None;
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = u8::try_from(key >> 3).unwrap_or_default();
        let wire = key & 7;
        if field == 1 && wire == 2 {
            if let Ok((len, body)) = read_varint(payload, index) {
                let end = body.saturating_add(len as usize);
                if end <= payload.len() {
                    nested = Some(&payload[body..end]);
                }
            }
            break;
        }
        index = match wire {
            0 => read_varint(payload, index)
                .map(|(_, next)| next)
                .unwrap_or(payload.len()),
            1 => index.saturating_add(8),
            2 => read_varint(payload, index)
                .map(|(len, body)| body.saturating_add(len as usize))
                .unwrap_or(payload.len()),
            5 => index.saturating_add(4),
            _ => payload.len(),
        };
    }
    let body = nested.unwrap_or(payload);
    (
        decode_varint_u64_field(body, 1),
        decode_varint_u64_field(body, 2),
        decode_varint_u64_field(body, 3),
    )
}

pub(super) fn decode_string_field(payload: &[u8], wanted_field: u8) -> Option<String> {
    let mut index = 0;
    while index < payload.len() {
        let (key, next) = read_varint(payload, index).ok()?;
        index = next;
        let field = u8::try_from(key >> 3).ok()?;
        let wire = key & 7;
        if wire == 2 {
            let (length, next) = read_varint(payload, index).ok()?;
            let end = next.checked_add(usize::try_from(length).ok()?)?;
            let bytes = payload.get(next..end)?;
            if field == wanted_field {
                return String::from_utf8(bytes.to_vec()).ok();
            }
            index = end;
            continue;
        }
        index = match wire {
            0 => read_varint(payload, index).ok()?.1,
            1 => index.checked_add(8)?,
            5 => index.checked_add(4)?,
            _ => return None,
        };
    }
    None
}
pub(super) fn decode_hero_add_exp_request(payload: &[u8]) -> (u64, Vec<(i32, i32)>) {
    let mut hero_id = 0;
    let mut items = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = key >> 3;
        let wire = key & 7;
        if field == 1 && wire == 0 {
            if let Ok((value, next)) = read_varint(payload, index) {
                hero_id = value;
                index = next;
            } else {
                break;
            }
        } else if field == 2 && wire == 2 {
            let Ok((length, next)) = read_varint(payload, index) else {
                break;
            };
            let Ok(length) = usize::try_from(length) else {
                break;
            };
            let Some(end) = next.checked_add(length) else {
                break;
            };
            let Some(body) = payload.get(next..end) else {
                break;
            };
            if let Some(item) = decode_hero_exp_item(body) {
                items.push(item);
            }
            index = end;
        } else {
            index = skip_wire(payload, index, wire).unwrap_or(payload.len());
        }
    }
    (hero_id, items)
}

pub(super) fn decode_repeated_varint_field(payload: &[u8], wanted_field: u8) -> Vec<i32> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = (key >> 3) as u8;
        let wire = key & 7;
        match wire {
            0 => {
                let Ok((value, next)) = read_varint(payload, index) else {
                    break;
                };
                index = next;
                if field == wanted_field {
                    if let Ok(value) = i32::try_from(value) {
                        values.push(value);
                    }
                }
            }
            1 => index = index.saturating_add(8),
            2 => {
                let Ok((len, next)) = read_varint(payload, index) else {
                    break;
                };
                index = next.saturating_add(len as usize);
            }
            5 => index = index.saturating_add(4),
            _ => break,
        }
    }
    values
}

pub(super) fn decode_repeated_i32_field(payload: &[u8], wanted_field: u8) -> Vec<i32> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = (key >> 3) as u8;
        let wire = key & 7;
        match wire {
            0 => {
                let Ok((value, next)) = read_varint(payload, index) else {
                    break;
                };
                index = next;
                if field == wanted_field {
                    values.push(value as i32);
                }
            }
            1 => index = index.saturating_add(8),
            2 => {
                let Ok((len, next)) = read_varint(payload, index) else {
                    break;
                };
                let start = next;
                let end = start.saturating_add(len as usize);
                if end > payload.len() {
                    break;
                }
                if field == wanted_field {
                    let mut packed = start;
                    while packed < end {
                        let Ok((value, next)) = read_varint(payload, packed) else {
                            break;
                        };
                        values.push(value as i32);
                        packed = next;
                    }
                }
                index = end;
            }
            5 => index = index.saturating_add(4),
            _ => break,
        }
    }
    values
}

pub(super) fn decode_repeated_message_field(payload: &[u8], wanted_field: u8) -> Vec<Vec<u8>> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = u8::try_from(key >> 3).unwrap_or_default();
        let wire = key & 7;
        match wire {
            0 => {
                let Ok((_, next)) = read_varint(payload, index) else {
                    break;
                };
                index = next;
            }
            1 => index = index.saturating_add(8),
            2 => {
                let Ok((length, next)) = read_varint(payload, index) else {
                    break;
                };
                let start = next;
                let end = start.saturating_add(length as usize);
                if end > payload.len() {
                    break;
                }
                if field == wanted_field {
                    values.push(payload[start..end].to_vec());
                }
                index = end;
            }
            5 => index = index.saturating_add(4),
            _ => break,
        }
    }
    values
}

pub(super) fn decode_auto_equip_units(payload: &[u8]) -> Vec<(u64, Vec<(u64, u64)>)> {
    decode_repeated_message_field(payload, 1)
        .into_iter()
        .filter_map(|unit| {
            let hero_id = decode_varint_u64_field(&unit, 1);
            let equips = decode_repeated_message_field(&unit, 2)
                .into_iter()
                .map(|equip| {
                    (
                        decode_varint_u64_field(&equip, 1),
                        decode_varint_u64_field(&equip, 2),
                    )
                })
                .filter(|(slot, _)| (1..=6).contains(slot))
                .collect::<Vec<_>>();
            (hero_id > 0 && !equips.is_empty()).then_some((hero_id, equips))
        })
        .collect()
}

pub(super) fn decode_equip_effect_request(payload: &[u8]) -> (u64, Vec<(i32, Vec<i32>)>) {
    let hero_id = decode_varint_u64_field(payload, 1);
    let effects = decode_repeated_message_field(payload, 2)
        .into_iter()
        .map(|effect| {
            (
                decode_varint_field(&effect, 1),
                decode_repeated_varint_field(&effect, 2),
            )
        })
        .collect();
    (hero_id, effects)
}

pub(super) fn decode_equip_binding_request(payload: &[u8]) -> (u64, u64, u64) {
    (
        decode_varint_u64_field(payload, 1),
        decode_varint_u64_field(payload, 2),
        decode_varint_u64_field(payload, 3),
    )
}

pub(super) fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(super) fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(super) fn decode_start_hero_groups(payload: &[u8]) -> Vec<Vec<i32>> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        let field = (key >> 3) as u8;
        let wire = key & 7;
        if field == 13 && wire == 2 {
            let Ok((len, next)) = read_varint(payload, index) else {
                break;
            };
            let start = next;
            let end = start.saturating_add(len as usize).min(payload.len());
            groups.push(decode_repeated_varint_field(&payload[start..end], 1));
            index = end;
            continue;
        }
        match wire {
            0 => {
                index = read_varint(payload, index)
                    .map(|(_, next)| next)
                    .unwrap_or(payload.len())
            }
            1 => index = index.saturating_add(8),
            2 => {
                let Ok((len, next)) = read_varint(payload, index) else {
                    break;
                };
                index = next.saturating_add(len as usize);
            }
            5 => index = index.saturating_add(4),
            _ => break,
        }
    }
    groups
}
