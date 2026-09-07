#![allow(dead_code)]

#[cfg(test)]
use serde_json::Value;

use super::*;

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn mop_up_payload(account: &Value, now: u32) -> Vec<u8> {
    mop_up_payload_with_pass_rets(account, now, &[])
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn mop_up_payload_with_pass_rets(
    account: &Value,
    now: u32,
    pass_rets: &[Vec<u8>],
) -> Vec<u8> {
    let entries = account
        .get("sweep")
        .and_then(|sweep| sweep.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut output = Vec::new();
    let active = mop_up_active_count(account, now);
    append_varint_field(&mut output, 1, active as u64);
    for entry in entries {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(&entry, "fleetId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(&entry, "copyId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(&entry, "startTime").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            4,
            json_i64(&entry, "endTime").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            5,
            json_i64(&entry, "sweepCounts").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            6,
            json_i64(&entry, "chapterId").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    for pass_ret in pass_rets {
        append_message_field(&mut output, 3, pass_ret);
    }
    output
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn completed_sweep_copy_id(account: &Value, now: u32) -> i32 {
    account
        .get("sweep")
        .and_then(|sweep| sweep.get("entries"))
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find_map(|entry| {
                (json_i64(entry, "endTime").unwrap_or_default() <= i64::from(now))
                    .then(|| json_i32(entry, "copyId"))
                    .flatten()
            })
        })
        .unwrap_or_default()
}

pub(super) fn mop_up_pass_rets(copy_id: i32, rewards: &[ShopReward]) -> Vec<Vec<u8>> {
    // Emit completion record even when stage has no configured drops. The client opens
    // reward dialog from passRets presence; omitting it leaves getrewardspage with nil data.
    if copy_id <= 0 {
        return Vec::new();
    }
    vec![battle_pass_payload_with_rewards(
        copy_id, false, 3, 60, rewards,
    )]
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn mop_up_active_count(account: &Value, now: u32) -> usize {
    account
        .get("sweep")
        .and_then(|sweep| sweep.get("entries"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| json_i64(entry, "endTime").unwrap_or_default() > i64::from(now))
                .count()
        })
        .unwrap_or_default()
}

pub(super) fn draw_draw_count(multiplier: f64, seed: u64) -> usize {
    let multiplier = normalize_multiplier(multiplier);
    let whole = multiplier.floor() as usize;
    let fraction = multiplier - whole as f64;
    if fraction <= 0.0 {
        return whole;
    }
    let roll = (seed % 1_000_000) as f64 / 1_000_000.0;
    whole.saturating_add(usize::from(roll < fraction))
}

pub(super) fn next_battle_drop_seed() -> u64 {
    mix_build_draw_roll(
        u64::from(current_unix_millis()).rotate_left(32)
            ^ BATTLE_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    )
}

pub(super) fn draw_copy_drop_with_seed(
    catalog: &BattleCatalog,
    drop_id: i32,
    depth: u8,
    seed: u64,
) -> Option<ShopReward> {
    if depth >= 16 {
        return None;
    }
    let entries = catalog.drop_pools.get(&drop_id)?;
    let total = entries
        .iter()
        .map(|entry| i64::from(entry.4.max(0)))
        .sum::<i64>();
    if total <= 0 {
        return None;
    }
    let mut roll = (mix_build_draw_roll(seed) % u64::try_from(total).ok()?) as i64;
    for entry in entries {
        roll -= i64::from(entry.4.max(0));
        if roll >= 0 {
            continue;
        }
        if entry.0 == 4 {
            return draw_copy_drop_with_seed(
                catalog,
                entry.1,
                depth + 1,
                mix_build_draw_roll(seed ^ 0xA076_1D64_78BD_642F),
            );
        }
        return Some(ShopReward {
            goods_type: entry.0,
            item_id: entry.1,
            num: entry.2.max(1),
            instance_id: 0,
        });
    }
    None
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn fleet_hero_ids(account: &Value, fleet_id: u64) -> Vec<u64> {
    let Some(tactics) = account
        .get("fleet")
        .and_then(|fleet| fleet.get("tactics"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    tactics
        .iter()
        .enumerate()
        .find(|(index, tactic)| {
            json_u64(tactic, "fleetId") == Some(fleet_id)
                || u64::try_from(index.saturating_add(1)).ok() == Some(fleet_id)
        })
        .map(|(_, tactic)| {
            json_i32_array(tactic, "heroInfo")
                .into_iter()
                .chain(json_i32_array(tactic, "heroIds"))
                .filter(|id| *id > 0)
                .filter_map(|id| u64::try_from(id).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn settle_mop_up(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    fashion_catalog: Option<&FashionList>,
    now: u32,
) -> Vec<ShopReward> {
    settle_mop_up_with_config(
        account,
        catalog,
        fashion_catalog,
        now,
        1.0,
        1.0,
        1.0,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn settle_mop_up_with_config(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    fashion_catalog: Option<&FashionList>,
    now: u32,
    drop_multiplier: f64,
    commander_exp_multiplier: f64,
    ship_exp_multiplier: f64,
    commander_level_catalog: Option<&CommanderLevelCatalog>,
    hero_level_catalog: Option<&HeroLevelCatalog>,
) -> Vec<ShopReward> {
    settle_mop_up_with_gameplay_config(
        account,
        catalog,
        fashion_catalog,
        now,
        drop_multiplier,
        commander_exp_multiplier,
        ship_exp_multiplier,
        1.0,
        commander_level_catalog,
        hero_level_catalog,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
#[cfg(test)]
pub(super) fn settle_mop_up_with_gameplay_config(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    fashion_catalog: Option<&FashionList>,
    now: u32,
    drop_multiplier: f64,
    commander_exp_multiplier: f64,
    ship_exp_multiplier: f64,
    affection_multiplier: f64,
    commander_level_catalog: Option<&CommanderLevelCatalog>,
    hero_level_catalog: Option<&HeroLevelCatalog>,
) -> Vec<ShopReward> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let completed = account
        .get("sweep")
        .and_then(|sweep| sweep.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| json_i64(entry, "endTime").unwrap_or_default() <= i64::from(now))
        .cloned()
        .collect::<Vec<_>>();
    if completed.is_empty() {
        return Vec::new();
    }
    let mut rewards = Vec::new();
    for entry in &completed {
        let copy_id = json_i32(entry, "copyId").unwrap_or_default();
        let count = json_i32(entry, "sweepCounts")
            .unwrap_or_default()
            .clamp(1, 99);
        let fleet_id = json_u64(entry, "fleetId").unwrap_or_default();
        let hero_ids = fleet_hero_ids(account, fleet_id);
        let (commander_base_exp, ship_base_exp) = battle_copy_experience(Some(catalog), copy_id);
        for _ in 0..count {
            let commander_exp =
                scale_reward(i64::from(commander_base_exp), commander_exp_multiplier)
                    .clamp(0, i64::from(i32::MAX)) as i32;
            let ship_exp = scale_reward(i64::from(ship_base_exp), ship_exp_multiplier)
                .clamp(0, i64::from(i32::MAX)) as i32;
            add_commander_battle_exp(account, commander_exp, commander_level_catalog);
            if !hero_ids.is_empty() {
                add_ship_battle_exp(account, &hero_ids, ship_exp, hero_level_catalog);
            }
            if let Some(rule) = catalog.settlement_by_copy.get(&copy_id).copied() {
                apply_battle_settlement(
                    account,
                    &hero_ids,
                    None,
                    &std::collections::HashSet::new(),
                    rule,
                    affection_multiplier,
                );
            }
            rewards.extend(draw_battle_drop_rewards(
                account,
                Some(catalog),
                copy_id,
                drop_multiplier,
                now,
                fashion_catalog,
            ));
        }
    }
    if let Some(entries) = account
        .get_mut("sweep")
        .and_then(|sweep| sweep.get_mut("entries"))
        .and_then(Value::as_array_mut)
    {
        entries.retain(|entry| json_i64(entry, "endTime").unwrap_or_default() > i64::from(now));
    }
    rewards
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn update_mop_up_state(account: &mut Value, method: &str, args: &[u8], now: u32) {
    let (fleet_id, copy_id, sweep_counts) = decode_mop_up_arg(args);
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let sweep = root
        .entry("sweep".to_owned())
        .or_insert_with(|| json!({"entries": []}));
    let Some(sweep) = sweep.as_object_mut() else {
        return;
    };
    let entries = sweep
        .entry("entries".to_owned())
        .or_insert_with(|| json!([]));
    let Some(entries) = entries.as_array_mut() else {
        return;
    };
    if method == "mopUp.StopSweep" {
        entries.retain(|entry| {
            (fleet_id != 0 && json_u64(entry, "fleetId") != Some(fleet_id))
                || (copy_id != 0 && json_u64(entry, "copyId") != Some(copy_id))
        });
        return;
    }
    if fleet_id == 0 || copy_id == 0 || sweep_counts == 0 {
        return;
    }
    entries.retain(|entry| json_u64(entry, "fleetId") != Some(fleet_id));
    entries.push(json!({
        "fleetId": fleet_id,
        "copyId": copy_id,
        "startTime": now,
        "endTime": now.saturating_add(1),
        "sweepCounts": sweep_counts,
        "chapterId": 0,
    }));
}
