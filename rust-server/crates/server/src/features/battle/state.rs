#![allow(dead_code)]

#[cfg(test)]
use serde_json::Value;

use super::*;

pub(crate) fn encode_random_factor_payload(
    copy_id: i32,
    battle_catalog: Option<&BattleCatalog>,
) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(entries) = battle_catalog.and_then(|catalog| catalog.random_factors.get(&copy_id)) {
        for entry in entries {
            let mut factor = Vec::new();
            for value in &entry.factors {
                append_varint_field(&mut factor, 1, (*value).max(0) as u64);
            }
            if entry.group_id > 0 {
                append_varint_field(&mut factor, 2, entry.group_id as u64);
            }
            if entry.set_id > 0 {
                append_varint_field(&mut factor, 3, entry.set_id as u64);
            }
            append_message_field(&mut output, 1, &factor);
        }
    }
    output
}
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BattleStartOptions {
    pub(crate) is_running_fight: bool,
    pub(crate) battle_mode: i32,
    pub(crate) anim_mode: i32,
    pub(crate) match_type: i32,
}

pub(crate) fn battle_start_payload_from_typed_account(
    account: &blueoath_domain::AccountState,
    copy_id: i32,
    requested_hero_groups: &[Vec<i32>],
    battle_catalog: Option<&BattleCatalog>,
    ship_stat_multiplier: f64,
    options: BattleStartOptions,
) -> Vec<u8> {
    let groups = if requested_hero_groups.is_empty() {
        vec![account
            .fleet
            .fleets
            .values()
            .next()
            .map(|fleet| {
                fleet
                    .members
                    .iter()
                    .filter_map(|id| i32::try_from(id.get()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()]
    } else {
        requested_hero_groups.to_vec()
    };
    let mut players = Vec::new();
    let mut extra_player_lists = Vec::new();
    for (fleet_index, requested_ids) in groups.iter().enumerate() {
        let selected = requested_ids
            .iter()
            .filter_map(|id| u64::try_from(*id).ok())
            .filter_map(|id| {
                account
                    .dock
                    .heroes
                    .values()
                    .find(|hero| hero.id.get() == id)
            })
            .take(6)
            .collect::<Vec<_>>();
        let selected = if selected.is_empty() {
            account
                .fleet
                .fleets
                .values()
                .next()
                .map(|fleet| {
                    fleet
                        .members
                        .iter()
                        .filter_map(|id| account.dock.heroes.get(id))
                        .take(6)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            selected
        };
        let mut fleet_payload = Vec::new();
        append_varint_field(
            &mut fleet_payload,
            1,
            (fleet_index as u64).saturating_add(1),
        );
        append_varint_field(&mut fleet_payload, 2, 2);
        append_varint_field(&mut fleet_payload, 3, fleet_index as u64);
        for (position, hero) in selected.iter().enumerate() {
            let template_id = hero.template_id.get();
            let mut ship = Vec::new();
            append_varint_field(&mut ship, 1, hero.id.get());
            append_varint_field(&mut ship, 2, template_id);
            append_varint_field(&mut ship, 3, u64::from(hero.level.max(1)));
            append_varint_field(&mut ship, 4, position as u64);
            // BattleStartData does not calculate player attributes when this repeated
            // field is absent. It initializes max HP to 1, which makes every ship appear
            // at 1 HP. Keep field 5 for battle runtime input; HeroGrid remains raw and
            // does not expose server-computed display attributes.
            let computed_attributes = ship_attributes_for_typed_hero_with_heroes(
                hero,
                &account.activities.progress,
                &account.dock.equipments,
                SHIP_STAT_CATALOG.get(),
                EQUIP_CATALOG.get(),
                SHIP_REMOULD_CATALOG.get(),
                ship_stat_multiplier,
                Some(&account.dock.heroes),
            );
            // BattleStartData must receive final values, including equipment,
            // combination, remould and non-primary attributes. Client panel
            // keeps its own calculation path; battle runtime consumes this set.
            for (attr_id, value) in &computed_attributes {
                let mut attr = Vec::new();
                append_varint_field(&mut attr, 1, *attr_id as u64);
                append_varint_field(&mut attr, 2, (*value).max(0) as u64);
                append_message_field(&mut ship, 5, &attr);
            }
            // TStartBaseHero.CurHp uses the same fixed-point ratio as HeroGrid.CurHp.
            // The typed account stores absolute HP, so convert before sending.
            append_varint_field(
                &mut ship,
                6,
                typed_hero_cur_hp_for_client(account, hero) as u64,
            );
            append_varint_field(&mut ship, 11, 3);
            let fashioning = if hero.fashioning > 0 {
                u64::from(hero.fashioning)
            } else {
                template_id.saturating_sub(1) / 10
            };
            append_varint_field(&mut ship, 12, fashioning);

            let mut encoded_skill = false;
            if hero.pskills.is_empty() {
                let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
                if let Some(configured_skills) = HERO_SKILL_CATALOG
                    .get()
                    .and_then(|catalog| catalog.get(&template_id))
                {
                    for skill_id in configured_skills {
                        if *skill_id <= 0 {
                            continue;
                        }
                        let mut skill = Vec::new();
                        append_varint_field(
                            &mut skill,
                            1,
                            resolved_battle_skill_id(
                                &account.activities.progress,
                                hero.id.get(),
                                *skill_id as u64,
                            ),
                        );
                        append_varint_field(&mut skill, 2, 1);
                        append_message_field(&mut ship, 8, &skill);
                        encoded_skill = true;
                    }
                }
            } else {
                for (&skill_id, &skill_level) in &hero.pskills {
                    if skill_id == 0 {
                        continue;
                    }
                    let mut skill = Vec::new();
                    append_varint_field(
                        &mut skill,
                        1,
                        resolved_battle_skill_id(
                            &account.activities.progress,
                            hero.id.get(),
                            skill_id,
                        ),
                    );
                    append_varint_field(&mut skill, 2, u64::from(skill_level.max(1)));
                    append_message_field(&mut ship, 8, &skill);
                    encoded_skill = true;
                }
            }
            if !encoded_skill {
                let mut skill = Vec::new();
                append_varint_field(&mut skill, 1, 41210);
                append_varint_field(&mut skill, 2, 1);
                append_message_field(&mut ship, 8, &skill);
            }

            let mut battle_equip_index = 0_u64;
            for equip_id in hero.equip_slots.iter().flatten() {
                let Some(equipment) = account.dock.equipments.get(equip_id) else {
                    continue;
                };
                let mut equip = Vec::new();
                append_varint_field(&mut equip, 1, equipment.template_id.get());
                append_varint_field(&mut equip, 2, battle_equip_index);
                // TBattleEquip.PlaneNum is required by client air-attack setup.
                append_varint_field(&mut equip, 3, 100);
                if let Some(equip_catalog) = EQUIP_CATALOG.get() {
                    let template_id =
                        i32::try_from(equipment.template_id.get()).unwrap_or_default();
                    let mut properties = std::collections::BTreeMap::<i32, i64>::new();
                    for (attr_id, value) in equip_catalog
                        .prop_by_template
                        .get(&template_id)
                        .into_iter()
                        .flatten()
                    {
                        properties
                            .entry(*attr_id)
                            .and_modify(|current| *current = current.saturating_add(*value))
                            .or_insert(*value);
                    }
                    for (attr_id, value) in equip_catalog
                        .enhance_prop_by_template
                        .get(&template_id)
                        .into_iter()
                        .flatten()
                    {
                        let value = value.saturating_mul(i64::from(equipment.enhance_level));
                        properties
                            .entry(*attr_id)
                            .and_modify(|current| *current = current.saturating_add(value))
                            .or_insert(value);
                    }
                    for (attr_id, value) in properties {
                        if attr_id <= 0 || value < 0 {
                            continue;
                        }
                        let mut property = Vec::new();
                        append_varint_field(&mut property, 1, attr_id as u64);
                        append_varint_field(&mut property, 2, value as u64);
                        append_message_field(&mut equip, 4, &property);
                    }
                    if equipment.star > 0 {
                        if let Some(skills) = equip_catalog.skills_by_template.get(&template_id) {
                            for (skill_id, max_level) in skills {
                                if *skill_id <= 0 {
                                    continue;
                                }
                                let mut skill = Vec::new();
                                append_varint_field(&mut skill, 1, *skill_id as u64);
                                append_varint_field(
                                    &mut skill,
                                    2,
                                    i32::try_from(equipment.star)
                                        .unwrap_or(i32::MAX)
                                        .min(*max_level)
                                        .max(1) as u64,
                                );
                                append_message_field(&mut equip, 5, &skill);
                            }
                        }
                    }
                }
                append_message_field(&mut ship, 7, &equip);
                battle_equip_index = battle_equip_index.saturating_add(1);
            }
            if let Some(bathroom_hero) = account
                .bathroom
                .heroes
                .iter()
                .find(|bathroom_hero| bathroom_hero.hero_id == hero.id.get())
            {
                if bathroom_hero.buff_id > 0
                    && bathroom_hero.buff_time > u64::from(current_unix_seconds())
                {
                    append_varint_field(&mut ship, 9, u64::from(bathroom_hero.buff_id));
                }
            }
            if let Some(break_config) = SHIP_BREAK_CATALOG.get().and_then(|catalog| {
                i32::try_from(template_id)
                    .ok()
                    .and_then(|id| catalog.by_template.get(&id))
            }) {
                for effect_id in &break_config.ship_break_effect_ids {
                    if *effect_id > 0 {
                        append_varint_field(&mut ship, 10, *effect_id as u64);
                    }
                }
            }
            append_message_field(&mut fleet_payload, 4, &ship);
            append_varint_field(&mut fleet_payload, 8, hero.id.get());
        }
        append_varint_field(&mut fleet_payload, 5, 0);
        append_varint_field(&mut fleet_payload, 7, 0);
        append_varint_field(&mut fleet_payload, 9, 1);
        let player = encode_battle_player(
            &fleet_payload,
            fleet_index,
            account.character.uid.max(1),
            u64::from(account.character.level.max(1)),
            account.character.name.as_bytes(),
        );
        if fleet_index == 0 {
            append_message_field(&mut players, 1, &player);
        } else {
            let mut list = Vec::new();
            append_message_field(&mut list, 1, &player);
            extra_player_lists.push(list);
        }
    }

    let mut output = Vec::new();
    append_message_field(&mut output, 1, &players);
    for list in extra_player_lists {
        append_message_field(&mut output, 15, &list);
    }
    append_varint_field(&mut output, 2, u64::from(current_unix_seconds()));
    // Client CopyEnter requires TStartBaseRet.Rid before it enters battle.
    // Co-op and normal PVE use same response contract.
    append_varint_field(
        &mut output,
        3,
        u64::from(current_unix_seconds()).saturating_add(account.character.uid % 997),
    );
    let copy_info = battle_catalog.and_then(|catalog| catalog.copies.get(&copy_id));
    append_varint_field(
        &mut output,
        3,
        copy_info
            .map(|copy| copy.config_id)
            .unwrap_or(copy_id.max(1))
            .max(1) as u64,
    );
    for fleet_id in battle_session_fleet_ids(copy_id, battle_catalog) {
        append_varint_field(&mut output, 5, fleet_id.max(1) as u64);
    }
    append_varint_field(&mut output, 6, copy_id.max(1) as u64);
    append_varint_field(
        &mut output,
        7,
        copy_info.map(|copy| copy.copy_type).unwrap_or(1).max(1) as u64,
    );
    append_varint_field(
        &mut output,
        8,
        u64::from(
            account
                .battle
                .passed_copies
                .contains(&blueoath_domain::CopyId::new(copy_id.max(1) as u64).unwrap()),
        ),
    );
    append_varint_field(&mut output, 10, u64::from(options.is_running_fight));
    append_bytes_field(&mut output, 16, b"1111111111111111111111111111111111111");
    append_varint_field(&mut output, 18, options.battle_mode.max(0) as u64);
    for mission_id in [101, 102, 103] {
        append_varint_field(&mut output, 23, mission_id);
    }
    append_varint_field(&mut output, 13, 0);

    let is_search_3d = battle_catalog.is_some_and(|catalog| catalog.search_3d.contains(&copy_id));
    if is_search_3d {
        // Search battles initialize movement/search state from these fields.
        // Omitting them leaves the client sea scene visible but input-disabled.
        append_varint_field(&mut output, 19, 0);
        append_varint_field(&mut output, 20, options.anim_mode.max(0) as u64);
        append_varint_field(&mut output, 22, 0);
        for (kind, value) in [(50000_i32, 1_i32), (0_i32, 1_i32)] {
            let mut config = Vec::new();
            if kind != 0 {
                append_varint_field(&mut config, 1, kind as u64);
            }
            append_varint_field(&mut config, 2, value as u64);
            append_message_field(&mut output, 25, &config);
        }

        // Field 17 carries model/VCR identities. Enemy combat attributes are
        // also sent below in field 24 so battle setup does not depend on a
        // second, potentially mismatched enemy-stat source.
        let mut sent_vcr = std::collections::HashSet::new();
        let mut emit_vcr = |ship_info_id: i32| {
            if ship_info_id <= 0 || !sent_vcr.insert(ship_info_id) {
                return;
            }
            let mut vcr = Vec::new();
            append_varint_field(&mut vcr, 1, ship_info_id as u64);
            append_varint_field(&mut vcr, 2, 1);
            append_varint_field(&mut vcr, 3, 1);
            append_message_field(&mut output, 17, &vcr);
        };
        for requested_ids in &groups {
            for hero in requested_ids
                .iter()
                .filter_map(|id| u64::try_from(*id).ok())
                .filter_map(|id| {
                    account
                        .dock
                        .heroes
                        .values()
                        .find(|hero| hero.id.get() == id)
                })
                .take(6)
            {
                let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
                emit_vcr(((template_id - 1) / 10).max(1));
            }
        }
        if groups.iter().all(Vec::is_empty) {
            if let Some(fleet) = account.fleet.fleets.values().next() {
                for hero in fleet
                    .members
                    .iter()
                    .filter_map(|id| account.dock.heroes.get(id))
                    .take(6)
                {
                    let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
                    emit_vcr(((template_id - 1) / 10).max(1));
                }
            }
        }
        if let Some(catalog) = battle_catalog {
            for fleet_id in battle_session_fleet_ids(copy_id, Some(catalog)) {
                for enemy_id in catalog.fleet_enemies.get(&fleet_id).into_iter().flatten() {
                    if let Some(ship_info_id) = catalog
                        .enemies
                        .get(enemy_id)
                        .map(|enemy| enemy.ship_info_id)
                    {
                        emit_vcr(ship_info_id);
                    }
                }
            }
        }
    }
    append_enemy_fleet_payload(&mut output, copy_id, battle_catalog);
    if options.match_type > 0 {
        append_varint_field(&mut output, 26, options.match_type as u64);
    }
    output
}

fn resolved_battle_skill_id(
    progress: &std::collections::BTreeMap<String, u64>,
    hero_id: u64,
    skill_id: u64,
) -> u64 {
    let mut resolved = skill_id;
    for _ in 0..8 {
        let key = format!("compat:hero:{hero_id}:pskill:{resolved}:replace");
        let Some(next) = progress.get(&key).copied().filter(|value| *value > 0) else {
            break;
        };
        if next == resolved {
            break;
        }
        resolved = next;
    }
    resolved
}

fn append_enemy_fleet_payload(
    output: &mut Vec<u8>,
    copy_id: i32,
    battle_catalog: Option<&BattleCatalog>,
) {
    let Some(catalog) = battle_catalog else {
        return;
    };
    for fleet_id in battle_session_fleet_ids(copy_id, Some(catalog)) {
        let Some(enemy_ids) = catalog.fleet_enemies.get(&fleet_id) else {
            continue;
        };
        let mut fleet = Vec::new();
        append_varint_field(&mut fleet, 1, fleet_id.max(1) as u64);
        append_varint_field(&mut fleet, 2, 0);
        let mut encoded_enemy = false;
        for enemy_id in enemy_ids {
            let Some(enemy) = catalog.enemies.get(enemy_id) else {
                continue;
            };
            let mut ship = Vec::new();
            append_varint_field(&mut ship, 1, (*enemy_id).max(1) as u64);
            for (attr_id, value) in [
                (1, enemy.hp),
                (8, enemy.attack),
                (9, enemy.defense),
                (10, enemy.torpedo),
                (11, enemy.torpedo_defense),
                (19, enemy.hit),
                (20, enemy.dodge),
            ] {
                if value < 0 {
                    continue;
                }
                let mut attr = Vec::new();
                append_varint_field(&mut attr, 1, attr_id);
                append_varint_field(&mut attr, 2, value as u64);
                append_message_field(&mut ship, 2, &attr);
            }
            append_message_field(&mut fleet, 3, &ship);
            encoded_enemy = true;
        }
        if encoded_enemy {
            append_message_field(output, 24, &fleet);
        }
    }
}

fn encode_battle_player(
    fleet: &[u8],
    fleet_index: usize,
    uid: u64,
    level: u64,
    name: &[u8],
) -> Vec<u8> {
    let mut player = Vec::new();
    append_varint_field(&mut player, 1, uid);
    append_varint_field(&mut player, 2, uid);
    append_bytes_field(&mut player, 3, name);
    append_varint_field(&mut player, 4, level);
    append_varint_field(&mut player, 5, 1);
    append_varint_field(&mut player, 6, fleet_index as u64);
    append_message_field(&mut player, 7, fleet);
    player
}

pub(crate) fn battle_fleet_ids(copy_id: i32, battle_catalog: Option<&BattleCatalog>) -> Vec<i32> {
    let Some(catalog) = battle_catalog else {
        return vec![copy_id.max(1)];
    };
    catalog
        .copies
        .get(&copy_id)
        .map(|copy| copy.fleet_ids.clone())
        .filter(|ids| !ids.is_empty())
        .unwrap_or_else(|| vec![copy_id.max(1)])
}

pub(crate) fn battle_session_fleet_ids(
    copy_id: i32,
    battle_catalog: Option<&BattleCatalog>,
) -> Vec<i32> {
    let direct = battle_fleet_ids(copy_id, battle_catalog);
    let Some(catalog) = battle_catalog else {
        return direct;
    };
    let mut result = Vec::new();
    let mut pending = direct.into_iter().rev().collect::<Vec<_>>();
    let mut seen = std::collections::HashSet::new();
    while let Some(fleet_id) = pending.pop() {
        if !seen.insert(fleet_id) {
            continue;
        }
        result.push(fleet_id);
        if let Some(attached) = catalog.attached_fleet_ids.get(&fleet_id) {
            pending.extend(attached.iter().rev().copied());
        }
    }
    result
}

pub(crate) fn battle_position_fleet_id(
    copy_id: i32,
    fleet_id: i32,
    fleet_index: usize,
    battle_catalog: Option<&BattleCatalog>,
) -> i32 {
    let Some(catalog) = battle_catalog else {
        return fleet_id;
    };
    if !catalog.search_3d.contains(&copy_id) || fleet_index != 0 {
        return fleet_id;
    }
    // Activity challenge fleets carry their own battlefield/search config.
    // Replacing them with the 1-A anchor makes the client load wrong map
    // orientation and movement data.
    if catalog.copies.get(&copy_id).is_some_and(|copy| {
        ((202_431..=202_464).contains(&copy_id) || copy.copy_type == 33)
            && copy.fleet_ids.contains(&fleet_id)
    }) {
        return fleet_id;
    }
    // Client reads config_fleet.is_last_fleet from FleetId. The 1-A anchor is
    // marked final in the JP client; replacing a non-final fleet with it makes
    // 1-1 end after its first encounter.
    if catalog
        .fleet_is_last
        .get(&fleet_id)
        .is_some_and(|is_last| !is_last)
    {
        return fleet_id;
    }
    const SEA_POSITION_ANCHOR_FLEET_ID: i32 = 160010000;
    let has_anchor = catalog
        .copies
        .get(&copy_id)
        .is_some_and(|copy| copy.fleet_ids.contains(&SEA_POSITION_ANCHOR_FLEET_ID));
    if has_anchor {
        fleet_id
    } else {
        SEA_POSITION_ANCHOR_FLEET_ID
    }
}

pub(crate) fn battle_fleet_aliases(
    copy_id: i32,
    battle_catalog: Option<&BattleCatalog>,
) -> Vec<(i32, i32)> {
    battle_session_fleet_ids(copy_id, battle_catalog)
        .into_iter()
        .enumerate()
        .filter_map(|(index, real_id)| {
            let wire_id = battle_position_fleet_id(copy_id, real_id, index, battle_catalog);
            (wire_id != real_id).then_some((wire_id, real_id))
        })
        .collect()
}

pub(crate) fn battle_attack_payload_from_request(
    request: &blueoath_protocol::CopyAttackRequest,
    damage: u64,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, request.attack_type);
    append_varint_field(&mut output, 2, request.copy_id);
    for hero_id in &request.hero_ids {
        append_varint_field(&mut output, 3, *hero_id);
    }
    append_varint_field(&mut output, 4, request.enemy_id);
    append_varint_field(&mut output, 5, damage);
    output
}

pub(crate) fn copy_progress_max_or_first(catalog_ids: &[i32], passed_ids: &[i32]) -> i32 {
    passed_ids
        .iter()
        .copied()
        .max()
        .or_else(|| catalog_ids.first().copied())
        .unwrap_or_default()
}

pub(crate) fn copy_progress_max_or_initial(
    catalog_ids: &[i32],
    passed_ids: &[i32],
    initial_copy_id: i32,
) -> i32 {
    passed_ids
        .iter()
        .copied()
        .max()
        .or_else(|| {
            (initial_copy_id > 0 && catalog_ids.contains(&initial_copy_id))
                .then_some(initial_copy_id)
        })
        .or_else(|| catalog_ids.first().copied())
        .unwrap_or_default()
}

pub(crate) fn battle_supply_cost_typed(
    account: &blueoath_domain::AccountState,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    hero_ids: &[u64],
    count: i32,
) -> Option<u64> {
    if !(1..=99).contains(&count) || hero_ids.is_empty() {
        return None;
    }
    let Some(catalog) = catalog else {
        return None;
    };
    // Mubar defense-ring sorties consume sortie points in the original client,
    // not the normal fuel resource. Keep server-side validation while making
    // this battle type fuel-free even when display config contains stale costs.
    let (base, factor) = if catalog
        .copies
        .get(&copy_id)
        .is_some_and(|copy| copy.copy_type == 33)
    {
        (0, 0)
    } else {
        catalog
            .supply_cost_by_copy
            .get(&copy_id)
            .copied()
            .unwrap_or_default()
    };
    let mut ship_cost = 0_i64;
    let mut seen = std::collections::HashSet::new();
    for id in hero_ids {
        if !seen.insert(*id) {
            continue;
        }
        let Some(hero) = account
            .dock
            .heroes
            .values()
            .find(|hero| hero.id.get() == *id)
        else {
            return None;
        };
        if factor > 0 {
            let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
            ship_cost = ship_cost.saturating_add(
                catalog
                    .ship_supply_cost
                    .get(&template_id)
                    .copied()
                    .unwrap_or_default(),
            );
        }
    }
    let cost = base
        .saturating_add(ship_cost.saturating_mul(factor).saturating_add(9999) / 10000)
        .saturating_mul(i64::from(count));
    u64::try_from(cost.max(0)).ok()
}

pub(crate) fn consume_battle_supply_typed(
    account: &mut blueoath_domain::AccountState,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    hero_ids: &[u64],
    count: i32,
) -> bool {
    let Some(cost) = battle_supply_cost_typed(account, catalog, copy_id, hero_ids, count) else {
        return false;
    };
    account
        .resources
        .debit(blueoath_domain::CurrencyKind::Supply, cost)
        .is_ok()
}

pub(crate) fn apply_battle_settlement_typed(
    account: &mut blueoath_domain::AccountState,
    hero_ids: &[blueoath_domain::HeroId],
    mvp_hero_id: Option<u64>,
    shipwrecked_ids: &std::collections::HashSet<u64>,
    rule: BattleSettlementRule,
    affection_multiplier: f64,
) -> bool {
    let mut changed = false;
    for (position, hero_id) in hero_ids.iter().enumerate() {
        let Some(hero) = account.dock.heroes.get_mut(hero_id) else {
            continue;
        };
        let mood = i64::from(hero.mood.min(MOOD_MAX as u32));
        let mut affection_gain = if mood > i64::from(MOOD_MIN) {
            i64::from(rule.affection_add.max(0))
        } else {
            0
        };
        if mood > i64::from(MOOD_MIN) && position == 0 {
            affection_gain =
                affection_gain.saturating_add(i64::from(rule.affection_flagship_add.max(0)));
        }
        if mood > i64::from(MOOD_MIN) && mvp_hero_id == Some(hero_id.get()) {
            affection_gain =
                affection_gain.saturating_add(i64::from(rule.affection_mvp_add.max(0)));
        }
        let mood_multiplier = if mood >= MOOD_AFFECTION_BONUS_THRESHOLD {
            1.2
        } else {
            1.0
        };
        let mut affection_delta =
            scale_reward(affection_gain, affection_multiplier * mood_multiplier);
        if shipwrecked_ids.contains(&hero_id.get()) {
            affection_delta =
                affection_delta.saturating_sub(i64::from(rule.affection_reduce.max(0)));
        }
        let affection = i64::try_from(hero.affection).unwrap_or(i64::MAX);
        let next_affection = affection
            .saturating_add(affection_delta)
            .clamp(0, 1_000_000) as u64;
        if next_affection != hero.affection {
            hero.affection = next_affection;
            changed = true;
        }

        let mood_reduce = if shipwrecked_ids.contains(&hero_id.get()) {
            rule.mood_shipwrecks_reduce
        } else {
            rule.mood_reduce
        };
        let next_mood = mood
            .saturating_sub(i64::from(mood_reduce.max(0)))
            .clamp(i64::from(MOOD_MIN), i64::from(MOOD_MAX)) as u32;
        if next_mood != hero.mood {
            hero.mood = next_mood;
            changed = true;
        }
    }
    changed
}

pub(crate) fn battle_copy_experience(catalog: Option<&BattleCatalog>, copy_id: i32) -> (i32, i32) {
    let Some(catalog) = catalog else {
        return (100, 0);
    };
    if !catalog.copies.contains_key(&copy_id) {
        return (100, 0);
    }
    battle_fleet_experience(
        Some(catalog),
        &battle_session_fleet_ids(copy_id, Some(catalog)),
    )
}

pub(crate) fn battle_fleet_experience(
    catalog: Option<&BattleCatalog>,
    fleet_ids: &[i32],
) -> (i32, i32) {
    let Some(catalog) = catalog else {
        return (100, 0);
    };
    let (mut commander_exp, mut ship_exp) = (0i32, 0i32);
    for fleet_id in fleet_ids {
        if let Some(reward) = catalog.fleet_rewards.get(&fleet_id) {
            commander_exp = commander_exp.saturating_add(reward.commander_exp);
            ship_exp = ship_exp.saturating_add(reward.ship_exp);
        }
    }
    if commander_exp <= 0 {
        commander_exp = 100;
    }
    (commander_exp, ship_exp)
}

pub(crate) fn add_commander_battle_exp_typed(
    account: &mut blueoath_domain::AccountState,
    gained: u64,
    catalog: Option<&CommanderLevelCatalog>,
) {
    let mut remaining = gained;
    let max_level = catalog
        .map(CommanderLevelCatalog::max_level)
        .and_then(|level| u32::try_from(level).ok())
        .unwrap_or(100);
    while remaining > 0 && account.character.level < max_level {
        let Some(need) = catalog
            .and_then(|catalog| {
                catalog
                    .exp_needed
                    .get(&i32::try_from(account.character.level).unwrap_or(i32::MAX))
            })
            .copied()
            .filter(|need| *need > 0)
            .map(|need| need as u64)
        else {
            account.character.exp = account.character.exp.saturating_add(remaining);
            return;
        };
        let total = account.character.exp.saturating_add(remaining);
        if total < need {
            account.character.exp = total;
            return;
        }
        remaining = total.saturating_sub(need);
        account.character.exp = 0;
        account.character.level = account.character.level.saturating_add(1);
    }
    if account.character.level >= max_level {
        account.character.level = max_level;
        account.character.exp = 0;
    }
}

pub(crate) fn add_ship_battle_exp_typed(
    account: &mut blueoath_domain::AccountState,
    hero_ids: &[blueoath_domain::HeroId],
    gained: u64,
    catalog: Option<&HeroLevelCatalog>,
) -> Vec<(u64, i32)> {
    if gained == 0 {
        return Vec::new();
    }
    let mut rewards = Vec::new();
    for hero_id in hero_ids {
        let Some(hero) = account.dock.heroes.get_mut(hero_id) else {
            continue;
        };
        let gained = scale_reward(
            i64::try_from(gained).unwrap_or(i64::MAX),
            if i64::from(hero.mood) >= MOOD_AFFECTION_BONUS_THRESHOLD {
                1.2
            } else {
                1.0
            },
        )
        .max(0) as u64;
        rewards.push((hero_id.get(), i32::try_from(gained).unwrap_or(i32::MAX)));
        let mut remaining = gained;
        let max_level = catalog
            .map(HeroLevelCatalog::max_level)
            .and_then(|level| u32::try_from(level).ok())
            .unwrap_or(100);
        while remaining > 0 && hero.level < max_level {
            let Some(need) = catalog
                .and_then(|catalog| {
                    catalog
                        .exp_needed
                        .get(&i32::try_from(hero.level).unwrap_or(i32::MAX))
                })
                .copied()
                .filter(|need| *need > 0)
                .map(|need| need as u64)
            else {
                hero.exp = hero.exp.saturating_add(remaining);
                break;
            };
            let total = hero.exp.saturating_add(remaining);
            if total < need {
                hero.exp = total;
                remaining = 0;
            } else {
                remaining = total.saturating_sub(need);
                hero.exp = 0;
                hero.level = hero.level.saturating_add(1);
            }
        }
        if hero.level >= max_level {
            hero.level = max_level;
            hero.exp = 0;
        }
    }
    rewards
}

pub(crate) fn battle_evaluation_multipliers(
    catalog: Option<&BattleCatalog>,
    grade: i32,
) -> (f64, f64) {
    let Some(rule) = catalog.and_then(|catalog| catalog.evaluation_by_grade.get(&grade)) else {
        return (1.0, 1.0);
    };
    (
        f64::from(rule.exp_ratio) / 10_000.0,
        f64::from(rule.settle_drop_ratio) / 10_000.0,
    )
}

pub(crate) fn battle_other_drop_multiplier(catalog: Option<&BattleCatalog>, grade: i32) -> f64 {
    catalog
        .and_then(|catalog| catalog.evaluation_by_grade.get(&grade))
        .map(|rule| f64::from(rule.other_drop_ratio) / 10_000.0)
        .unwrap_or(1.0)
}

fn increment_daily_group_success(daily: &mut serde_json::Map<String, Value>, group_id: i32) {
    let groups = daily
        .entry("groups".to_owned())
        .or_insert_with(|| json!([]));
    let Some(groups) = groups.as_array_mut() else {
        return;
    };
    if let Some(group) = groups.iter_mut().find(|value| {
        value
            .get("dailyGroupId")
            .and_then(Value::as_i64)
            .is_some_and(|id| id == i64::from(group_id))
    }) {
        let success_times = group
            .get("successTimes")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0)
            .saturating_add(1);
        group["successTimes"] = json!(success_times);
    } else {
        groups.push(json!({"dailyGroupId": group_id, "successTimes": 1}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains_field(payload: &[u8], wanted_field: u64) -> bool {
        let mut index = 0;
        while index < payload.len() {
            let Ok((key, next)) = read_varint(payload, index) else {
                return false;
            };
            if key >> 3 == wanted_field {
                return true;
            }
            if key & 7 == 2 {
                let Ok((length, body_start)) = read_varint(payload, next) else {
                    return false;
                };
                let Ok(length) = usize::try_from(length) else {
                    return false;
                };
                let Some(body_end) = body_start.checked_add(length) else {
                    return false;
                };
                if body_end > payload.len() {
                    return false;
                }
                if contains_field(&payload[body_start..body_end], wanted_field) {
                    return true;
                }
                index = body_end;
                continue;
            }
            let Some(next) = skip_wire(payload, next, key & 7) else {
                return false;
            };
            index = next;
        }
        false
    }

    fn contains_varint(payload: &[u8], wanted_field: u64, wanted_value: u64) -> bool {
        let mut index = 0;
        while index < payload.len() {
            let Ok((key, next)) = read_varint(payload, index) else {
                return false;
            };
            if key & 7 == 0 {
                let Ok((value, next)) = read_varint(payload, next) else {
                    return false;
                };
                if key >> 3 == wanted_field && value == wanted_value {
                    return true;
                }
                index = next;
            } else if key & 7 == 2 {
                let Ok((length, body_start)) = read_varint(payload, next) else {
                    return false;
                };
                let Ok(length) = usize::try_from(length) else {
                    return false;
                };
                let Some(body_end) = body_start.checked_add(length) else {
                    return false;
                };
                if body_end > payload.len() {
                    return false;
                }
                if contains_varint(&payload[body_start..body_end], wanted_field, wanted_value) {
                    return true;
                }
                index = body_end;
            } else {
                let Some(next) = skip_wire(payload, next, key & 7) else {
                    return false;
                };
                index = next;
            }
        }
        false
    }

    #[test]
    fn battle_start_includes_real_hero_loadout() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("battle-loadout").unwrap(),
            "Captain",
        );
        let hero_id = *account.dock.heroes.keys().next().expect("starter hero");
        let equip_id = blueoath_domain::EquipId::new(99).unwrap();
        let hero = account.dock.heroes.get_mut(&hero_id).expect("hero");
        hero.fashioning = 987_654;
        hero.pskills.insert(99_001, 3);
        hero.equip_slots[0] = Some(equip_id);
        account.dock.equipments.insert(
            equip_id,
            blueoath_domain::EquipmentState {
                id: equip_id,
                template_id: blueoath_domain::TemplateId::new(88_001).unwrap(),
                enhance_level: 2,
                star: 0,
                enhance_exp: 0,
                hero_id: Some(hero_id),
            },
        );

        let payload = battle_start_payload_from_typed_account(
            &account,
            100,
            &[vec![hero_id.get() as i32]],
            None,
            1.0,
            BattleStartOptions::default(),
        );

        assert!(blueoath_protocol::decode_varint_fields(&payload)
            .unwrap()
            .contains_key(&3));
        assert!(contains_varint(&payload, 12, 987_654));
        assert!(contains_varint(&payload, 1, 99_001));
        assert!(contains_varint(&payload, 1, 88_001));
        assert!(contains_field(&payload, 7));
        assert!(contains_field(&payload, 8));
    }

    #[test]
    fn battle_start_includes_enemy_fleet_stats() {
        let mut catalog = BattleCatalog::default();
        catalog.copies.insert(
            100,
            BattleCopy {
                config_id: 100,
                copy_type: 1,
                fleet_ids: vec![200],
            },
        );
        catalog.fleet_is_last.insert(200, true);
        catalog.fleet_enemies.insert(200, vec![300]);
        catalog.enemies.insert(
            300,
            BattleEnemy {
                hp: 2_000,
                attack: 300,
                defense: 150,
                hit: 100,
                dodge: 20,
                torpedo: 40,
                torpedo_defense: 30,
                ..BattleEnemy::default()
            },
        );

        let mut payload = Vec::new();
        append_enemy_fleet_payload(&mut payload, 100, Some(&catalog));

        assert!(contains_field(&payload, 24));
        assert!(contains_varint(&payload, 1, 300));
        assert!(contains_varint(&payload, 2, 2_000));
        assert!(contains_varint(&payload, 2, 300));
    }

    #[test]
    fn mubar_search_keeps_its_own_position_fleet() {
        let mut catalog = BattleCatalog::default();
        catalog.search_3d.insert(932_113);
        catalog.copies.insert(
            932_113,
            BattleCopy {
                config_id: 932_113,
                copy_type: 33,
                fleet_ids: vec![932_1131],
            },
        );
        catalog.fleet_is_last.insert(932_1131, true);

        assert_eq!(
            battle_position_fleet_id(932_113, 932_1131, 0, Some(&catalog)),
            932_1131
        );
    }

    #[test]
    fn mubar_battle_does_not_consume_normal_supply() {
        let account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("mubar-supply").unwrap(),
            "Captain",
        );
        let hero_id = account
            .dock
            .heroes
            .keys()
            .next()
            .expect("starter hero")
            .get();
        let mut catalog = BattleCatalog::default();
        catalog.copies.insert(
            932_113,
            BattleCopy {
                config_id: 932_113,
                copy_type: 33,
                fleet_ids: vec![932_1131],
            },
        );
        catalog
            .supply_cost_by_copy
            .insert(932_113, (999_999, 999_999));

        assert_eq!(
            battle_supply_cost_typed(&account, Some(&catalog), 932_113, &[hero_id], 1),
            Some(0)
        );
    }
}

pub(crate) fn battle_pass_payload_with_rewards(
    copy_id: i32,
    first_pass: bool,
    grade: i32,
    battle_time: i32,
    rewards: &[ShopReward],
) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut item, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 1, &item);
    }
    if copy_id > 0 {
        append_varint_field(&mut output, 12, copy_id as u64);
    }
    append_varint_field(&mut output, 4, grade.max(0) as u64);
    append_varint_field(&mut output, 6, if grade > 0 && grade < 9 { 7 } else { 0 });
    if first_pass {
        append_varint_field(&mut output, 10, 1);
    }
    append_varint_field(&mut output, 8, battle_time.max(0).max(60) as u64);
    append_varint_field(&mut output, 3, 0);
    output
}

pub(crate) fn battle_pass_payload_with_damage(
    copy_id: i32,
    first_pass: bool,
    grade: i32,
    battle_time: i32,
    rewards: &[ShopReward],
    current_damage: i32,
    max_damage: i32,
) -> Vec<u8> {
    let mut output =
        battle_pass_payload_with_rewards(copy_id, first_pass, grade, battle_time, rewards);
    for (key, value) in [("CurDamage", current_damage), ("MaxDamage", max_damage)] {
        let mut extra = Vec::new();
        append_bytes_field(&mut extra, 1, key.as_bytes());
        append_varint_field(&mut extra, 2, value.max(0) as u64);
        append_message_field(&mut output, 5, &extra);
    }
    output
}

pub(crate) fn battle_pass_payload_with_experience(
    copy_id: i32,
    first_pass: bool,
    grade: i32,
    battle_time: i32,
    rewards: &[ShopReward],
    exp_rewards: &[(u64, i32)],
) -> Vec<u8> {
    let mut output =
        battle_pass_payload_with_rewards(copy_id, first_pass, grade, battle_time, rewards);
    append_battle_exp_rewards(&mut output, exp_rewards);
    output
}

pub(crate) fn battle_pass_payload_with_damage_and_experience(
    base: BattlePassDamagePayload,
    exp_rewards: &[(u64, i32)],
) -> Vec<u8> {
    let mut output = battle_pass_payload_with_damage(
        base.copy_id,
        base.first_pass,
        base.grade,
        base.battle_time,
        base.rewards,
        base.current_damage,
        base.max_damage,
    );
    append_battle_exp_rewards(&mut output, exp_rewards);
    output
}

pub(crate) struct BattlePassDamagePayload<'a> {
    pub(crate) copy_id: i32,
    pub(crate) first_pass: bool,
    pub(crate) grade: i32,
    pub(crate) battle_time: i32,
    pub(crate) rewards: &'a [ShopReward],
    pub(crate) current_damage: i32,
    pub(crate) max_damage: i32,
}

fn append_battle_exp_rewards(output: &mut Vec<u8>, exp_rewards: &[(u64, i32)]) {
    for (hero_id, value) in exp_rewards {
        if *hero_id == 0 {
            continue;
        }
        let mut reward = Vec::new();
        append_varint_field(&mut reward, 1, *hero_id);
        append_varint_field(&mut reward, 2, (*value).max(0) as u64);
        append_message_field(output, 11, &reward);
    }
}
