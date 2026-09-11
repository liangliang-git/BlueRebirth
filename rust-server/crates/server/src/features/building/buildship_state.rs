use super::*;

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
