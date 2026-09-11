#![allow(dead_code)]

use super::*;

fn hero_slot_ids(hero: &Value, equip_type: u64) -> Option<&Value> {
    if equip_type == 1 {
        Some(hero.get("equipSlots")?)
    } else {
        hero.get("equipSlotsByType")?.get(equip_type.to_string())
    }
}

fn hero_slot_states(hero: &Value, equip_type: u64) -> Option<&Value> {
    if equip_type == 1 {
        hero.get("equipStates")
    } else {
        hero.get("equipStatesByType")?.get(equip_type.to_string())
    }
}

fn hero_slots_mut(hero: &mut serde_json::Map<String, Value>, equip_type: u64) -> &mut Value {
    if equip_type == 1 {
        hero.entry("equipSlots".to_owned())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    } else {
        hero.entry("equipSlotsByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("equipment slots by type must be an object")
            .entry(equip_type.to_string())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    }
}

fn hero_states_mut(hero: &mut serde_json::Map<String, Value>, equip_type: u64) -> &mut Value {
    if equip_type == 1 {
        hero.entry("equipStates".to_owned())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    } else {
        hero.entry("equipStatesByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("equipment states by type must be an object")
            .entry(equip_type.to_string())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    }
}

pub(crate) fn encode_retire_hero_response(rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut nested = Vec::new();
        append_varint_field(&mut nested, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut nested, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut nested, 3, reward.num.max(0) as u64);
        append_message_field(&mut output, 1, &nested);
    }
    output
}

pub(crate) fn illustrate_info_payload_for_templates(
    template_ids: &[i32],
    handbook_behaviours: Option<&[i32]>,
) -> Vec<u8> {
    let now = current_unix_seconds();
    let mut output = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for template_id in template_ids {
        let illustrate_id = (template_id.saturating_sub(1)) / 10;
        if illustrate_id <= 0 || !seen.insert(illustrate_id) {
            continue;
        }
        append_illustrate_info_item(&mut output, illustrate_id, now, handbook_behaviours);
    }
    // IllustrateEquipList repeated field must be non-nil on client incremental updates too.
    append_message_field(&mut output, 9, &[]);
    output
}

pub(crate) fn illustrate_info_payload_for_entries(entries: &[(i32, Vec<i32>)]) -> Vec<u8> {
    let now = current_unix_seconds();
    let mut output = Vec::new();
    for (illustrate_id, behaviours) in entries {
        if *illustrate_id <= 0 {
            continue;
        }
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, *illustrate_id as u64);
        append_varint_field(&mut item, 2, u64::from(now));
        append_varint_field(&mut item, 3, 0);
        for behaviour in behaviours.iter().filter(|id| **id > 0) {
            append_varint_field(&mut item, 5, *behaviour as u64);
        }
        append_varint_field(&mut item, 6, 0);
        append_message_field(&mut output, 1, &item);
    }
    append_message_field(&mut output, 9, &[]);
    output
}

fn append_illustrate_info_item(
    output: &mut Vec<u8>,
    illustrate_id: i32,
    now: u32,
    handbook_behaviours: Option<&[i32]>,
) {
    let mut item = Vec::new();
    append_varint_field(&mut item, 1, illustrate_id as u64);
    append_varint_field(&mut item, 2, u64::from(now));
    append_varint_field(&mut item, 3, 0);
    // BehaviourList and MarryCount are always present in the C# payload. Use the
    // client catalog when available so all illustration actions unlock consistently.
    if let Some(behaviours) = handbook_behaviours.filter(|items| !items.is_empty()) {
        for behaviour in behaviours {
            append_varint_field(&mut item, 5, (*behaviour).max(0) as u64);
        }
    } else {
        append_varint_field(&mut item, 5, 0);
    }
    append_varint_field(&mut item, 6, 0);
    append_message_field(output, 1, &item);
}

pub(crate) fn story_memory_payload(memories: Option<&[(i32, i32)]>) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(memories) = memories {
        for (chapter_id, index) in memories {
            let mut memory = Vec::new();
            append_varint_field(&mut memory, 1, (*chapter_id).max(0) as u64);
            append_varint_field(&mut memory, 2, (*index).max(0) as u64);
            append_message_field(&mut output, 1, &memory);
        }
    }
    output
}
