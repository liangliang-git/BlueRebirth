#![allow(dead_code)]

use super::*;

pub(crate) fn select_construction_template(gold: i64, steel: i64, aluminium: i64) -> i32 {
    if let Some(catalog) = BUILD_FORMULA_CATALOG.get() {
        for (r1, r2, r3, ships) in &catalog.0 {
            let in_range = |value: i64, range: &[i64]| {
                range.len() >= 2 && value >= range[0] && value <= range[1]
            };
            if in_range(gold, r1) && in_range(steel, r2) && in_range(aluminium, r3) {
                if let Some(template) = ships.first() {
                    return *template;
                }
            }
        }
    }
    match gold + steel + aluminium {
        total if total >= 2400 => 10210513,
        total if total >= 1200 => 10210512,
        _ => 10210511,
    }
}

pub(crate) fn construction_duration_seconds(template_id: i32) -> i32 {
    let configured = BUILD_SHIP_CATALOG
        .get()
        .and_then(|catalog| catalog.ship_build_time.get(&template_id))
        .copied()
        .unwrap_or(60);
    configured.clamp(60, 7 * 24 * 60 * 60)
}

pub(crate) fn build_notes_payload(now: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(catalog) = BUILD_FORMULA_CATALOG.get() else {
        return out;
    };
    for (r1, r2, r3, ships) in &catalog.0 {
        let Some(&template_id) = ships.first() else {
            continue;
        };
        let gold = r1.first().copied().unwrap_or(30).clamp(30, 999);
        let steel = r2.first().copied().unwrap_or(30).clamp(30, 999);
        let aluminium = r3.first().copied().unwrap_or(30).clamp(30, 999);
        let mut project = Vec::new();
        for (res_id, count) in [(10029, steel), (10030, aluminium)] {
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, res_id as u64);
            append_varint_field(&mut item, 2, count as u64);
            append_message_field(&mut project, 1, &item);
        }
        append_varint_field(&mut project, 2, gold as u64);
        let mut formula = Vec::new();
        append_varint_field(&mut formula, 1, u64::from(now));
        append_message_field(&mut formula, 2, &project);
        append_varint_field(&mut formula, 3, template_id as u64);
        let mut note = Vec::new();
        append_bytes_field(&mut note, 1, format!("Ship {template_id}").as_bytes());
        append_message_field(&mut note, 2, &formula);
        append_varint_field(&mut note, 3, 0);
        append_varint_field(&mut note, 4, 0);
        append_varint_field(&mut note, 5, template_id as u64);
        append_message_field(&mut out, 1, &note);
    }
    out
}

pub(crate) fn discuss_payload(htid: i32) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, 0);
    append_varint_field(&mut out, 2, 0);
    append_varint_field(&mut out, 3, u64::from(current_unix_seconds()));
    append_varint_field(&mut out, 4, 0);
    if let Some(catalog) = BUILD_FORMULA_CATALOG.get() {
        if let Some((r1, r2, r3, ships)) = catalog.0.iter().find(|(_, _, _, ships)| {
            ships
                .iter()
                .any(|template| *template == htid || *template / 10 == htid)
        }) {
            let template_id = ships.first().copied().unwrap_or_default();
            let msg = format!(
                "Build formula: gold {} steel {} aluminium {}",
                r1.first().copied().unwrap_or(30),
                r2.first().copied().unwrap_or(30),
                r3.first().copied().unwrap_or(30)
            );
            let mut item = Vec::new();
            append_bytes_field(&mut item, 1, format!("Ship {template_id}").as_bytes());
            append_bytes_field(&mut item, 2, msg.as_bytes());
            for field in 3..=8 {
                append_varint_field(&mut item, field, 0);
            }
            append_message_field(&mut out, 5, &item);
        }
    }
    out
}

pub(crate) fn encode_discuss_empty() -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, 0);
    append_varint_field(&mut out, 2, 0);
    append_varint_field(&mut out, 3, u64::from(current_unix_seconds()));
    append_varint_field(&mut out, 4, 0);
    out
}

pub(crate) fn encode_rewards_list(rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type as u64);
        append_varint_field(&mut item, 2, reward.item_id as u64);
        append_varint_field(&mut item, 3, reward.num as u64);
        append_varint_field(&mut item, 4, reward.instance_id as u64);
        append_message_field(&mut out, 1, &item);
    }
    out
}

pub(crate) fn encode_buildship_ret(rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_varint_field(&mut item, 4, reward.instance_id.max(0) as u64);
        append_message_field(&mut out, 1, &item);
        append_message_field(&mut out, 3, &[]);
    }
    out
}
