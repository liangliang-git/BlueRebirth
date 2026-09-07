#![allow(dead_code)]

use serde_json::Value;

use super::*;

pub(super) fn encode_random_factor_payload(
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

#[cfg(test)]
#[cfg(test)]
pub(super) fn battle_start_payload(
    account: &Value,
    copy_id: i32,
    requested_hero_ids: &[i32],
    battle_catalog: Option<&BattleCatalog>,
) -> Vec<u8> {
    battle_start_payload_with_fleet_groups_with_stats(
        account,
        copy_id,
        &[requested_hero_ids.to_vec()],
        battle_catalog,
        None,
        1.0,
        BattleStartOptions::default(),
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BattleStartOptions {
    pub(super) is_running_fight: bool,
    pub(super) battle_mode: i32,
    pub(super) anim_mode: i32,
    pub(super) match_type: i32,
}

#[cfg(test)]
pub(super) fn battle_start_payload_with_fleet_groups_with_stats(
    account: &Value,
    copy_id: i32,
    requested_hero_groups: &[Vec<i32>],
    battle_catalog: Option<&BattleCatalog>,
    ship_stat_catalog: Option<&ShipStatCatalog>,
    ship_stat_multiplier: f64,
    options: BattleStartOptions,
) -> Vec<u8> {
    let character = account.get("character").unwrap_or(account);
    let uid = json_i64(character, "uid").unwrap_or(1).max(1) as u64;
    let level = json_i64(character, "level").unwrap_or(80).max(1) as u64;
    let name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Captain")
        .as_bytes();
    let all_heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut players = Vec::new();
    let mut extra_player_lists = Vec::new();
    let groups = if requested_hero_groups.is_empty() {
        vec![Vec::new()]
    } else {
        requested_hero_groups.to_vec()
    };
    for (fleet_index, requested_hero_ids) in groups.iter().enumerate() {
        let heroes = select_battle_heroes(&all_heroes, requested_hero_ids);
        let fleet = encode_battle_fleet(
            &heroes,
            fleet_index,
            ship_stat_catalog,
            ship_stat_multiplier,
        );
        let player = encode_battle_player(&fleet, fleet_index, uid, level, name);
        if fleet_index == 0 {
            append_message_field(&mut players, 1, &player);
        } else {
            let mut player_list = Vec::new();
            append_message_field(&mut player_list, 1, &player);
            extra_player_lists.push(player_list);
        }
    }

    let mut output = Vec::new();
    append_message_field(&mut output, 1, &players);
    for player_list in extra_player_lists {
        append_message_field(&mut output, 15, &player_list);
    }
    append_varint_field(&mut output, 2, u64::from(current_unix_seconds()));
    let copy_info = battle_catalog.and_then(|catalog| catalog.copies.get(&copy_id));
    let is_search_3d = battle_catalog.is_some_and(|catalog| catalog.search_3d.contains(&copy_id));
    let rid = copy_info
        .map(|copy| copy.config_id)
        .unwrap_or(copy_id.max(1));
    let session_fleet_ids = battle_session_fleet_ids(copy_id, battle_catalog);
    append_varint_field(&mut output, 3, rid.max(1) as u64);
    for fleet_id in &session_fleet_ids {
        append_varint_field(&mut output, 5, (*fleet_id).max(1) as u64);
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
        u64::from(battle_copy_passed(account, copy_id)),
    );
    append_varint_field(&mut output, 10, u64::from(options.is_running_fight));
    append_bytes_field(&mut output, 16, b"1111111111111111111111111111111111111");
    append_varint_field(&mut output, 18, options.battle_mode.max(0) as u64);
    // The client builds MissionNode from CopyMission during PVEStartData setup.
    // Sending no entries leaves that node unresolved for weekly copies and the
    // battle loading state never advances. These IDs form the valid base mission
    // chain present in the bundled client catalog.
    for mission_id in [101, 102, 103] {
        append_varint_field(&mut output, 23, mission_id);
    }
    // SafeLv (13). The client reads this field while creating the sea-search
    // core; omit it and some builds never leave the ready state.
    append_varint_field(&mut output, 13, 0);
    if is_search_3d {
        // Search/Challenge Sea initialization reads these fields before creating
        // fleet movement and search state. Keep protobuf shape aligned with the
        // original client server response.
        append_varint_field(&mut output, 19, 0);
        append_varint_field(&mut output, 20, options.anim_mode.max(0) as u64);
        append_varint_field(&mut output, 22, 0);
        // 52002 is the client-side search timeout. Sending it here overwrites
        // the sea battle limit with zero and triggers an immediate auto_end.
        for (kind, value) in [(50000_i32, 1_i32), (0_i32, 1_i32)] {
            let mut config = Vec::new();
            if kind != 0 {
                append_varint_field(&mut config, 1, kind as u64);
            }
            if value != 0 {
                append_varint_field(&mut config, 2, value as u64);
            }
            append_message_field(&mut output, 25, &config);
        }
    }
    if options.match_type > 0 {
        append_varint_field(&mut output, 26, options.match_type as u64);
    }
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
            append_message_field(&mut output, 12, &factor);
        }
    }

    for (fleet_index, fleet_id) in session_fleet_ids.iter().enumerate() {
        let enemy_ids = battle_catalog
            .and_then(|catalog| catalog.fleet_enemies.get(fleet_id))
            .cloned()
            .filter(|ids| !ids.is_empty())
            .unwrap_or_else(|| vec![1]);
        let mut enemy = Vec::new();
        // Legacy sea maps may need 1-A anchor compatibility. Activity
        // challenge copies keep their configured fleet ID in helper below;
        // changing it makes client load wrong heading/movement config.
        let position_fleet_id =
            battle_position_fleet_id(copy_id, *fleet_id, fleet_index, battle_catalog);
        append_varint_field(&mut enemy, 1, position_fleet_id.max(1) as u64);
        append_varint_field(&mut enemy, 2, 0);
        for enemy_id in enemy_ids {
            let stats = battle_catalog.and_then(|catalog| catalog.enemies.get(&enemy_id));
            let mut enemy_ship = Vec::new();
            append_varint_field(&mut enemy_ship, 1, enemy_id.max(1) as u64);
            let attrs = [
                (
                    1,
                    battle_enemy_hp_for_client(copy_id, stats.map_or(1000, |value| value.hp)),
                ),
                (8, stats.map_or(0, |value| value.attack)),
                (9, stats.map_or(0, |value| value.defense)),
                (10, stats.map_or(0, |value| value.torpedo)),
                (11, stats.map_or(0, |value| value.torpedo_defense)),
                (17, stats.map_or(0, |value| value.crit)),
                (18, stats.map_or(0, |value| value.anti_crit)),
                (19, stats.map_or(100, |value| value.hit)),
                (20, stats.map_or(0, |value| value.dodge)),
            ];
            for (attr_id, value) in attrs {
                let mut attr = Vec::new();
                append_varint_field(&mut attr, 1, attr_id);
                append_varint_field(&mut attr, 2, value.max(0) as u64);
                append_message_field(&mut enemy_ship, 2, &attr);
            }
            append_message_field(&mut enemy, 3, &enemy_ship);
        }
        append_message_field(&mut output, 24, &enemy);
    }

    // Skip battle VCRs. Sea-search transition waits on these ids; without the
    // matching config_ship_info entries the client remains in its local search
    // state and movement input is ignored.
    let mut sent_vcr = std::collections::HashSet::new();
    let mut emit_vcr = |ship_info_id: i32| {
        if ship_info_id <= 0 || !sent_vcr.insert(ship_info_id) {
            return;
        }
        let mut vcr = Vec::new();
        append_varint_field(&mut vcr, 1, ship_info_id as u64);
        if is_search_3d {
            append_varint_field(&mut vcr, 2, 1);
            append_varint_field(&mut vcr, 3, 1);
        }
        append_message_field(&mut output, 17, &vcr);
    };
    for requested_hero_ids in &groups {
        for hero in select_battle_heroes(&all_heroes, requested_hero_ids) {
            let template_id = json_i64(hero, "templateId").unwrap_or(10210511).max(1);
            emit_vcr(((template_id - 1) / 10) as i32);
        }
    }
    for fleet_id in &session_fleet_ids {
        for enemy_id in battle_catalog
            .and_then(|catalog| catalog.fleet_enemies.get(fleet_id))
            .into_iter()
            .flatten()
        {
            if let Some(ship_info_id) = battle_catalog
                .and_then(|catalog| catalog.enemies.get(enemy_id))
                .map(|enemy| enemy.ship_info_id)
            {
                emit_vcr(ship_info_id);
            }
        }
    }
    output
}

pub(super) fn battle_start_payload_from_typed_account(
    account: &blueoath_domain::AccountState,
    copy_id: i32,
    requested_hero_groups: &[Vec<i32>],
    battle_catalog: Option<&BattleCatalog>,
    ship_stat_catalog: Option<&ShipStatCatalog>,
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
            for (attr_id, value) in ship_attributes_for_hero(
                None,
                i32::try_from(template_id).unwrap_or_default(),
                i64::from(hero.level.max(1)),
                ship_stat_catalog,
                ship_stat_multiplier,
            ) {
                let mut attr = Vec::new();
                append_varint_field(&mut attr, 1, attr_id as u64);
                append_varint_field(&mut attr, 2, value.max(0) as u64);
                append_message_field(&mut ship, 5, &attr);
            }
            append_varint_field(&mut ship, 6, hero.hp.max(1));
            append_varint_field(&mut ship, 11, 3);
            let mut skill = Vec::new();
            append_varint_field(&mut skill, 1, 41210);
            append_varint_field(&mut skill, 2, 1);
            append_message_field(&mut ship, 8, &skill);
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
    output
}

fn select_battle_heroes<'a>(all_heroes: &'a [Value], requested_ids: &[i32]) -> Vec<&'a Value> {
    if requested_ids.is_empty() {
        return all_heroes.iter().take(6).collect();
    }
    let mut selected: Vec<&'a Value> = Vec::new();
    for requested_id in requested_ids {
        if selected.len() >= 6 {
            break;
        }
        if let Some(hero) = all_heroes.iter().find(|hero| {
            json_i64(hero, "heroId").is_some_and(|hero_id| hero_id == i64::from(*requested_id))
        }) {
            if !selected
                .iter()
                .any(|existing| json_i64(existing, "heroId") == json_i64(hero, "heroId"))
            {
                selected.push(hero);
            }
        }
    }
    if selected.is_empty() {
        all_heroes.iter().take(6).collect()
    } else {
        selected
    }
}

fn encode_battle_fleet(
    heroes: &[&Value],
    fleet_index: usize,
    ship_stat_catalog: Option<&ShipStatCatalog>,
    ship_stat_multiplier: f64,
) -> Vec<u8> {
    let mut fleet = Vec::new();
    append_varint_field(&mut fleet, 1, (fleet_index as u64).saturating_add(1));
    append_varint_field(&mut fleet, 2, 2);
    append_varint_field(&mut fleet, 3, fleet_index as u64);
    for (index, hero) in heroes.iter().enumerate() {
        let hero_id = json_i64(hero, "heroId").unwrap_or(index as i64 + 1).max(1) as u64;
        let template_id = json_i64(hero, "templateId").unwrap_or(10210511).max(1) as u64;
        let hero_level = json_i64(hero, "level").unwrap_or(80).max(1) as u64;
        let mut ship = Vec::new();
        append_varint_field(&mut ship, 1, hero_id);
        append_varint_field(&mut ship, 2, template_id);
        append_varint_field(&mut ship, 3, hero_level);
        append_varint_field(&mut ship, 4, index as u64);
        let attrs = ship_attributes_for_hero(
            Some(hero),
            i32::try_from(template_id).unwrap_or_default(),
            hero_level as i64,
            ship_stat_catalog,
            ship_stat_multiplier,
        );
        for (attr_id, value) in attrs {
            let mut attr = Vec::new();
            append_varint_field(&mut attr, 1, attr_id as u64);
            append_varint_field(&mut attr, 2, value.max(0) as u64);
            append_message_field(&mut ship, 5, &attr);
        }
        append_varint_field(&mut ship, 6, 10_000_000_000);
        append_varint_field(&mut ship, 11, 3);
        append_varint_field(
            &mut ship,
            12,
            json_i64(hero, "fashioning").unwrap_or(0).max(0) as u64,
        );
        let mut skill = Vec::new();
        append_varint_field(&mut skill, 1, 41210);
        append_varint_field(&mut skill, 2, 1);
        append_message_field(&mut ship, 8, &skill);
        append_message_field(&mut fleet, 4, &ship);
        append_varint_field(&mut fleet, 8, hero_id);
    }
    append_varint_field(&mut fleet, 5, 0);
    append_varint_field(&mut fleet, 7, 0);
    append_varint_field(&mut fleet, 9, 1);
    fleet
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

#[cfg(test)]
pub(super) fn battle_enemy_ids(copy_id: i32, battle_catalog: Option<&BattleCatalog>) -> Vec<i32> {
    let Some(catalog) = battle_catalog else {
        return vec![1];
    };
    if !catalog.copies.contains_key(&copy_id) {
        return vec![1];
    }
    let mut ids = battle_session_fleet_ids(copy_id, battle_catalog)
        .iter()
        .flat_map(|fleet_id| catalog.fleet_enemies.get(fleet_id).into_iter().flatten())
        .copied()
        .filter(|id| *id > 0)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(1);
    }
    ids
}

pub(super) fn battle_fleet_ids(copy_id: i32, battle_catalog: Option<&BattleCatalog>) -> Vec<i32> {
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

pub(super) fn battle_session_fleet_ids(
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

pub(super) fn battle_position_fleet_id(
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
    if (202_431..=202_464).contains(&copy_id)
        && catalog
            .copies
            .get(&copy_id)
            .is_some_and(|copy| copy.fleet_ids.contains(&fleet_id))
    {
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

#[cfg(test)]
pub(super) fn battle_fleet_aliases(
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

#[cfg(test)]
pub(super) fn battle_enemy_hps(copy_id: i32, battle_catalog: Option<&BattleCatalog>) -> Vec<i64> {
    battle_enemy_ids(copy_id, battle_catalog)
        .into_iter()
        .map(|enemy_id| {
            battle_catalog
                .and_then(|catalog| catalog.enemies.get(&enemy_id))
                .map(|enemy| i64::from(enemy.hp.max(1)))
                .unwrap_or(1000)
        })
        .collect()
}

#[cfg(test)]
pub(super) fn battle_enemy_hp_for_client(copy_id: i32, configured_hp: i32) -> i32 {
    let hp = configured_hp.max(1);
    if copy_id == 100 {
        hp.min(1_000)
    } else {
        hp
    }
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn battle_attack_payload_with_damage(args: &[u8], damage: u64) -> Vec<u8> {
    let mut output = Vec::new();
    for (field, wanted) in [(1, 1), (2, 2), (3, 3), (4, 4)] {
        if field == 3 {
            for value in decode_repeated_varint_field(args, wanted) {
                append_varint_field(&mut output, field, value.max(0) as u64);
            }
        } else if let Some(value) =
            (decode_varint_field(args, wanted) > 0).then(|| decode_varint_field(args, wanted))
        {
            append_varint_field(&mut output, field, value.max(0) as u64);
        }
    }
    append_varint_field(&mut output, 5, damage);
    output
}

pub(super) fn battle_attack_payload_from_request(
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

#[cfg(test)]
fn battle_passed_fleet_ids(payload: &[u8]) -> std::collections::HashSet<i64> {
    let fleets = decode_repeated_message_field(payload, 20);
    let fleets = if fleets.is_empty() {
        decode_repeated_message_field(payload, 17)
    } else {
        fleets
    };
    fleets
        .into_iter()
        .map(|fleet| i64::from(decode_varint_field(&fleet, 1)))
        .filter(|fleet_id| *fleet_id > 0)
        .collect()
}

#[cfg(test)]
fn normalized_battle_passed_fleet_ids(
    session: &Value,
    payload: &[u8],
) -> std::collections::HashSet<i64> {
    let remaining_key = if session.get("remainingFleetIds").is_some() {
        "remainingFleetIds"
    } else {
        "remainingEnemyIds"
    };
    let remaining = session
        .get(remaining_key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect::<std::collections::HashSet<_>>();
    battle_passed_fleet_ids(payload)
        .into_iter()
        .filter_map(|wire_id| {
            if remaining.contains(&wire_id) {
                return Some(wire_id);
            }
            session
                .get("fleetAliases")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|alias| {
                    json_i64(alias, "wireId") == Some(wire_id)
                        && json_i64(alias, "realId")
                            .is_some_and(|real_id| remaining.contains(&real_id))
                })
                .and_then(|alias| json_i64(alias, "realId"))
        })
        .collect()
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn validate_battle_fleet_pass(session: &Value, payload: &[u8]) -> bool {
    let passed_fleet_ids = normalized_battle_passed_fleet_ids(session, payload);
    !passed_fleet_ids.is_empty() && passed_fleet_ids.len() == battle_passed_fleet_ids(payload).len()
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn mark_battle_fleet_passed(session: &mut Value, payload: &[u8]) -> bool {
    let passed_fleet_ids = normalized_battle_passed_fleet_ids(session, payload);
    if passed_fleet_ids.is_empty()
        || passed_fleet_ids.len() != battle_passed_fleet_ids(payload).len()
    {
        return false;
    }
    let Some(session) = session.as_object_mut() else {
        return true;
    };
    let remaining_key = if session.contains_key("remainingFleetIds") {
        "remainingFleetIds"
    } else {
        "remainingEnemyIds"
    };
    let Some(remaining) = session.get_mut(remaining_key).and_then(Value::as_array_mut) else {
        return true;
    };
    if !passed_fleet_ids.is_empty() {
        remaining
            .retain(|fleet_id| !passed_fleet_ids.contains(&fleet_id.as_i64().unwrap_or_default()));
    }
    remaining.is_empty()
}

#[cfg(test)]
pub(super) fn mark_first_battle_fleet_passed(session: &mut Value) -> bool {
    let Some(session) = session.as_object_mut() else {
        return true;
    };
    let remaining_key = if session.contains_key("remainingFleetIds") {
        "remainingFleetIds"
    } else {
        "remainingEnemyIds"
    };
    let Some(remaining) = session.get_mut(remaining_key).and_then(Value::as_array_mut) else {
        return true;
    };
    if !remaining.is_empty() {
        remaining.remove(0);
    }
    remaining.is_empty()
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn save_battle_hero_hp(
    account: &mut Value,
    heroes: &[BattleHeroResult],
    allowed_hero_ids: &[u64],
) -> bool {
    let Some(account_heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for result in heroes {
        if result.hero_id == 0
            || (!allowed_hero_ids.is_empty() && !allowed_hero_ids.contains(&result.hero_id))
        {
            continue;
        }
        let Some(hero) = account_heroes
            .iter_mut()
            .find(|hero| json_u64(hero, "heroId") == Some(result.hero_id))
        else {
            continue;
        };
        let hp = result.hp.max(0);
        if hero.get("curHp").and_then(Value::as_i64) != Some(hp) {
            hero["curHp"] = json!(hp);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn validate_battle_attack(
    session: &serde_json::Map<String, Value>,
    args: &[u8],
) -> Option<i64> {
    let attack_type = decode_varint_field(args, 1);
    let copy_id = decode_varint_field(args, 2);
    let hero_ids = decode_repeated_varint_field(args, 3);
    let enemy_id = decode_varint_field(args, 4);
    let session_copy_id = session.get("copyId").and_then(Value::as_i64)?;
    let session_hero_ids = session
        .get("heroIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let remaining_enemy_ids = session
        .get("remainingEnemyIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let remaining_fleet_ids = session
        .get("remainingFleetIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    (attack_type > 0
        && copy_id > 0
        && i64::from(copy_id) == session_copy_id
        && !hero_ids.is_empty()
        && hero_ids
            .iter()
            .all(|hero_id| session_hero_ids.contains(&i64::from(*hero_id)))
        && hero_ids.len() <= 6
        && hero_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == hero_ids.len()
        && enemy_id > 0
        && (remaining_enemy_ids.contains(&i64::from(enemy_id))
            || remaining_fleet_ids.contains(&i64::from(enemy_id))))
    .then_some(i64::from(enemy_id))
}

pub(super) fn battle_copy_passed(account: &Value, copy_id: i32) -> bool {
    let has_record = |value: &Value| {
        value
            .get("records")
            .and_then(Value::as_array)
            .is_some_and(|records| {
                records.iter().any(|record| {
                    value_i64_any(record, &["copyId", "copy_id"]) == i64::from(copy_id)
                })
            })
    };
    has_record(account.get("copyProgress").unwrap_or(&Value::Null))
        || has_record(account.get("seaProgress").unwrap_or(&Value::Null))
        || account
            .get("dailyCopy")
            .and_then(|daily| daily.get("chapters"))
            .and_then(Value::as_array)
            .is_some_and(|chapters| {
                chapters
                    .iter()
                    .any(|chapter| json_i32_array(chapter, "passCopy").contains(&copy_id))
            })
}

pub(super) fn completed_copy_ids(account: &Value, progress_key: &str) -> Vec<i32> {
    account
        .get(progress_key)
        .and_then(|progress| progress.get("records"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            let copy_id = value_i64_any(record, &["copyId", "copy_id"]);
            let passed = ["starLevel", "star_level", "passCount", "pass_count"]
                .iter()
                .any(|key| value_i64_any(record, &[*key]) > 0);
            (copy_id > 0 && passed)
                .then(|| i32::try_from(copy_id).ok())
                .flatten()
        })
        .collect()
}

pub(super) fn completed_copy_counts(account: &Value, progress_key: &str) -> Vec<(i32, i32)> {
    account
        .get(progress_key)
        .and_then(|progress| progress.get("records"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            let copy_id = value_i64_any(record, &["copyId", "copy_id"]);
            if copy_id <= 0 {
                return None;
            }
            let passed = ["starLevel", "star_level", "passCount", "pass_count"]
                .iter()
                .any(|key| value_i64_any(record, &[*key]) > 0);
            if !passed {
                return None;
            }
            let pass_count = value_i64_any(record, &["passCount", "pass_count"])
                .max(1)
                .min(i64::from(i32::MAX));
            Some((i32::try_from(copy_id).ok()?, pass_count as i32))
        })
        .collect()
}

pub(super) fn copy_progress_max_or_first(catalog_ids: &[i32], passed_ids: &[i32]) -> i32 {
    passed_ids
        .iter()
        .copied()
        .max()
        .or_else(|| catalog_ids.first().copied())
        .unwrap_or_default()
}

pub(super) fn copy_progress_max_or_initial(
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

#[cfg(test)]
pub(super) fn copy_request_type(payload: &[u8]) -> i32 {
    let requested = decode_varint_field(payload, 1);
    matches!(requested, 2 | 9 | 10 | 24 | 33 | 34)
        .then_some(requested)
        .unwrap_or(1)
}

pub(super) fn commander_level(account: &Value) -> i32 {
    json_i32(account.get("character").unwrap_or(account), "level")
        .unwrap_or_default()
        .max(1)
}

pub(super) fn sea_difficulty_for_account(account: &Value) -> i32 {
    if commander_level(account) < SEA_DIFFICULTY_UNLOCK_LEVEL {
        return 1;
    }
    account
        .get("character")
        .and_then(|character| json_i32(character, "seaDifficulty"))
        .or_else(|| json_i32(account, "seaDifficulty"))
        .unwrap_or(1)
        .clamp(1, 7)
}

pub(super) fn set_sea_difficulty(account: &mut Value, difficulty: i32) {
    if let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) {
        character.insert("seaDifficulty".to_owned(), json!(difficulty.clamp(1, 7)));
    }
}

pub(super) fn account_has_fleet(account: &Value, fleet_id: u64) -> bool {
    if fleet_id == 0 {
        return false;
    }
    account
        .get("fleet")
        .and_then(|fleet| fleet.get("tactics"))
        .and_then(Value::as_array)
        .is_some_and(|tactics| {
            tactics.iter().enumerate().any(|(index, tactic)| {
                json_u64(tactic, "fleetId") == Some(fleet_id)
                    || u64::try_from(index.saturating_add(1)).ok() == Some(fleet_id)
            })
        })
}

pub(super) fn consume_battle_supply(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    hero_ids: &[u64],
    count: i32,
) -> bool {
    if !(1..=99).contains(&count) || hero_ids.is_empty() {
        return false;
    }
    let Some(catalog) = catalog else {
        return false;
    };
    let (base, factor) = catalog
        .supply_cost_by_copy
        .get(&copy_id)
        .copied()
        .unwrap_or_default();
    let Some(heroes) = account.pointer("/dock/heroes").and_then(Value::as_array) else {
        return false;
    };
    let mut ship_cost = 0i64;
    let mut seen = std::collections::HashSet::new();
    for id in hero_ids {
        if !seen.insert(*id) {
            continue;
        }
        let Some(hero) = heroes
            .iter()
            .find(|hero| json_u64(hero, "heroId") == Some(*id))
        else {
            return false;
        };
        if factor > 0 {
            let cost = json_i32(hero, "templateId")
                .and_then(|tid| catalog.ship_supply_cost.get(&tid))
                .copied()
                .unwrap_or_default();
            ship_cost = ship_cost.saturating_add(cost);
        }
    }
    // Copy cost is fixed supply plus fleet supply multiplied by the 1/10000 coefficient.
    let cost = base
        .saturating_add(ship_cost.saturating_mul(factor).saturating_add(9999) / 10000)
        .saturating_mul(i64::from(count));
    let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) else {
        return false;
    };
    let supply = character
        .get("supply")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if supply < cost {
        return false;
    }
    character.insert("supply".to_owned(), json!(supply - cost));
    true
}

pub(super) fn consume_battle_supply_typed(
    account: &mut blueoath_domain::AccountState,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    hero_ids: &[u64],
    count: i32,
) -> bool {
    if !(1..=99).contains(&count) || hero_ids.is_empty() {
        return false;
    }
    let Some(catalog) = catalog else {
        return false;
    };
    let (base, factor) = catalog
        .supply_cost_by_copy
        .get(&copy_id)
        .copied()
        .unwrap_or_default();
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
            return false;
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
    let Ok(cost) = u64::try_from(cost.max(0)) else {
        return false;
    };
    account
        .resources
        .debit(blueoath_domain::CurrencyKind::Supply, cost)
        .is_ok()
}

pub(super) fn add_commander_battle_exp(
    account: &mut Value,
    gained: i32,
    catalog: Option<&CommanderLevelCatalog>,
) {
    if gained <= 0 {
        return;
    }
    let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) else {
        return;
    };
    let mut level = character
        .get("level")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(1)
        .max(1);
    let mut exp = character
        .get("exp")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let mut remaining = i64::from(gained);
    while remaining > 0 && level < 100 {
        let Some(need) = catalog
            .and_then(|catalog| catalog.exp_needed.get(&level))
            .copied()
            .filter(|need| *need > 0)
        else {
            break;
        };
        let total = exp.saturating_add(remaining);
        if total < i64::from(need) {
            exp = total;
            remaining = 0;
        } else {
            remaining = total.saturating_sub(i64::from(need));
            exp = 0;
            level = level.saturating_add(1);
        }
    }
    if level >= 100 {
        level = 100;
        exp = 0;
    } else if remaining > 0 && catalog.is_none_or(|catalog| catalog.exp_needed.is_empty()) {
        exp = exp.saturating_add(remaining);
    }
    character.insert("level".to_owned(), json!(level));
    character.insert("exp".to_owned(), json!(exp.min(i64::from(i32::MAX)) as i32));
}

pub(super) fn add_ship_battle_exp(
    account: &mut Value,
    hero_ids: &[u64],
    gained: i32,
    catalog: Option<&HeroLevelCatalog>,
) {
    if gained <= 0 {
        return;
    }
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for hero in heroes {
        if !hero_ids.is_empty()
            && !json_u64(hero, "heroId").is_some_and(|id| hero_ids.contains(&id))
        {
            continue;
        }
        let mood = i64::from(
            json_i32(hero, "mood")
                .unwrap_or(MOOD_INITIAL)
                .clamp(MOOD_MIN, MOOD_MAX),
        );
        let gained = scale_reward(
            i64::from(gained),
            if mood >= MOOD_AFFECTION_BONUS_THRESHOLD {
                1.2
            } else {
                1.0
            },
        )
        .clamp(0, i64::from(i32::MAX)) as i32;
        if gained <= 0 {
            continue;
        }
        let mut level = json_i32(hero, "level").unwrap_or(1).max(1);
        let mut exp = i64::from(json_i32(hero, "exp").unwrap_or_default().max(0));
        let mut remaining = i64::from(gained);
        while remaining > 0 && level < 200 {
            let Some(need) = catalog
                .and_then(|catalog| catalog.exp_needed.get(&level))
                .copied()
                .filter(|need| *need > 0)
            else {
                exp = exp.saturating_add(remaining);
                remaining = 0;
                break;
            };
            let total = exp.saturating_add(remaining);
            if total < i64::from(need) {
                exp = total;
                remaining = 0;
            } else {
                remaining = total.saturating_sub(i64::from(need));
                exp = 0;
                level = level.saturating_add(1);
            }
        }
        if level >= 200 {
            level = 200;
            exp = 0;
        }
        hero["level"] = json!(level);
        hero["exp"] = json!(exp.min(i64::from(i32::MAX)) as i32);
    }
}

pub(super) fn apply_battle_settlement(
    account: &mut Value,
    hero_ids: &[u64],
    mvp_hero_id: Option<u64>,
    shipwrecked_ids: &std::collections::HashSet<u64>,
    rule: BattleSettlementRule,
    affection_multiplier: f64,
) -> bool {
    if hero_ids.is_empty() {
        return false;
    }
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for hero in heroes {
        let Some(hero_id) = json_u64(hero, "heroId") else {
            continue;
        };
        let Some(position) = hero_ids.iter().position(|id| *id == hero_id) else {
            continue;
        };
        let mood = i64::from(
            json_i32(hero, "mood")
                .unwrap_or(MOOD_INITIAL)
                .clamp(MOOD_MIN, MOOD_MAX),
        );
        let affection = i64::from(json_i32(hero, "affection").unwrap_or_default());
        let mut affection_gain = if mood > MOOD_MIN as i64 {
            i64::from(rule.affection_add.max(0))
        } else {
            0
        };
        if mood > MOOD_MIN as i64 && position == 0 {
            affection_gain =
                affection_gain.saturating_add(i64::from(rule.affection_flagship_add.max(0)));
        }
        if mood > MOOD_MIN as i64 && mvp_hero_id == Some(hero_id) {
            affection_gain =
                affection_gain.saturating_add(i64::from(rule.affection_mvp_add.max(0)));
        }
        let affection_gain_multiplier = if mood >= MOOD_AFFECTION_BONUS_THRESHOLD {
            1.2
        } else {
            1.0
        };
        let affection_gain = scale_reward(
            affection_gain,
            affection_multiplier * affection_gain_multiplier,
        );
        let mut affection_delta = affection_gain;
        if shipwrecked_ids.contains(&hero_id) {
            affection_delta =
                affection_delta.saturating_sub(i64::from(rule.affection_reduce.max(0)));
        }
        let affection_max = if json_i32(hero, "marryTime").unwrap_or_default() > 0 {
            2_000_000_i64
        } else {
            1_000_000_i64
        };
        let next_affection = affection
            .saturating_add(affection_delta)
            .clamp(0, affection_max);
        if next_affection != affection {
            hero["affection"] = json!(next_affection as i32);
            changed = true;
        }

        let mood_reduce = if shipwrecked_ids.contains(&hero_id) {
            rule.mood_shipwrecks_reduce
        } else {
            rule.mood_reduce
        };
        let next_mood = mood
            .saturating_sub(i64::from(mood_reduce.max(0)))
            .clamp(i64::from(MOOD_MIN), i64::from(MOOD_MAX));
        if next_mood != mood {
            hero["mood"] = json!(next_mood as i32);
            changed = true;
        }
    }
    changed
}

pub(super) fn battle_copy_experience(catalog: Option<&BattleCatalog>, copy_id: i32) -> (i32, i32) {
    let Some(catalog) = catalog else {
        return (100, 0);
    };
    if !catalog.copies.contains_key(&copy_id) {
        return (100, 0);
    }
    let (mut commander_exp, mut ship_exp) = (0i32, 0i32);
    for fleet_id in battle_session_fleet_ids(copy_id, Some(catalog)) {
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

pub(super) fn battle_evaluation_multipliers(
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

pub(super) fn battle_other_drop_multiplier(catalog: Option<&BattleCatalog>, grade: i32) -> f64 {
    catalog
        .and_then(|catalog| catalog.evaluation_by_grade.get(&grade))
        .map(|rule| f64::from(rule.other_drop_ratio) / 10_000.0)
        .unwrap_or(1.0)
}

pub(super) fn battle_rank_drop_reward(
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    grade: i32,
) -> Option<ShopReward> {
    let rank_drop_id = catalog?.copy_rank_drop_ids.get(&copy_id)?;
    let (_, goods_type, item_id) = catalog?
        .rank_drop_rewards
        .get(rank_drop_id)?
        .iter()
        .find(|(reward_grade, _, _)| *reward_grade == grade)?;
    Some(ShopReward {
        goods_type: *goods_type,
        item_id: *item_id,
        num: 1,
        instance_id: 0,
    })
}

pub(super) fn battle_task_progress_enabled(catalog: Option<&BattleCatalog>, copy_id: i32) -> bool {
    !catalog.is_some_and(|catalog| catalog.task_disabled_copies.contains(&copy_id))
}

pub(super) fn draw_battle_drop_rewards(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    multiplier: f64,
    now: u32,
    fashion_catalog: Option<&FashionList>,
) -> Vec<ShopReward> {
    draw_battle_drop_rewards_for_grade(
        account,
        catalog,
        copy_id,
        multiplier,
        0,
        now,
        fashion_catalog,
    )
}

pub(super) fn draw_battle_drop_rewards_for_grade(
    account: &mut Value,
    catalog: Option<&BattleCatalog>,
    copy_id: i32,
    multiplier: f64,
    grade: i32,
    now: u32,
    fashion_catalog: Option<&FashionList>,
) -> Vec<ShopReward> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let mut rewards = Vec::new();
    let mut draw_index = 0u64;
    let seed = next_battle_drop_seed();
    let settle_multiplier = multiplier * battle_evaluation_multipliers(Some(catalog), grade).1;
    let other_multiplier = multiplier * battle_other_drop_multiplier(Some(catalog), grade);
    for drop_id in catalog.copy_drop_ids.get(&copy_id).into_iter().flatten() {
        let draws = draw_draw_count(
            settle_multiplier,
            mix_build_draw_roll(seed.wrapping_add(draw_index)),
        );
        draw_index = draw_index.wrapping_add(1);
        for _ in 0..draws {
            let roll_seed = seed.wrapping_add(draw_index);
            draw_index = draw_index.wrapping_add(1);
            if let Some(mut reward) = draw_copy_drop_with_seed(catalog, *drop_id, 0, roll_seed) {
                catalog.drop_quantities.apply(
                    copy_id,
                    &mut reward,
                    roll_seed.wrapping_add(0xD1B5_4A32_D192_ED03),
                );
                rewards.push(grant_reward(account, reward, now, fashion_catalog));
            }
        }
    }
    if catalog.copies.contains_key(&copy_id) {
        for fleet_id in battle_session_fleet_ids(copy_id, Some(catalog)) {
            for drop_id in catalog.fleet_drop_ids.get(&fleet_id).into_iter().flatten() {
                if let Some(mut reward) =
                    draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
                {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    rewards.push(grant_reward(account, reward, now, fashion_catalog));
                }
            }
            for drop_id in catalog
                .fleet_other_drop_ids
                .get(&fleet_id)
                .into_iter()
                .flatten()
            {
                if let Some(mut reward) =
                    draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
                {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    if draw_draw_count(
                        other_multiplier,
                        mix_build_draw_roll(seed.wrapping_add(draw_index)),
                    ) > 0
                    {
                        rewards.push(grant_reward(account, reward, now, fashion_catalog));
                    }
                    draw_index = draw_index.wrapping_add(1);
                }
            }
            for drop_id in catalog
                .fleet_settle_drop_ids
                .get(&fleet_id)
                .into_iter()
                .flatten()
            {
                if let Some(mut reward) =
                    draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
                {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    if draw_draw_count(
                        settle_multiplier,
                        mix_build_draw_roll(seed.wrapping_add(draw_index)),
                    ) > 0
                    {
                        rewards.push(grant_reward(account, reward, now, fashion_catalog));
                    }
                    draw_index = draw_index.wrapping_add(1);
                }
            }
        }
    }
    if let Some((count, guaranteed_rewards)) = catalog.copy_must_drop_rewards.get(&copy_id) {
        for _ in 0..*count {
            for (goods_type, item_id, num) in guaranteed_rewards {
                rewards.push(grant_reward(
                    account,
                    ShopReward {
                        goods_type: *goods_type,
                        item_id: *item_id,
                        num: *num,
                        instance_id: 0,
                    },
                    now,
                    fashion_catalog,
                ));
            }
        }
    }
    rewards
}

pub(super) fn record_battle_pass(
    account: &mut Value,
    copy_id: i32,
    grade: i32,
    battle_time: i32,
    battle_catalog: Option<&BattleCatalog>,
    ex_buffs: &[i32],
) {
    if copy_id <= 0 || grade >= 9 {
        return;
    }
    let copy_type = battle_catalog
        .and_then(|catalog| catalog.copies.get(&copy_id))
        .map(|copy| copy.copy_type)
        .unwrap_or(1);
    if copy_type == 9 {
        let daily_group_id = battle_catalog
            .and_then(|catalog| catalog.daily_group_by_copy.get(&copy_id))
            .copied()
            .unwrap_or_default();
        let Some(account) = account.as_object_mut() else {
            return;
        };
        let daily = account.entry("dailyCopy").or_insert_with(|| json!({}));
        let Some(daily) = daily.as_object_mut() else {
            return;
        };
        let chapters = daily.entry("chapters").or_insert_with(|| json!([]));
        let Some(chapters) = chapters.as_array_mut() else {
            return;
        };
        if let Some(chapter) = chapters.first_mut() {
            let Some(chapter) = chapter.as_object_mut() else {
                return;
            };
            let challenge_times = chapter
                .get("challengeTimes")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0)
                .saturating_add(1);
            chapter.insert("challengeTimes".to_owned(), json!(challenge_times));
            chapter.insert("exBuff".to_owned(), json!(ex_buffs));
            let pass_copy = chapter.entry("passCopy").or_insert_with(|| json!([]));
            let Some(pass_copy) = pass_copy.as_array_mut() else {
                return;
            };
            if !pass_copy
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(copy_id)))
            {
                pass_copy.push(json!(copy_id));
            }
        } else {
            chapters.push(json!({
                "chapterId": 0,
                "challengeTimes": 1,
                "passCopy": [copy_id],
                "selectEx": false,
                "exStar": grade.clamp(0, 6),
                "exBuff": ex_buffs
            }));
        }
        let records = daily
            .entry("records".to_owned())
            .or_insert_with(|| json!([]));
        if let Some(records) = records.as_array_mut() {
            records.push(json!({
                "copyId": copy_id,
                "passTime": battle_time.max(0),
                "recTime": current_unix_seconds(),
                "exBuff": ex_buffs,
                "exStar": grade.clamp(0, 6)
            }));
            if records.len() > 50 {
                let trim = records.len() - 50;
                records.drain(0..trim);
            }
        }
        if daily_group_id > 0 {
            increment_daily_group_success(daily, daily_group_id);
        }
        return;
    }
    record_copy_history(account, copy_id, battle_time);
    let key = if copy_type == 2 {
        "seaProgress"
    } else {
        "copyProgress"
    };
    let Some(account) = account.as_object_mut() else {
        return;
    };
    let progress = account.entry(key).or_insert_with(|| json!({}));
    let Some(progress) = progress.as_object_mut() else {
        return;
    };
    let records = progress.entry("records").or_insert_with(|| json!([]));
    let Some(records) = records.as_array_mut() else {
        return;
    };
    if let Some(record) = records
        .iter_mut()
        .find(|record| value_i64_any(record, &["copyId", "copy_id"]) == i64::from(copy_id))
    {
        let pass_count = value_i64_any(record, &["passCount", "pass_count"]) + 1;
        let existing_grade = value_i64_any(record, &["grade"]) as i32;
        let existing_star = value_i64_any(record, &["starLevel", "star_level"]) as i32;
        let star_level = if grade > 0 { 7 } else { 0 };
        if let Some(object) = record.as_object_mut() {
            object.insert("grade".to_owned(), json!(existing_grade.max(grade)));
            object.insert("starLevel".to_owned(), json!(existing_star.max(star_level)));
            object.insert("passTime".to_owned(), json!(battle_time.max(0)));
            object.insert("passCount".to_owned(), json!(pass_count));
        }
    } else {
        records.push(json!({
            "copyId": copy_id,
            "starLevel": if grade > 0 { 7 } else { 0 },
            "grade": grade,
            "passTime": battle_time.max(0),
            "passCount": 1
        }));
    }
}

fn record_copy_history(account: &mut Value, copy_id: i32, battle_time: i32) {
    let tactic = account
        .get("fleet")
        .and_then(|fleet| fleet.get("tactics"))
        .and_then(Value::as_array)
        .and_then(|tactics| tactics.first());
    let hero_ids = tactic
        .map(|value| json_i32_array(value, "heroInfo"))
        .unwrap_or_default();
    let hero_ids = if hero_ids.is_empty() {
        tactic
            .map(|value| json_i32_array(value, "heroIds"))
            .unwrap_or_default()
    } else {
        hero_ids
    };
    let template_ids = hero_ids
        .iter()
        .filter_map(|hero_id| {
            account
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_i32(hero, "heroId") == Some(*hero_id))
                .and_then(|hero| json_i32(hero, "templateId"))
        })
        .collect::<Vec<_>>();
    let record = json!({
        "copyId": copy_id,
        "passTime": battle_time.max(0),
        "recTime": current_unix_seconds(),
        "strategyId": tactic.and_then(|value| json_i32(value, "strategyId")).unwrap_or_default(),
        "heroIds": hero_ids,
        "templateIds": template_ids,
    });
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let records = root
        .entry("copyRecords".to_owned())
        .or_insert_with(|| json!([]));
    let Some(records) = records.as_array_mut() else {
        return;
    };
    records.push(record);
    if records.len() > 50 {
        let trim = records.len() - 50;
        records.drain(0..trim);
    }
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

pub(super) fn battle_pass_payload_with_rewards(
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

pub(super) fn battle_pass_payload_with_damage(
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

pub(super) fn battle_pass_payload_with_experience(
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

pub(super) fn battle_pass_payload_with_damage_and_experience(
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

pub(super) struct BattlePassDamagePayload<'a> {
    pub(super) copy_id: i32,
    pub(super) first_pass: bool,
    pub(super) grade: i32,
    pub(super) battle_time: i32,
    pub(super) rewards: &'a [ShopReward],
    pub(super) current_damage: i32,
    pub(super) max_damage: i32,
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
