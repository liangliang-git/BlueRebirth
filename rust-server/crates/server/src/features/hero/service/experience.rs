//! Hero experience and level progression handlers.

use super::super::*;
/// 根据强类型英雄属性计算英雄最大生命值。
pub(super) fn typed_hero_max_hp(
    account: &blueoath_domain::AccountState,
    hero: &blueoath_domain::HeroState,
) -> u64 {
    ship_max_hp_for_typed_hero_with_heroes(
        hero,
        &account.activities.progress,
        &account.dock.equipments,
        SHIP_STAT_CATALOG.get(),
        EQUIP_CATALOG.get(),
        SHIP_REMOULD_CATALOG.get(),
        SHIP_STAT_MULTIPLIER.get().copied().unwrap_or(1.0),
        Some(&account.dock.heroes),
    )
}

/// 按升级前生命比例计算升级后的当前生命值。
pub(super) fn hp_after_level_up(current_hp: u64, old_max_hp: u64, new_max_hp: u64) -> u64 {
    if current_hp >= old_max_hp {
        new_max_hp
    } else {
        current_hp.min(new_max_hp)
    }
}

/// 在英雄升级后刷新其当前生命值并返回状态变化。
pub(super) fn refresh_typed_hero_hp_after_level_up(
    account: &mut blueoath_domain::AccountState,
    hero_id: blueoath_domain::HeroId,
    old_level: u32,
    new_level: u32,
) {
    if new_level <= old_level {
        return;
    }

    let Some(current_hero) = account.dock.heroes.get(&hero_id) else {
        return;
    };
    let mut old_hero = current_hero.clone();
    old_hero.level = old_level;
    let old_max_hp = typed_hero_max_hp(account, &old_hero);
    let mut leveled_hero = current_hero.clone();
    leveled_hero.level = new_level;
    let new_max_hp = typed_hero_max_hp(account, &leveled_hero);
    let new_hp = hp_after_level_up(current_hero.hp, old_max_hp, new_max_hp);

    if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
        hero.hp = new_hp;
    }
}
