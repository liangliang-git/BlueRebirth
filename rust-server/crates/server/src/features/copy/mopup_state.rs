#![allow(dead_code)]

use super::*;

pub(crate) fn mop_up_pass_rets(copy_id: i32, rewards: &[ShopReward]) -> Vec<Vec<u8>> {
    // Emit completion record even when stage has no configured drops. The client opens
    // reward dialog from passRets presence; omitting it leaves getrewardspage with nil data.
    if copy_id <= 0 {
        return Vec::new();
    }
    vec![battle_pass_payload_with_rewards(
        copy_id, false, 3, 60, rewards,
    )]
}

pub(crate) fn draw_draw_count(multiplier: f64, seed: u64) -> usize {
    let multiplier = normalize_multiplier(multiplier);
    let whole = multiplier.floor() as usize;
    let fraction = multiplier - whole as f64;
    if fraction <= 0.0 {
        return whole;
    }
    let roll = (seed % 1_000_000) as f64 / 1_000_000.0;
    whole.saturating_add(usize::from(roll < fraction))
}

pub(crate) fn next_battle_drop_seed() -> u64 {
    mix_build_draw_roll(
        u64::from(current_unix_millis()).rotate_left(32)
            ^ BATTLE_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    )
}

pub(crate) fn draw_copy_drop_with_seed(
    catalog: &BattleCatalog,
    drop_id: i32,
    depth: u8,
    seed: u64,
) -> Option<ShopReward> {
    if depth >= 16 {
        return None;
    }
    catalog.drop_pools.get(&drop_id)?;
    draw_copy_drop_rewards_with_seed(catalog, drop_id, depth, seed)
        .into_iter()
        .next()
}

pub(crate) fn draw_copy_drop_rewards_with_seed(
    catalog: &BattleCatalog,
    drop_id: i32,
    depth: u8,
    seed: u64,
) -> Vec<ShopReward> {
    if depth >= 16 {
        return Vec::new();
    }
    let Some(pool) = catalog.drop_pools.get(&drop_id) else {
        return Vec::new();
    };
    let mut rewards = Vec::new();
    for index in 0..pool.random_count {
        if let Some(reward) = draw_drop_entry_with_seed(
            catalog,
            &pool.random_entries,
            depth,
            seed.wrapping_add(u64::try_from(index).unwrap_or_default()),
        ) {
            rewards.push(reward);
        }
    }
    let offset = u64::try_from(pool.random_count.max(0)).unwrap_or_default();
    for index in 0..pool.separate_count {
        if let Some(reward) = draw_drop_entry_with_seed(
            catalog,
            &pool.separate_entries,
            depth,
            seed.wrapping_add(offset)
                .wrapping_add(u64::try_from(index).unwrap_or_default()),
        ) {
            rewards.push(reward);
        }
    }
    rewards
}

pub(crate) fn draw_copy_category_rewards_with_seed(
    catalog: &BattleCatalog,
    drop_id: i32,
    seed: u64,
    include_first_clear: bool,
) -> Vec<ShopReward> {
    let Some(pool) = catalog.copy_drop_pools.get(&drop_id) else {
        return Vec::new();
    };
    let mut rewards = Vec::new();
    let mut offset = 0u64;
    if include_first_clear {
        for (index, entry) in pool.first_clear_entries.iter().enumerate() {
            if let Some(reward) =
                draw_drop_entry_direct(catalog, entry, 0, seed.wrapping_add(index as u64))
            {
                rewards.push(reward);
            }
        }
        offset = pool.first_clear_entries.len() as u64;
    }
    for (index, entry) in pool.guaranteed_entries.iter().enumerate() {
        if let Some(reward) =
            draw_drop_entry_direct(catalog, entry, 0, seed.wrapping_add(offset + index as u64))
        {
            rewards.push(reward);
        }
    }
    offset += pool.guaranteed_entries.len() as u64;
    if !pool.random_entries.is_empty() {
        for index in 0..pool.random_count {
            if let Some(reward) = draw_drop_entry_with_seed(
                catalog,
                &pool.random_entries,
                0,
                seed.wrapping_add(offset)
                    .wrapping_add(u64::try_from(index).unwrap_or_default()),
            ) {
                rewards.push(reward);
            }
        }
    }
    rewards
}

fn draw_drop_entry_with_seed(
    catalog: &BattleCatalog,
    entries: &[BuildDropEntry],
    depth: u8,
    seed: u64,
) -> Option<ShopReward> {
    if depth >= 16 {
        return None;
    }
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
        return draw_drop_entry_direct(
            catalog,
            entry,
            depth,
            mix_build_draw_roll(seed ^ 0xA076_1D64_78BD_642F),
        );
    }
    None
}

fn draw_drop_entry_direct(
    catalog: &BattleCatalog,
    entry: &BuildDropEntry,
    depth: u8,
    seed: u64,
) -> Option<ShopReward> {
    if entry.0 == 4 {
        return draw_copy_drop_with_seed(catalog, entry.1, depth + 1, seed);
    }
    Some(ShopReward {
        goods_type: entry.0,
        item_id: entry.1,
        num: entry.2.max(1),
        instance_id: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_drop_keeps_singletons_and_draws_one_per_candidate_group() {
        let mut catalog = BattleCatalog::default();
        catalog.copy_drop_pools.insert(
            20_0412,
            BattleCopyDropPool {
                first_clear_entries: vec![(5, 2, 50, 1, 1)],
                guaranteed_entries: vec![(5, 9, 1, 1, 1), (1, 13_001, 1, 1, 1)],
                random_entries: vec![(2, 30_012, 1, 1, 1), (2, 30_022, 1, 1, 1)],
                random_count: 1,
            },
        );

        let rewards = draw_copy_category_rewards_with_seed(&catalog, 20_0412, 7, true);

        assert_eq!(rewards.len(), 4);
        assert!(rewards
            .iter()
            .any(|reward| { reward.goods_type == 5 && reward.item_id == 2 && reward.num == 50 }));
        assert!(rewards
            .iter()
            .any(|reward| { reward.goods_type == 5 && reward.item_id == 9 && reward.num == 1 }));
        assert!(rewards.iter().any(|reward| {
            reward.goods_type == 1 && reward.item_id == 13_001 && reward.num == 1
        }));
        assert_eq!(
            rewards
                .iter()
                .filter(|reward| reward.goods_type == 2)
                .count(),
            1
        );

        let repeat_rewards = draw_copy_category_rewards_with_seed(&catalog, 20_0412, 7, false);
        assert_eq!(repeat_rewards.len(), 3);
        assert!(!repeat_rewards
            .iter()
            .any(|reward| reward.goods_type == 5 && reward.item_id == 2));
    }

    #[test]
    fn battle_drop_pools_draw_random_and_separate_categories() {
        let mut catalog = BattleCatalog::default();
        catalog.drop_pools.insert(
            33_0101,
            BattleDropPool {
                random_entries: vec![(1, 10_182, 1, 1, 1)],
                random_count: 1,
                separate_entries: vec![(1, 10_185, 1, 1, 1)],
                separate_count: 1,
            },
        );

        let rewards = draw_copy_drop_rewards_with_seed(&catalog, 33_0101, 0, 9);

        assert_eq!(rewards.len(), 2);
    }
}
