#[cfg(test)]
use serde_json::Value;

use super::*;

#[cfg(test)]
pub(crate) fn buildship_info_payload(account: Option<&Value>, now: u32) -> Vec<u8> {
    const ENABLED_POOLS: &[u32] = &[106, 109, 124, 150, 151, 152, 154];
    let mut output = Vec::new();
    let close_time = u64::from(now).saturating_add(365 * 24 * 60 * 60);
    // Client catalog contains historical/duplicate activity rows. Offline mode exposes
    // one stable pool per supported exploration activity; otherwise client renders every
    // duplicate event as a separate tab.
    let mut pools = BUILD_SHIP_CATALOG
        .get()
        .map(|catalog| {
            ENABLED_POOLS
                .iter()
                .copied()
                .filter(|id| catalog.extract_to_drop.contains_key(&(*id as i32)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if pools.is_empty() {
        pools.extend_from_slice(ENABLED_POOLS);
    }
    pools.sort_unstable();
    pools.dedup();
    for pool_id in pools {
        let mut entry = Vec::new();
        append_varint_field(&mut entry, 1, u64::from(pool_id));
        append_varint_field(&mut entry, 2, close_time);
        append_message_field(&mut output, 10, &entry);
    }
    append_buildship_count_map(&mut output, account, "drawCount", 4);
    append_buildship_reward_map(&mut output, account, "usedBoxInfo", 6);
    append_buildship_reward_map(&mut output, account, "usedRewardInfo", 7);
    output
}

pub(crate) fn buildship_info_payload_from_typed(
    account: &blueoath_domain::AccountState,
    now: u32,
) -> Vec<u8> {
    const ENABLED_POOLS: &[u32] = &[106, 109, 124, 150, 151, 152, 154];
    let mut output = Vec::new();
    let close_time = u64::from(now).saturating_add(365 * 24 * 60 * 60);
    let mut pools = BUILD_SHIP_CATALOG
        .get()
        .map(|catalog| {
            ENABLED_POOLS
                .iter()
                .copied()
                .filter(|id| catalog.extract_to_drop.contains_key(&(*id as i32)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if pools.is_empty() {
        pools.extend_from_slice(ENABLED_POOLS);
    }
    pools.sort_unstable();
    pools.dedup();
    for pool_id in pools {
        let mut entry = Vec::new();
        append_varint_field(&mut entry, 1, u64::from(pool_id));
        append_varint_field(&mut entry, 2, close_time);
        append_message_field(&mut output, 10, &entry);
    }
    for (pool_id, count) in &account.build_ship.draw_counts {
        let mut entry = Vec::new();
        append_varint_field(&mut entry, 1, *pool_id);
        append_varint_field(&mut entry, 2, u64::from(*count));
        append_message_field(&mut output, 4, &entry);
    }
    for (field, claims) in [
        (6_u8, &account.build_ship.used_box_info),
        (7_u8, &account.build_ship.used_reward_info),
    ] {
        for (pool_id, milestones) in claims {
            if milestones.is_empty() {
                continue;
            }
            let mut entry = Vec::new();
            append_varint_field(&mut entry, 1, *pool_id);
            for milestone in milestones {
                append_varint_field(&mut entry, 2, u64::from(*milestone));
            }
            append_message_field(&mut output, field, &entry);
        }
    }
    output
}

pub(crate) fn expand_build_drop(catalog: &BuildShipCatalog, drop_id: i32) -> Vec<BuildDropEntry> {
    fn visit(catalog: &BuildShipCatalog, id: i32, depth: u8, out: &mut Vec<BuildDropEntry>) {
        if depth > 8 {
            return;
        }
        for entry in catalog.pools.get(&id).into_iter().flatten() {
            if entry.0 == 4 {
                visit(catalog, entry.1, depth + 1, out);
            } else {
                out.push(*entry);
            }
        }
    }
    let mut out = Vec::new();
    visit(catalog, drop_id, 0, &mut out);
    out
}

#[cfg(test)]
pub(crate) fn append_buildship_count_map(
    output: &mut Vec<u8>,
    account: Option<&Value>,
    key: &str,
    field: u8,
) {
    let Some(map) = account
        .and_then(|account| account.get("buildState"))
        .and_then(|state| state.get(key).or_else(|| state.get(snake_case_key(key))))
        .and_then(Value::as_object)
    else {
        return;
    };
    for (pool_id, count) in map {
        let Ok(pool_id) = pool_id.parse::<u32>() else {
            continue;
        };
        let mut entry = Vec::new();
        append_varint_field(&mut entry, 1, u64::from(pool_id));
        append_varint_field(&mut entry, 2, json_i64_any(count).max(0) as u64);
        append_message_field(output, field, &entry);
    }
}

#[cfg(test)]
pub(crate) fn append_buildship_reward_map(
    output: &mut Vec<u8>,
    account: Option<&Value>,
    key: &str,
    field: u8,
) {
    let Some(map) = account
        .and_then(|account| account.get("buildState"))
        .and_then(|state| state.get(key).or_else(|| state.get(snake_case_key(key))))
        .and_then(Value::as_object)
    else {
        return;
    };
    for (pool_id, counts) in map {
        let Ok(pool_id) = pool_id.parse::<u32>() else {
            continue;
        };
        let Some(counts) = counts.as_array() else {
            continue;
        };
        if counts.is_empty() {
            continue;
        }
        let mut entry = Vec::new();
        append_varint_field(&mut entry, 1, u64::from(pool_id));
        for count in counts {
            append_varint_field(&mut entry, 2, json_i64_any(count).max(0) as u64);
        }
        append_message_field(output, field, &entry);
    }
}

#[cfg(test)]
pub(crate) fn snake_case_key(key: &str) -> String {
    let mut output = String::new();
    for (index, ch) in key.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.extend(ch.to_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}
