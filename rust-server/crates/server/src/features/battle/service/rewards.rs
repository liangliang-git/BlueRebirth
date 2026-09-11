use super::super::*;

use crate::common::response::{Response, ResponseEffects};
use crate::features::copy::mopup_state::{
    draw_copy_category_rewards_with_seed_multiplier, draw_copy_drop_rewards_with_seed_multiplier,
    next_battle_drop_seed,
};

// 抽取掉落奖励
pub(crate) fn draw_typed_battle_drop_rewards(
    catalog: &BattleCatalog,
    copy_id: i32,
    multiplier: f64,
    grade: i32,
    include_first_clear: bool,
) -> Vec<ShopReward> {
    let fleet_ids = battle_session_fleet_ids(copy_id, Some(catalog));
    draw_typed_battle_drop_rewards_for_fleets(
        catalog,
        copy_id,
        &fleet_ids,
        multiplier,
        grade,
        true,
        include_first_clear,
    )
}

pub(crate) fn draw_typed_battle_wave_drop_rewards(
    catalog: &BattleCatalog,
    copy_id: i32,
    fleet_ids: &[i32],
    multiplier: f64,
    grade: i32,
    include_stage_rewards: bool,
) -> Vec<ShopReward> {
    draw_typed_battle_drop_rewards_for_fleets(
        catalog,
        copy_id,
        fleet_ids,
        multiplier,
        grade,
        include_stage_rewards,
        false,
    )
}

pub(crate) fn draw_typed_battle_drop_rewards_for_fleets(
    catalog: &BattleCatalog,
    copy_id: i32,
    fleet_ids: &[i32],
    multiplier: f64,
    grade: i32,
    include_stage_rewards: bool,
    include_first_clear: bool,
) -> Vec<ShopReward> {
    let mut rewards = Vec::new();
    let mut draw_index = 0u64;
    let seed = next_battle_drop_seed();
    let settle_multiplier = multiplier * battle_evaluation_multipliers(Some(catalog), grade).1;
    let other_multiplier = multiplier * battle_other_drop_multiplier(Some(catalog), grade);
    let is_daily_copy = catalog.daily_group_by_copy.contains_key(&copy_id);
    if include_stage_rewards {
        let basic_drop_ids = if is_daily_copy {
            catalog.daily_basic_drop_ids_by_copy.get(&copy_id)
        } else {
            catalog.copy_drop_ids.get(&copy_id)
        };
        for drop_id in basic_drop_ids.into_iter().flatten() {
            let category_rewards = if is_daily_copy {
                draw_copy_drop_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    0,
                    seed.wrapping_add(draw_index),
                    multiplier,
                )
            } else {
                let include_display_first_clear =
                    include_first_clear && !catalog.copy_first_rewards.contains_key(&copy_id);
                draw_copy_category_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    seed.wrapping_add(draw_index),
                    include_display_first_clear,
                    multiplier,
                )
            };
            draw_index = draw_index.wrapping_add(1);
            for mut reward in category_rewards {
                catalog.drop_quantities.apply(
                    copy_id,
                    &mut reward,
                    seed.wrapping_add(draw_index)
                        .wrapping_add(0xD1B5_4A32_D192_ED03),
                );
                rewards.push(reward);
            }
        }

        // config_daily_group.extra_drop is the actual daily extra-reward pool.
        // Extra attempts are intentionally unlimited up to the wire-visible
        // 99,999 sentinel, so every successful daily run can draw this pool.
        if is_daily_copy {
            for drop_id in catalog
                .daily_extra_drop_ids_by_copy
                .get(&copy_id)
                .into_iter()
                .flatten()
            {
                let category_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    0,
                    seed.wrapping_add(draw_index),
                    multiplier,
                );
                draw_index = draw_index.wrapping_add(1);
                for mut reward in category_rewards {
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index)
                            .wrapping_add(0xD1B5_4A32_D192_ED03),
                    );
                    rewards.push(reward);
                }
            }
        }
    }

    for fleet_id in fleet_ids {
        for drop_id in catalog.fleet_drop_ids.get(&fleet_id).into_iter().flatten() {
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            for mut reward in fleet_rewards {
                draw_index = draw_index.wrapping_add(1);
                catalog
                    .drop_quantities
                    .apply(copy_id, &mut reward, seed.wrapping_add(draw_index));
                rewards.push(reward);
            }
        }
        for drop_id in catalog
            .fleet_other_drop_ids
            .get(&fleet_id)
            .into_iter()
            .flatten()
        {
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                other_multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            if !fleet_rewards.is_empty() {
                for mut reward in fleet_rewards {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    rewards.push(reward);
                }
            }
        }
        for drop_id in catalog
            .fleet_settle_drop_ids
            .get(&fleet_id)
            .into_iter()
            .flatten()
        {
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                settle_multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            if !fleet_rewards.is_empty() {
                for mut reward in fleet_rewards {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    rewards.push(reward);
                }
            }
        }
    }

    if include_stage_rewards {
        if let Some((count, guaranteed)) = catalog.copy_must_drop_rewards.get(&copy_id) {
            for _ in 0..*count {
                rewards.extend(
                    guaranteed
                        .iter()
                        .map(|(goods_type, item_id, num)| ShopReward {
                            goods_type: *goods_type,
                            item_id: *item_id,
                            num: *num,
                            instance_id: 0,
                        }),
                );
            }
        }
    }
    rewards
}

pub(crate) fn queue_battle_inventory_refresh(
    effects: &mut ResponseEffects,
    account: &blueoath_domain::AccountState,
    fashion_catalog: Option<&FashionList>,
) {
    // Battle rewards mutate several independent client caches. Sending only bag data
    // leaves equipment/ship/fashion drops invisible until a later full refresh.
    // Queue snapshots before the battle callback so the result screen cannot overwrite
    // them with its pre-settlement cache.
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    ));
    effects.push_pre(Response::raw(
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_typed_account(account)),
    ));
    effects.push_pre(Response::raw(
        "fashion.updateData",
        FashionListCodec::encode(&fashion_list_from_typed_account(account, fashion_catalog)),
    ));
}
