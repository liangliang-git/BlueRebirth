use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

#[derive(Clone, Copy)]
pub(crate) struct BuildingTypedCatalogs<'a> {
    pub(crate) building: Option<&'a BuildingCatalog>,
    pub(crate) oil_multiplier: f64,
    pub(crate) gold_multiplier: f64,
}

#[allow(dead_code)]
pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    effects: &mut ResponseEffects,
    building_catalog: Option<&BuildingCatalog>,
) -> HandlerResult {
    handle_typed_with_multipliers(
        None,
        account,
        method,
        request_args,
        now,
        effects,
        BuildingTypedCatalogs {
            building: building_catalog,
            oil_multiplier: 1.0,
            gold_multiplier: 1.0,
        },
    )
}

pub(crate) fn handle_typed_with_multipliers(
    server_state: Option<&ServerState>,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    effects: &mut ResponseEffects,
    catalogs: BuildingTypedCatalogs<'_>,
) -> HandlerResult {
    let building_catalog = catalogs.building;
    let oil_multiplier = catalogs.oil_multiplier;
    let gold_multiplier = catalogs.gold_multiplier;
    ensure_typed_worker_state(account, building_catalog, now);
    apply_typed_dorm_mood(account, building_catalog, now);
    ensure_typed_building_productions(account, building_catalog, now);
    match method {
        "building.UpdateBuildingInfo" => {
            ensure_typed_building_productions(account, building_catalog, now);
            HandlerResult::Reply(Response::raw(
                method,
                UserBuildingInfoCodec::encode(&building_info_from_typed_account_with_catalog(
                    account,
                    now,
                    building_catalog,
                )),
            ))
        }
        "building.AddBuilding" => {
            let Ok(request) = BuildingAddRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building add request is invalid",
                ));
            };
            let Some(building_id) =
                add_typed_building(account, request.template_id, request.land_index)
            else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building placement is invalid",
                ));
            };
            append_typed_building_refresh(effects, account, now, building_catalog);
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, building_id);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "building.UpgradeBuilding" | "building.DegradeBuilding" => {
            let Ok(request) = BuildingIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building request is invalid",
                ));
            };
            let building_id = request.building_id;
            let delta = if method == "building.UpgradeBuilding" {
                1
            } else {
                -1
            };
            if !change_typed_building_level(account, building_id, delta, building_catalog) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building level change is invalid",
                ));
            }
            append_typed_building_refresh(effects, account, now, building_catalog);
            HandlerResult::PushOnly
        }
        "building.FinishBuilding" | "building.UseStrengthSpeedup" => {
            let Ok(request) = BuildingIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building request is invalid",
                ));
            };
            let building_id = request.building_id;
            let Some(building_id) = u64::try_from(building_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("building id is invalid"));
            };
            if !account.buildings.levels.contains_key(&building_id) {
                return HandlerResult::Error(GameError::InvalidRequest("building was not found"));
            }
            append_typed_building_refresh(effects, account, now, building_catalog);
            HandlerResult::PushOnly
        }
        "building.ProduceItem" | "building.ComposeItem" => {
            let Ok(request) = BuildingProduceRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building production request is invalid",
                ));
            };
            let Some(catalog) = building_catalog else {
                return HandlerResult::Empty;
            };
            if !set_typed_building_production(account, &request, catalog, now) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building production request is invalid",
                ));
            }
            append_typed_building_refresh(effects, account, now, building_catalog);
            HandlerResult::PushOnly
        }
        "building.ReceiveBuilding"
        | "building.ReceiveItem"
        | "building.ReceiveAll"
        | "building.ReceiveResource" => {
            let Some(catalog) = building_catalog else {
                return HandlerResult::Empty;
            };
            let (building_id, resource_id) = if method == "building.ReceiveAll" {
                (None, None)
            } else if method == "building.ReceiveResource" {
                let Ok(request) = BuildingResourceRequest::decode(request_args) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "building resource request is invalid",
                    ));
                };
                (None, Some(request.resource_id))
            } else {
                let Ok(request) = BuildingIdRequest::decode(request_args) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "building receive request is invalid",
                    ));
                };
                (Some(request.building_id), None)
            };
            let rewards = collect_typed_building_rewards(
                account,
                catalog,
                building_id,
                resource_id,
                now,
                oil_multiplier,
                gold_multiplier,
            );
            if rewards.is_empty() {
                return HandlerResult::Reply(Response::raw(method, encode_rewards_list(&[])));
            }
            for reward in &rewards {
                apply_typed_building_reward(account, reward);
            }
            append_typed_building_refresh(effects, account, now, building_catalog);
            if let Some(state) = server_state {
                effects.push_pre(Response::raw(
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
                ));
            }
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            HandlerResult::Reply(Response::raw(method, encode_rewards_list(&rewards)))
        }
        "build.BuildInfo" | "build.BuildsInfo" => HandlerResult::Reply(Response::raw(
            method,
            typed_construction_info_payload(account, now),
        )),
        "build.BuildingByFormula" => {
            let Ok(request) = BuildFormulaRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "construction request is invalid",
                ));
            };
            if !start_typed_construction(account, &request, now) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "construction request cannot start",
                ));
            }
            effects.push_pre(Response::raw(
                "build.BuildsInfo",
                typed_construction_info_payload(account, now),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "build.BuildQuicklyFinish" => {
            let Ok(request) = ConstructionIndexesRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "construction finish request is invalid",
                ));
            };
            if !finish_typed_construction(account, &request, now) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "construction quick-finish failed",
                ));
            }
            effects.push_pre(Response::raw(
                "build.BuildsInfo",
                typed_construction_info_payload(account, now),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "build.BuildReceive" => {
            let Ok(request) = ConstructionReceiveRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "construction receive request is invalid",
                ));
            };
            let Some(rewards) = receive_typed_construction(account, &request, now) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "no completed construction",
                ));
            };
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "build.BuildsInfo",
                typed_construction_info_payload(account, now),
            ));
            HandlerResult::Reply(Response::raw(method, encode_rewards_list(&rewards)))
        }
        "building.UpdateHeroAddition" => {
            append_typed_building_refresh(effects, account, now, building_catalog);
            HandlerResult::PushOnly
        }
        "building.SetHero" | "building.SetBuildingListHero" => {
            let assignments = if method == "building.SetHero" {
                let Ok(request) = BuildingSetHeroRequest::decode(request_args) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "building assignment request is invalid",
                    ));
                };
                vec![(request.building_id, request.hero_ids)]
            } else {
                let Ok(request) = BuildingSetHeroListRequest::decode(request_args) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "building assignment request is invalid",
                    ));
                };
                decode_building_assignments(&request)
            };
            if !set_typed_building_assignments(account, &assignments, building_catalog) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building assignment is invalid",
                ));
            }
            append_typed_building_refresh(effects, account, now, building_catalog);
            HandlerResult::PushOnly
        }
        "buildnotes.GetNotesList" | "buildnotes.GiveLike" => {
            HandlerResult::Reply(Response::raw(method, build_notes_payload(now)))
        }
        "discuss.GetDiscuss" => {
            let Ok(request) = DiscussRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "discuss request is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(method, discuss_payload(request.discuss_id)))
        }
        "discuss.HeroLike" | "discuss.Discuss" | "discuss.Like" | "discuss.Dislike" => {
            HandlerResult::Reply(Response::raw(method, encode_discuss_empty()))
        }
        _ => HandlerResult::Empty,
    }
}

fn add_typed_building(
    account: &mut blueoath_domain::AccountState,
    template_id: i32,
    land_index: i32,
) -> Option<u64> {
    if template_id <= 0 || land_index <= 0 {
        return None;
    }
    if account
        .buildings
        .land_indices
        .values()
        .any(|index| i32::try_from(*index).ok() == Some(land_index))
    {
        return None;
    }
    let building_id = account
        .buildings
        .levels
        .keys()
        .copied()
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    account.buildings.levels.insert(building_id, 1);
    account
        .buildings
        .template_ids
        .insert(building_id, u64::try_from(template_id).ok()?);
    account
        .buildings
        .land_indices
        .insert(building_id, u32::try_from(land_index).ok()?);
    Some(building_id)
}

fn typed_project_payload(project: &blueoath_domain::ConstructionProjectState) -> Vec<u8> {
    let mut output = Vec::new();
    for (resource_id, count) in [(10029, project.steel), (10030, project.aluminium)] {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, resource_id);
        append_varint_field(&mut item, 2, u64::from(count));
        append_message_field(&mut output, 1, &item);
    }
    append_varint_field(&mut output, 2, u64::from(project.gold));
    output
}

pub(crate) fn typed_construction_info_payload(
    account: &blueoath_domain::AccountState,
    now: u32,
) -> Vec<u8> {
    let mut groups = [Vec::new(), Vec::new(), Vec::new()];
    for job in &account.buildings.construction_jobs {
        let completed = job.completed || job.end_at > 0 && job.end_at <= u64::from(now);
        let mut formula = Vec::new();
        append_varint_field(&mut formula, 1, job.end_at);
        append_message_field(&mut formula, 2, &typed_project_payload(&job.project));
        if completed {
            append_varint_field(&mut formula, 3, job.template_id);
        }
        let group = if completed {
            &mut groups[0]
        } else if job.end_at > 0 {
            &mut groups[1]
        } else {
            &mut groups[2]
        };
        group.push((job.sequence, formula));
    }
    let mut output = Vec::new();
    for (index, field) in [(0usize, 1_u8), (1, 2), (2, 3)] {
        groups[index].sort_by_key(|(sequence, _)| *sequence);
        for (_, formula) in groups[index].drain(..) {
            append_message_field(&mut output, field, &formula);
        }
    }
    if let Some(project) = &account.buildings.last_project {
        let mut last = Vec::new();
        append_varint_field(&mut last, 1, 0);
        append_message_field(&mut last, 2, &typed_project_payload(project));
        append_message_field(&mut output, 4, &last);
    }
    output
}

fn typed_inventory_count(account: &blueoath_domain::AccountState, template_id: u64) -> u64 {
    blueoath_domain::TemplateId::new(template_id)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

fn consume_typed_inventory(
    account: &mut blueoath_domain::AccountState,
    template_id: u64,
    amount: u64,
) -> bool {
    let Some(template_id) = blueoath_domain::TemplateId::new(template_id).ok() else {
        return false;
    };
    let Some(value) = account.inventory.items.get_mut(&template_id) else {
        return false;
    };
    if *value < amount {
        return false;
    }
    *value -= amount;
    true
}

fn start_typed_construction(
    account: &mut blueoath_domain::AccountState,
    request: &BuildFormulaRequest,
    now: u32,
) -> bool {
    if account.buildings.construction_jobs.len() + request.projects.len() > 10 {
        return false;
    }
    let total_gold = request
        .projects
        .iter()
        .map(|project| u64::try_from(project.gold).unwrap_or_default())
        .sum::<u64>();
    let total_steel = request
        .projects
        .iter()
        .map(|project| u64::try_from(project.steel).unwrap_or_default())
        .sum::<u64>();
    let total_aluminium = request
        .projects
        .iter()
        .map(|project| u64::try_from(project.aluminium).unwrap_or_default())
        .sum::<u64>();
    if account
        .resources
        .amount(blueoath_domain::CurrencyKind::Gold)
        .get()
        < total_gold
        || typed_inventory_count(account, 10029) < total_steel
        || typed_inventory_count(account, 10030) < total_aluminium
    {
        return false;
    }
    let active = account
        .buildings
        .construction_jobs
        .iter()
        .filter(|job| !job.completed && job.end_at > u64::from(now))
        .count();
    let mut sequence = account
        .buildings
        .construction_jobs
        .iter()
        .map(|job| job.sequence)
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    let mut jobs = Vec::with_capacity(request.projects.len());
    for (offset, project) in request.projects.iter().enumerate() {
        let template_id = select_construction_template(
            i64::from(project.gold),
            i64::from(project.steel),
            i64::from(project.aluminium),
        );
        let duration_seconds = construction_duration_seconds(template_id);
        let end_at = if active + offset < 2 {
            u64::from(now).saturating_add(u64::try_from(duration_seconds).unwrap_or_default())
        } else {
            0
        };
        jobs.push(blueoath_domain::ConstructionJobState {
            sequence,
            template_id: u64::try_from(template_id).unwrap_or_default(),
            duration_seconds: u32::try_from(duration_seconds).unwrap_or_default(),
            end_at,
            completed: false,
            project: blueoath_domain::ConstructionProjectState {
                gold: u32::try_from(project.gold).unwrap_or_default(),
                steel: u32::try_from(project.steel).unwrap_or_default(),
                aluminium: u32::try_from(project.aluminium).unwrap_or_default(),
            },
        });
        sequence = sequence.saturating_add(1);
    }
    if account
        .resources
        .debit(blueoath_domain::CurrencyKind::Gold, total_gold)
        .is_err()
        || !consume_typed_inventory(account, 10029, total_steel)
        || !consume_typed_inventory(account, 10030, total_aluminium)
    {
        return false;
    }
    account.buildings.last_project = jobs.last().map(|job| job.project.clone());
    account.buildings.construction_jobs.extend(jobs);
    true
}

fn finish_typed_construction(
    account: &mut blueoath_domain::AccountState,
    request: &ConstructionIndexesRequest,
    now: u32,
) -> bool {
    let mut indexes = request.indexes.clone();
    indexes.sort_unstable();
    indexes.dedup();
    if typed_inventory_count(account, 10031) < indexes.len() as u64 {
        return false;
    }
    let active = account
        .buildings
        .construction_jobs
        .iter()
        .filter(|job| !job.completed && job.end_at > u64::from(now))
        .collect::<Vec<_>>();
    let selected = indexes
        .iter()
        .filter_map(|index| usize::try_from(index.saturating_sub(1)).ok())
        .filter_map(|index| active.get(index).map(|job| job.sequence))
        .collect::<Vec<_>>();
    if selected.len() != indexes.len() {
        return false;
    }
    if !consume_typed_inventory(account, 10031, indexes.len() as u64) {
        return false;
    }
    for job in &mut account.buildings.construction_jobs {
        if selected.contains(&job.sequence) {
            job.completed = true;
            job.end_at = u64::from(now);
        }
    }
    true
}

fn receive_typed_construction(
    account: &mut blueoath_domain::AccountState,
    request: &ConstructionReceiveRequest,
    now: u32,
) -> Option<Vec<ShopReward>> {
    let completed = account
        .buildings
        .construction_jobs
        .iter()
        .filter(|job| job.completed || job.end_at > 0 && job.end_at <= u64::from(now))
        .cloned()
        .collect::<Vec<_>>();
    if completed.is_empty() {
        return None;
    }
    let mut indexes = if request.indexes.is_empty() {
        vec![1]
    } else {
        request.indexes.clone()
    };
    indexes.sort_unstable();
    indexes.dedup();
    if account.dock.heroes.len().saturating_add(indexes.len())
        > usize::try_from(account.ship_dock_capacity()).unwrap_or(usize::MAX)
    {
        return None;
    }
    let selected = indexes
        .iter()
        .filter_map(|index| usize::try_from(index.saturating_sub(1)).ok())
        .filter_map(|index| completed.get(index))
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() != indexes.len() {
        return None;
    }
    let selected_sequences = selected
        .iter()
        .map(|job| job.sequence)
        .collect::<std::collections::BTreeSet<_>>();
    let mut next_hero_id = account
        .dock
        .heroes
        .keys()
        .map(|id| id.get())
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    let mut rewards = Vec::with_capacity(selected.len());
    for job in &selected {
        let hero_id = blueoath_domain::HeroId::new(next_hero_id).ok()?;
        let template_id = blueoath_domain::TemplateId::new(job.template_id).ok()?;
        let mut hero = blueoath_domain::HeroState {
            id: hero_id,
            template_id,
            fashioning: u32::try_from(template_id.get().saturating_sub(1) / 10).unwrap_or(u32::MAX),
            name: String::new(),
            change_name_time: 0,
            level: 1,
            exp: 0,
            mood: blueoath_domain::HERO_MOOD_INITIAL,
            affection: 500_000,
            hp: ship_initial_hp_for_template(template_id.get()),
            locked: false,
            created_utc: String::new(),
            equip_slots: vec![None; 6],
            pskills: std::collections::BTreeMap::new(),
        };
        initialize_typed_hero_loadout_from_catalog(account, &mut hero);
        account.dock.heroes.insert(hero_id, hero);
        rewards.push(ShopReward {
            goods_type: 3,
            item_id: i32::try_from(job.template_id).ok()?,
            num: 1,
            instance_id: i32::try_from(next_hero_id).ok()?,
        });
        next_hero_id = next_hero_id.saturating_add(1);
    }
    account
        .buildings
        .construction_jobs
        .retain(|job| !selected_sequences.contains(&job.sequence));
    Some(rewards)
}

fn change_typed_building_level(
    account: &mut blueoath_domain::AccountState,
    building_id: i32,
    delta: i32,
    catalog: Option<&BuildingCatalog>,
) -> bool {
    if building_id <= 0 || delta == 0 {
        return false;
    }
    let Some(building_id) = u64::try_from(building_id).ok() else {
        return false;
    };
    let Some(catalog) = catalog else {
        return false;
    };
    let Some(current_level) = account.buildings.levels.get(&building_id).copied() else {
        return false;
    };
    let current_template_id = account
        .buildings
        .template_ids
        .get(&building_id)
        .copied()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default();
    let Some(current_config) = catalog.typed_building_configs.get(&current_template_id) else {
        return false;
    };
    let next = i64::from(current_level).saturating_add(i64::from(delta));
    if next <= 0 {
        return false;
    }
    let Ok(next_level) = i32::try_from(next) else {
        return false;
    };
    let Some((next_template_id, next_config)) =
        catalog.typed_building_configs.iter().find(|(_, config)| {
            config.building_type == current_config.building_type && config.level == next_level
        })
    else {
        return false;
    };
    if delta > 0
        && current_config.building_type != 1
        && account
            .buildings
            .levels
            .iter()
            .find_map(|(id, level)| {
                let template_id = account.buildings.template_ids.get(id)?;
                let config = catalog
                    .typed_building_configs
                    .get(&i32::try_from(*template_id).ok()?)?;
                (config.building_type == 1).then_some(*level)
            })
            .is_some_and(|office_level| {
                office_level < u32::try_from(next_level).unwrap_or(u32::MAX)
            })
    {
        return false;
    }
    if delta < 0
        && account
            .buildings
            .hero_assignments
            .get(&building_id)
            .is_some_and(|heroes| heroes.len() > next_config.hero_capacity)
    {
        return false;
    }
    if delta > 0 && !can_consume_building_upgrade_cost(account, next_template_id, catalog) {
        return false;
    }
    if delta > 0 && !consume_building_upgrade_cost(account, next_template_id, catalog) {
        return false;
    }
    if current_config.building_type == 1 {
        let current_max = typed_worker_max_strength_for_office_level(catalog, current_level);
        let next_max = typed_worker_max_strength_for_office_level(
            catalog,
            u32::try_from(next_level).unwrap_or(u32::MAX),
        );
        if next_max >= current_max {
            account.buildings.worker_strength = account
                .buildings
                .worker_strength
                .saturating_add(next_max.saturating_sub(current_max));
        } else {
            account.buildings.worker_strength = account.buildings.worker_strength.min(next_max);
        }
        account.buildings.worker_update_at = account.buildings.worker_update_at.max(1);
    }
    account
        .buildings
        .levels
        .insert(building_id, u32::try_from(next_level).unwrap_or(u32::MAX));
    account.buildings.template_ids.insert(
        building_id,
        u64::try_from(*next_template_id).unwrap_or_default(),
    );
    if let Some(production) = account.buildings.productions.get_mut(&building_id) {
        production.productivity = u32::try_from(next_config.productivity).unwrap_or_default();
        production.produce_speed = u32::try_from(next_config.produce_speed).unwrap_or_default();
    }
    true
}

fn building_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        19 => blueoath_domain::CurrencyKind::Oil,
        20 => blueoath_domain::CurrencyKind::BuildMaterial,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn can_consume_building_upgrade_cost(
    account: &blueoath_domain::AccountState,
    template_id: &i32,
    catalog: &BuildingCatalog,
) -> bool {
    let Some(rule) = catalog.upgrade_rules_by_template.get(template_id) else {
        return true;
    };
    if rule.cost_work > 0
        && account.buildings.worker_strength
            < u32::try_from(rule.cost_work)
                .unwrap_or(u32::MAX)
                .saturating_mul(10_000)
    {
        return false;
    }
    if account
        .resources
        .amount(blueoath_domain::CurrencyKind::Gold)
        .get()
        < u64::try_from(rule.cost_money).unwrap_or(u64::MAX)
    {
        return false;
    }
    let mut required = std::collections::BTreeMap::<(i32, i32), u64>::new();
    for &(goods_type, item_id, amount) in &rule.costs {
        let Ok(amount) = u64::try_from(amount) else {
            return false;
        };
        let entry = required.entry((goods_type, item_id)).or_default();
        *entry = entry.saturating_add(amount);
    }
    required.into_iter().all(|((goods_type, item_id), amount)| {
        if goods_type == 5 {
            building_currency_kind(item_id)
                .is_some_and(|kind| account.resources.amount(kind).get() >= amount)
        } else if matches!(goods_type, 1 | 6) {
            blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
                .ok()
                .is_some_and(|id| {
                    account
                        .inventory
                        .items
                        .get(&id)
                        .copied()
                        .unwrap_or_default()
                        >= amount
                })
        } else {
            false
        }
    })
}

fn consume_building_upgrade_cost(
    account: &mut blueoath_domain::AccountState,
    template_id: &i32,
    catalog: &BuildingCatalog,
) -> bool {
    let Some(rule) = catalog.upgrade_rules_by_template.get(template_id) else {
        return true;
    };
    if rule.cost_money > 0
        && account
            .resources
            .debit(
                blueoath_domain::CurrencyKind::Gold,
                u64::try_from(rule.cost_money).unwrap_or(u64::MAX),
            )
            .is_err()
    {
        return false;
    }
    for &(goods_type, item_id, amount) in &rule.costs {
        let amount = u64::try_from(amount).unwrap_or_default();
        if goods_type == 5 {
            let Some(kind) = building_currency_kind(item_id) else {
                return false;
            };
            if account.resources.debit(kind, amount).is_err() {
                return false;
            }
        } else if matches!(goods_type, 1 | 6) {
            let Ok(template_id) =
                blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
            else {
                return false;
            };
            if let Some(count) = account.inventory.items.get_mut(&template_id) {
                *count = count.saturating_sub(amount);
                if *count == 0 {
                    account.inventory.items.remove(&template_id);
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    if rule.cost_work > 0 {
        account.buildings.worker_strength = account.buildings.worker_strength.saturating_sub(
            u32::try_from(rule.cost_work)
                .unwrap_or(u32::MAX)
                .saturating_mul(10_000),
        );
    }
    true
}

fn set_typed_building_production(
    account: &mut blueoath_domain::AccountState,
    request: &BuildingProduceRequest,
    catalog: &BuildingCatalog,
    now: u32,
) -> bool {
    let Ok(building_id) = u64::try_from(request.building_id) else {
        return false;
    };
    if !account.buildings.levels.contains_key(&building_id) {
        return false;
    }
    let Some(recipe) = catalog.typed_recipe_configs.get(&request.recipe_id) else {
        return false;
    };
    let template_id = account
        .buildings
        .template_ids
        .get(&building_id)
        .and_then(|value| i32::try_from(*value).ok())
        .unwrap_or_default();
    let Some(building_config) = catalog.typed_building_configs.get(&template_id) else {
        return false;
    };
    if building_config.building_type != 7 {
        return false;
    }
    let recipe_time = recipe.time_seconds.max(1);
    let productivity = building_config.productivity.max(0);
    let produce_speed = building_config.produce_speed.max(0);
    account.buildings.productions.insert(
        building_id,
        blueoath_domain::BuildingProductionState {
            status: 3,
            recipe_id: u32::try_from(request.recipe_id).unwrap_or_default(),
            item_count: u32::try_from(request.count).unwrap_or_default(),
            product_count: 0,
            last_update_at: u64::from(now),
            recipe_time: u32::try_from(recipe_time).unwrap_or_default(),
            productivity: u32::try_from(productivity).unwrap_or_default(),
            produce_speed: u32::try_from(produce_speed).unwrap_or_default(),
        },
    );
    true
}

fn collect_typed_building_rewards(
    account: &mut blueoath_domain::AccountState,
    catalog: &BuildingCatalog,
    building_id: Option<i32>,
    resource_id: Option<i32>,
    now: u32,
    oil_multiplier: f64,
    gold_multiplier: f64,
) -> Vec<ShopReward> {
    let ids = account
        .buildings
        .productions
        .keys()
        .copied()
        .collect::<Vec<_>>();
    let mut rewards = Vec::new();
    for id in ids {
        if building_id.is_some_and(|value| u64::try_from(value).ok() != Some(id)) {
            continue;
        }
        let template_id = account
            .buildings
            .template_ids
            .get(&id)
            .and_then(|value| i32::try_from(*value).ok())
            .unwrap_or_default();
        let Some(config) = catalog.typed_building_configs.get(&template_id) else {
            continue;
        };
        let building_type = config.building_type;
        let Some(production) = account.buildings.productions.get(&id).cloned() else {
            continue;
        };
        let (reward, completed) = if matches!(building_type, 2 | 3 | 4 | 6) {
            typed_resource_reward(
                &production,
                config,
                catalog,
                resource_id,
                now,
                oil_multiplier,
                gold_multiplier,
            )
            .map(|reward| (reward, 0))
            .unwrap_or((
                ShopReward {
                    goods_type: 0,
                    item_id: 0,
                    num: 0,
                    instance_id: 0,
                },
                0,
            ))
        } else if building_type == 7 {
            typed_item_reward(&production, config, catalog, now).unwrap_or((
                ShopReward {
                    goods_type: 0,
                    item_id: 0,
                    num: 0,
                    instance_id: 0,
                },
                0,
            ))
        } else {
            continue;
        };
        if reward.num <= 0 {
            continue;
        }
        if let Some(state) = account.buildings.productions.get_mut(&id) {
            if building_type == 7 {
                state.item_count = state.item_count.saturating_sub(completed);
                state.product_count = 0;
                if state.item_count == 0 {
                    state.recipe_id = 0;
                    state.status = 1;
                } else {
                    state.last_update_at = u64::from(now);
                }
            } else {
                let max = u32::try_from(config.product_max.max(0)).unwrap_or_default();
                let settled = state.product_count.min(max);
                state.product_count = 0;
                state.status = if settled >= max { 1 } else { 3 };
                state.last_update_at = u64::from(now);
            }
        }
        rewards.push(reward);
    }
    rewards
}

fn typed_resource_reward(
    production: &blueoath_domain::BuildingProductionState,
    config: &BuildingConfig,
    catalog: &BuildingCatalog,
    resource_id: Option<i32>,
    now: u32,
    oil_multiplier: f64,
    gold_multiplier: f64,
) -> Option<ShopReward> {
    let product_id = config.product_id?;
    if !matches!(product_id, 1 | 5 | 19 | 20) || resource_id.is_some_and(|id| id != product_id) {
        return None;
    }
    let max = config.product_max.max(0);
    let mut count = i64::from(production.product_count.min(u32::try_from(max).ok()?));
    if production.status == 3 && production.productivity > 0 && count < i64::from(max) {
        let delta = i64::from(now)
            .saturating_sub(i64::try_from(production.last_update_at).ok()?)
            .max(0);
        let parameter_id = if product_id == 19 { 209 } else { 210 };
        let period = i64::from(
            catalog
                .resource_time_seconds
                .get(&parameter_id)
                .copied()
                .unwrap_or(600)
                .max(1),
        );
        let produced = i128::from(delta)
            .saturating_mul(i128::from(production.productivity))
            .checked_div(i128::from(period) * 10_000)
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or_default();
        let multiplier = match product_id {
            19 => oil_multiplier,
            1 => gold_multiplier,
            // Supply and building material have no gold/oil production bonus.
            _ => 1.0,
        };
        count = count
            .saturating_add(scale_reward(produced, multiplier))
            .min(i64::from(max));
    }
    (count > 0).then_some(ShopReward {
        goods_type: 5,
        item_id: product_id,
        num: i32::try_from(count).unwrap_or(i32::MAX),
        instance_id: 0,
    })
}

fn typed_item_reward(
    production: &blueoath_domain::BuildingProductionState,
    config: &BuildingConfig,
    catalog: &BuildingCatalog,
    now: u32,
) -> Option<(ShopReward, u32)> {
    let recipe_id = i32::try_from(production.recipe_id).ok()?;
    let recipe = catalog.typed_recipe_configs.get(&recipe_id)?;
    let recipe_time = i64::from(recipe.time_seconds.max(1));
    let goods_type = recipe.goods_type;
    let item_id = recipe.item_id;
    let item_num = recipe.item_amount.max(1);
    let completed = if production.status == 3 {
        u32::try_from(
            i64::from(now)
                .saturating_sub(i64::try_from(production.last_update_at).ok()?)
                .max(0)
                / recipe_time,
        )
        .ok()?
        .min(production.item_count)
    } else {
        0
    };
    let max = u32::try_from(config.product_max.max(0)).unwrap_or(u32::MAX);
    let total = production.product_count.saturating_add(completed).min(max);
    (total > 0).then_some((
        ShopReward {
            goods_type,
            item_id,
            num: i32::try_from(total)
                .unwrap_or(i32::MAX)
                .saturating_mul(item_num),
            instance_id: 0,
        },
        completed,
    ))
}

fn apply_typed_building_reward(account: &mut blueoath_domain::AccountState, reward: &ShopReward) {
    if reward.num <= 0 {
        return;
    }
    if reward.goods_type == 5 {
        let kind = match reward.item_id {
            1 => Some(blueoath_domain::CurrencyKind::Gold),
            2 => Some(blueoath_domain::CurrencyKind::Diamond),
            5 => Some(blueoath_domain::CurrencyKind::Supply),
            19 => Some(blueoath_domain::CurrencyKind::Oil),
            20 => Some(blueoath_domain::CurrencyKind::BuildMaterial),
            30 => Some(blueoath_domain::CurrencyKind::PvePoint),
            _ => None,
        };
        if let Some(kind) = kind {
            let _ = account
                .resources
                .credit(kind, u64::try_from(reward.num).unwrap_or_default());
        }
    } else if let Ok(template_id) =
        blueoath_domain::TemplateId::new(u64::try_from(reward.item_id).unwrap_or_default())
    {
        let entry = account.inventory.items.entry(template_id).or_default();
        *entry = entry.saturating_add(u64::try_from(reward.num).unwrap_or_default());
    }
}

fn set_typed_building_assignments(
    account: &mut blueoath_domain::AccountState,
    assignments: &[(i32, Vec<i32>)],
    catalog: Option<&BuildingCatalog>,
) -> bool {
    if assignments.is_empty() {
        return false;
    }
    let mut moving = std::collections::BTreeSet::new();
    let mut seen_buildings = std::collections::BTreeSet::new();
    for (building_id, hero_ids) in assignments {
        let Ok(building_id) = u64::try_from(*building_id) else {
            return false;
        };
        if !seen_buildings.insert(building_id)
            || !account.buildings.levels.contains_key(&building_id)
        {
            return false;
        }
        let template_id = account
            .buildings
            .template_ids
            .get(&building_id)
            .copied()
            .unwrap_or_default();
        let level = account
            .buildings
            .levels
            .get(&building_id)
            .copied()
            .unwrap_or_default();
        if hero_ids.len()
            > building_capacity(
                i32::try_from(template_id).unwrap_or_default(),
                i32::try_from(level).unwrap_or_default(),
                catalog,
            )
        {
            return false;
        }
        for hero_id in hero_ids {
            let Ok(hero_id) = u64::try_from(*hero_id) else {
                return false;
            };
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return false;
            };
            if !account.dock.heroes.contains_key(&hero_id) || !moving.insert(hero_id) {
                return false;
            }
        }
    }
    for hero_ids in account.buildings.hero_assignments.values_mut() {
        hero_ids.retain(|hero_id| !moving.contains(hero_id));
    }
    for (building_id, hero_ids) in assignments {
        let building_id = u64::try_from(*building_id).unwrap_or_default();
        let converted = hero_ids
            .iter()
            .filter_map(|hero_id| u64::try_from(*hero_id).ok())
            .filter_map(|hero_id| blueoath_domain::HeroId::new(hero_id).ok())
            .collect();
        account
            .buildings
            .hero_assignments
            .insert(building_id, converted);
    }
    account
        .buildings
        .hero_assignments
        .retain(|_, hero_ids| !hero_ids.is_empty());
    true
}

fn append_typed_building_refresh(
    effects: &mut ResponseEffects,
    account: &mut blueoath_domain::AccountState,
    now: u32,
    catalog: Option<&BuildingCatalog>,
) {
    ensure_typed_building_productions(account, catalog, now);
    effects.push_pre(Response::raw(
        "building.UpdateBuildingInfo",
        UserBuildingInfoCodec::encode(&building_info_from_typed_account_with_catalog(
            account, now, catalog,
        )),
    ));
}

fn ensure_typed_building_productions(
    account: &mut blueoath_domain::AccountState,
    catalog: Option<&BuildingCatalog>,
    now: u32,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let building_ids = account.buildings.levels.keys().copied().collect::<Vec<_>>();
    for building_id in building_ids {
        let Some(template_id) = account
            .buildings
            .template_ids
            .get(&building_id)
            .and_then(|value| i32::try_from(*value).ok())
        else {
            continue;
        };
        let Some(config) = catalog.typed_building_configs.get(&template_id) else {
            continue;
        };
        if !matches!(config.building_type, 2 | 3 | 4 | 6) {
            continue;
        }
        let production = account
            .buildings
            .productions
            .entry(building_id)
            .or_default();
        if production.last_update_at == 0 {
            production.last_update_at = u64::from(now);
        }
        if production.status == 0 {
            production.status = 3;
        }
        production.productivity = u32::try_from(config.productivity.max(0)).unwrap_or_default();
        production.produce_speed =
            u32::try_from(crate::game_config::building_produce_speed(config).max(0))
                .unwrap_or_default();
    }
}

fn apply_typed_dorm_mood(
    account: &mut blueoath_domain::AccountState,
    catalog: Option<&BuildingCatalog>,
    now: u32,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let interval = u64::try_from(
        catalog
            .resource_time_seconds
            .get(&139)
            .copied()
            .unwrap_or(6)
            .max(1),
    )
    .unwrap_or(6)
    .saturating_mul(60);
    let now = u64::from(now);
    if account.buildings.mood_update_at == 0 {
        account.buildings.mood_update_at = now;
        return;
    }
    if now <= account.buildings.mood_update_at {
        return;
    }
    let periods = (now - account.buildings.mood_update_at) / interval;
    if periods == 0 {
        return;
    }
    let dorm_mood_by_hero = account
        .buildings
        .levels
        .iter()
        .filter_map(|(building_id, _)| {
            let template_id = account.buildings.template_ids.get(building_id)?;
            let template_id = i32::try_from(*template_id).ok()?;
            let config = catalog.typed_building_configs.get(&template_id)?;
            (config.building_type == 5 && config.add_mood > 0).then_some((
                account
                    .buildings
                    .hero_assignments
                    .get(building_id)
                    .cloned()
                    .unwrap_or_default(),
                config.add_mood,
            ))
        })
        .flat_map(|(hero_ids, add_mood)| {
            hero_ids.into_iter().map(move |hero_id| (hero_id, add_mood))
        })
        .collect::<Vec<_>>();
    for (hero_id, add_mood) in dorm_mood_by_hero {
        let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
            continue;
        };
        let recovery =
            i64::from(add_mood.max(0)).saturating_mul(i64::try_from(periods).unwrap_or(i64::MAX));
        hero.mood = hero
            .mood
            .saturating_add(u32::try_from(recovery).unwrap_or(u32::MAX))
            .min(MOOD_MAX as u32);
    }
    account.buildings.mood_update_at = account
        .buildings
        .mood_update_at
        .saturating_add(periods.saturating_mul(interval));
}

fn typed_worker_max_strength(
    account: &blueoath_domain::AccountState,
    catalog: &BuildingCatalog,
) -> u32 {
    let office_level = account
        .buildings
        .levels
        .iter()
        .find_map(|(building_id, level)| {
            let template_id = account.buildings.template_ids.get(building_id)?;
            let config = catalog
                .typed_building_configs
                .get(&i32::try_from(*template_id).ok()?)?;
            (config.building_type == 1).then_some(*level)
        })
        .unwrap_or(1)
        .max(1);
    typed_worker_max_strength_for_office_level(catalog, office_level)
}

fn typed_worker_max_strength_for_office_level(catalog: &BuildingCatalog, office_level: u32) -> u32 {
    let level_bonus = catalog
        .worker_hp_level_up
        .iter()
        .take(usize::try_from(office_level.max(1)).unwrap_or_default())
        .fold(0_i32, |total, bonus| total.saturating_add(*bonus));
    u32::try_from(
        catalog
            .worker_hp_max
            .saturating_add(level_bonus)
            .max(0)
            .saturating_mul(10_000),
    )
    .unwrap_or(u32::MAX)
}

fn ensure_typed_worker_state(
    account: &mut blueoath_domain::AccountState,
    catalog: Option<&BuildingCatalog>,
    now: u32,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let max_strength = typed_worker_max_strength(account, catalog);
    let update_at = account.buildings.worker_update_at;
    if account.buildings.worker_strength == 0 {
        account.buildings.worker_strength = max_strength;
        account.buildings.worker_update_at = u64::from(now);
        return;
    }
    account.buildings.worker_strength = account.buildings.worker_strength.min(max_strength);
    if update_at == 0 || u64::from(now) <= update_at {
        account.buildings.worker_update_at = u64::from(now);
        return;
    }
    let interval = u64::try_from(catalog.worker_recover_interval_seconds.max(1)).unwrap_or(60);
    let periods = (u64::from(now) - update_at) / interval;
    if periods == 0 {
        return;
    }
    let recovery = u64::try_from(catalog.worker_recover.max(0))
        .unwrap_or_default()
        .saturating_mul(periods)
        .saturating_mul(10_000);
    account.buildings.worker_strength = account
        .buildings
        .worker_strength
        .saturating_add(u32::try_from(recovery).unwrap_or(u32::MAX))
        .min(max_strength);
    account.buildings.worker_update_at = update_at.saturating_add(periods.saturating_mul(interval));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_catalog() -> BuildingCatalog {
        let mut catalog = BuildingCatalog::default();
        for (template_id, level) in [(1, 1), (2, 2), (3, 3), (41, 1), (42, 2)] {
            catalog.typed_building_configs.insert(
                template_id,
                BuildingConfig {
                    building_type: if template_id < 10 { 1 } else { 5 },
                    level,
                    hero_capacity: 5,
                    product_max: 0,
                    product_id: None,
                    productivity: 0,
                    produce_speed: 0,
                    ..BuildingConfig::default()
                },
            );
            catalog.capacities.insert(template_id, 5);
        }
        catalog.worker_hp_max = 50;
        catalog.worker_hp_level_up = vec![50, 50, 50];
        catalog
    }

    #[test]
    fn upgrading_building_changes_template_and_worker_strength() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("building-upgrade").unwrap(),
            "Captain",
        );
        let catalog = test_catalog();
        let mut effects = ResponseEffects::default();
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);

        assert!(matches!(
            handle_typed(
                &mut account,
                "building.UpgradeBuilding",
                &request,
                100,
                &mut effects,
                Some(&catalog),
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(account.buildings.levels.get(&1), Some(&3));
        assert_eq!(account.buildings.template_ids.get(&1), Some(&3));
        let info = building_info_from_typed_account_with_catalog(&account, 100, Some(&catalog));
        assert_eq!(info.worker_strength, 2_000_000);
        assert_eq!(info.buildings[0].template_id, 3);
        assert_eq!(info.buildings[0].level, 3);
    }

    #[test]
    fn degrading_building_restores_previous_template() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("building-degrade").unwrap(),
            "Captain",
        );
        account.buildings.levels.insert(1, 3);
        account.buildings.template_ids.insert(1, 3);
        let catalog = test_catalog();
        let mut effects = ResponseEffects::default();
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);

        assert!(matches!(
            handle_typed(
                &mut account,
                "building.DegradeBuilding",
                &request,
                100,
                &mut effects,
                Some(&catalog),
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(account.buildings.levels.get(&1), Some(&2));
        assert_eq!(account.buildings.template_ids.get(&1), Some(&2));
    }

    #[test]
    fn dormitory_recovers_assigned_hero_mood_from_configured_rate() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("dorm-mood").unwrap(),
            "Captain",
        );
        account
            .dock
            .heroes
            .get_mut(&blueoath_domain::HeroId::new(1).unwrap())
            .unwrap()
            .mood = 0;
        account
            .buildings
            .hero_assignments
            .insert(2, vec![blueoath_domain::HeroId::new(1).unwrap()]);
        let mut catalog = test_catalog();
        catalog.resource_time_seconds.insert(139, 6);
        catalog
            .typed_building_configs
            .get_mut(&41)
            .unwrap()
            .add_mood = 25_000;
        let mut effects = ResponseEffects::default();
        let request = Vec::new();

        assert!(matches!(
            handle_typed(
                &mut account,
                "building.UpdateBuildingInfo",
                &request,
                100,
                &mut effects,
                Some(&catalog),
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.dock.heroes[&blueoath_domain::HeroId::new(1).unwrap()].mood,
            0
        );

        assert!(matches!(
            handle_typed(
                &mut account,
                "building.UpdateBuildingInfo",
                &request,
                460,
                &mut effects,
                Some(&catalog),
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.dock.heroes[&blueoath_domain::HeroId::new(1).unwrap()].mood,
            25_000
        );
    }

    #[test]
    fn refinery_uses_configured_productivity_for_wire_produce_speed() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("refinery-speed").unwrap(),
            "Captain",
        );
        account.buildings.levels.insert(3, 2);
        account.buildings.template_ids.insert(3, 22);

        let mut catalog = test_catalog();
        catalog.typed_building_configs.insert(
            22,
            BuildingConfig {
                building_type: 3,
                level: 2,
                product_max: 5_400,
                product_id: Some(5),
                productivity: 310_000,
                ..BuildingConfig::default()
            },
        );

        let info = building_info_from_typed_account_with_catalog(&account, 100, Some(&catalog));
        let refinery = info
            .buildings
            .iter()
            .find(|building| building.id == 3)
            .expect("refinery is projected");
        assert_eq!(refinery.productivity, 310_000);
        assert_eq!(refinery.produce_speed, 310_000);
        assert_eq!(refinery.product_count, 0);
        assert_eq!(refinery.status, 3);
    }

    #[test]
    fn receiving_refinery_supply_pushes_updated_user_info() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("refinery-supply-push").unwrap(),
            "Captain",
        );
        account.buildings.levels.insert(3, 2);
        account.buildings.template_ids.insert(3, 22);
        account.buildings.productions.insert(
            3,
            blueoath_domain::BuildingProductionState {
                status: 1,
                product_count: 123,
                ..blueoath_domain::BuildingProductionState::default()
            },
        );

        let mut catalog = test_catalog();
        catalog.typed_building_configs.insert(
            22,
            BuildingConfig {
                building_type: 3,
                level: 2,
                product_max: 5_400,
                product_id: Some(5),
                productivity: 310_000,
                ..BuildingConfig::default()
            },
        );
        let supply_before = account
            .resources
            .amount(blueoath_domain::CurrencyKind::Supply)
            .get();
        let server_state = ServerState::new("refinery-supply-push", "Captain", "test");
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 5);
        let mut effects = ResponseEffects::default();

        assert!(matches!(
            handle_typed_with_multipliers(
                Some(&server_state),
                &mut account,
                "building.ReceiveResource",
                &request,
                100,
                &mut effects,
                BuildingTypedCatalogs {
                    building: Some(&catalog),
                    oil_multiplier: 1.0,
                    gold_multiplier: 1.0,
                },
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Supply)
                .get(),
            supply_before + 123
        );
        let (pre, _, _) = effects.into_parts();
        assert!(pre
            .iter()
            .any(|response| response.method == "user.UpdateUserInfo"));
    }
}
