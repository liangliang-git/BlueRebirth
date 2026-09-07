use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{AccountState, BathroomHeroState};

pub(super) fn handle_bathroom_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    mood_recovery_multiplier: f64,
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let requested_hero_id = decode_varint_u64_field(request_args, 1);
    let before = account
        .bathroom
        .heroes
        .iter()
        .find(|hero| hero.hero_id == requested_hero_id)
        .cloned();

    let response = match method {
        "bathroom.GetBathroomInfo" => bathroom_info_payload_from_typed(account),
        "bathroom.BathStart" => {
            let position = decode_varint_u64_field(request_args, 2);
            if requested_hero_id == 0
                || !account
                    .dock
                    .heroes
                    .contains_key(&hero_key(requested_hero_id))
            {
                return HandlerResult::Error(GameError::InvalidRequest("bathroom hero is invalid"));
            }
            start_bathroom_hero(
                account,
                requested_hero_id,
                u32::try_from(position).unwrap_or(u32::MAX),
                now,
            );
            bathroom_info_payload_from_typed(account)
        }
        "bathroom.BathEnd" | "bathroom.BathChangeHero" => {
            account
                .bathroom
                .heroes
                .retain(|hero| hero.hero_id != requested_hero_id);
            if let Some(before) = before.as_ref() {
                recover_typed_hero_mood(
                    account,
                    before.hero_id,
                    before
                        .bath_time
                        .max(u64::from(now).saturating_sub(before.start_time)),
                    mood_recovery_multiplier,
                    now,
                );
            }
            append_method_push(
                post_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            );
            bathroom_end_payload(
                requested_hero_id,
                before.map(|hero| hero.bath_time).unwrap_or_default() as i64,
            )
        }
        "bathroom.BathService" => {
            let hero_id = requested_hero_id;
            let hero = account
                .bathroom
                .heroes
                .iter()
                .find(|hero| hero.hero_id == hero_id);
            bathroom_service_payload(
                hero_id,
                hero.map(|hero| i64::from(hero.position))
                    .unwrap_or_default(),
                hero.map(|hero| hero.bath_time as i64).unwrap_or_default(),
            )
        }
        "bathroom.BathAuto" => {
            let is_auto = decode_varint_u64_field(request_args, 2) != 0;
            if let Some(hero) = account
                .bathroom
                .heroes
                .iter_mut()
                .find(|hero| hero.hero_id == requested_hero_id)
            {
                hero.is_auto = is_auto;
            }
            Vec::new()
        }
        "bathroom.BathAllAuto" => {
            account.bathroom.is_all_auto = requested_hero_id != 0;
            Vec::new()
        }
        "bathroom.BathStartAll" => {
            let mut output = Vec::new();
            for nested in decode_repeated_message_field(request_args, 1) {
                let hero_id = decode_varint_u64_field(&nested, 1);
                let position = decode_varint_u64_field(&nested, 2);
                if hero_id == 0 {
                    continue;
                }
                start_bathroom_hero(
                    account,
                    hero_id,
                    u32::try_from(position).unwrap_or(u32::MAX),
                    now,
                );
                append_message_field(
                    &mut output,
                    1,
                    &bathroom_end_payload(
                        hero_id,
                        account
                            .bathroom
                            .heroes
                            .iter()
                            .find(|hero| hero.hero_id == hero_id)
                            .map(|hero| hero.bath_time as i64)
                            .unwrap_or_default(),
                    ),
                );
            }
            output
        }
        _ => return HandlerResult::Empty,
    };

    append_method_push(
        post_pushes,
        "bathroom.BathroomInfo",
        bathroom_info_payload_from_typed(account),
    );
    HandlerResult::Reply(Response::raw(method, response))
}

pub(super) fn handle_study_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "study.GetStudyInfo" => HandlerResult::Reply(Response::raw(
            method,
            study_info_payload_from_typed(account, now),
        )),
        "study.StartStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            let textbook_id = decode_varint_u64_field(request_args, 3);
            let valid = hero_id > 0
                && skill_id > 0
                && textbook_id > 0
                && account.dock.heroes.contains_key(&hero_key(hero_id))
                && account.study.progress.len() < 2
                && !account
                    .study
                    .progress
                    .iter()
                    .any(|progress| progress.hero_id == hero_id)
                && typed_item_count(account, textbook_id) > 0;
            if !valid {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study slot, hero, or textbook is invalid",
                ));
            }
            let _ = consume_typed_item(account, textbook_id, 1);
            account
                .study
                .progress
                .push(blueoath_domain::StudyProgressState {
                    hero_id,
                    skill_id,
                    textbook_id,
                    begin_time: u64::from(now),
                    end_time: u64::from(now.saturating_add(60)),
                });
            append_method_push(
                post_pushes,
                "study.GetStudyInfo",
                study_info_payload_from_typed(account, now),
            );
            append_method_push(
                post_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "study.CancelStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let requested_skill_id = decode_varint_u64_field(request_args, 2);
            let index = account.study.progress.iter().position(|progress| {
                progress.hero_id == hero_id
                    && (requested_skill_id == 0 || progress.skill_id == requested_skill_id)
            });
            let Some(index) = index else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study progress is missing",
                ));
            };
            account.study.progress.remove(index);
            append_method_push(
                post_pushes,
                "study.GetStudyInfo",
                study_info_payload_from_typed(account, now),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "study.EndStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            match finish_study_typed(account, hero_id, skill_id, now, false) {
                Ok(payload) => {
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                    );
                    append_method_push(
                        post_pushes,
                        "study.GetStudyInfo",
                        study_info_payload_from_typed(account, now),
                    );
                    HandlerResult::Reply(Response::raw(method, payload))
                }
                Err(error) => HandlerResult::Error(error),
            }
        }
        "study.SpeedUpStudy" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            let items = decode_repeated_message_field(request_args, 3)
                .into_iter()
                .map(|item| {
                    (
                        decode_varint_u64_field(&item, 1),
                        decode_varint_u64_field(&item, 2),
                    )
                })
                .filter(|(item_id, count)| *item_id > 0 && *count > 0)
                .collect::<Vec<_>>();
            if items.is_empty()
                || !account
                    .study
                    .progress
                    .iter()
                    .any(|progress| progress.hero_id == hero_id && progress.skill_id == skill_id)
                || items
                    .iter()
                    .any(|(item_id, count)| typed_item_count(account, *item_id) < *count)
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study progress or speedup items are missing",
                ));
            }
            for (item_id, count) in items {
                let _ = consume_typed_item(account, item_id, count);
            }
            match finish_study_typed(account, hero_id, skill_id, now, true) {
                Ok(payload) => {
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                    );
                    append_method_push(
                        post_pushes,
                        "study.GetStudyInfo",
                        study_info_payload_from_typed(account, now),
                    );
                    HandlerResult::Reply(Response::raw(method, payload))
                }
                Err(error) => HandlerResult::Error(error),
            }
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_item_count(account: &AccountState, template_id: u64) -> u64 {
    blueoath_domain::TemplateId::new(template_id)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

fn consume_typed_item(account: &mut AccountState, template_id: u64, count: u64) -> bool {
    let Ok(template_id) = blueoath_domain::TemplateId::new(template_id) else {
        return false;
    };
    let Some(amount) = account.inventory.items.get_mut(&template_id) else {
        return false;
    };
    if *amount < count {
        return false;
    }
    *amount -= count;
    if *amount == 0 {
        account.inventory.items.remove(&template_id);
    }
    true
}

fn finish_study_typed(
    account: &mut AccountState,
    hero_id: u64,
    skill_id: u64,
    now: u32,
    force: bool,
) -> Result<Vec<u8>, GameError> {
    let index = account
        .study
        .progress
        .iter()
        .position(|progress| progress.hero_id == hero_id && progress.skill_id == skill_id)
        .ok_or(GameError::InvalidState("study progress is missing"))?;
    if !force && account.study.progress[index].end_time > u64::from(now) {
        return Err(GameError::InvalidRequest("study is not finished"));
    }
    let hero_key = hero_key(hero_id);
    if !account.dock.heroes.contains_key(&hero_key) {
        return Err(GameError::NotFound("study hero"));
    }
    let progress = account.study.progress.remove(index);
    let hero = account
        .dock
        .heroes
        .get_mut(&hero_key)
        .expect("study hero validated before progress removal");
    let before = hero.pskills.get(&skill_id).copied().unwrap_or_default();
    let after = before.saturating_add(1).max(1);
    hero.pskills.insert(skill_id, after);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, hero_id);
    append_varint_field(&mut output, 2, skill_id);
    append_varint_field(&mut output, 3, u64::from(before));
    append_varint_field(&mut output, 4, u64::from(after));
    append_varint_field(&mut output, 5, progress.textbook_id);
    Ok(output)
}

fn study_info_payload_from_typed(account: &AccountState, _now: u32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 2);
    for progress in &account.study.progress {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, progress.hero_id);
        append_varint_field(&mut item, 2, progress.skill_id);
        append_varint_field(&mut item, 3, progress.textbook_id);
        append_varint_field(&mut item, 4, progress.begin_time);
        append_varint_field(&mut item, 5, progress.end_time);
        append_message_field(&mut output, 2, &item);
    }
    output
}

fn hero_key(value: u64) -> blueoath_domain::HeroId {
    blueoath_domain::HeroId::new(value).expect("validated positive hero id")
}

fn start_bathroom_hero(account: &mut AccountState, hero_id: u64, position: u32, now: u32) {
    account
        .bathroom
        .heroes
        .retain(|hero| hero.hero_id != hero_id);
    account.bathroom.heroes.push(BathroomHeroState {
        hero_id,
        position,
        start_time: u64::from(now),
        ..BathroomHeroState::default()
    });
}

fn recover_typed_hero_mood(
    account: &mut AccountState,
    hero_id: u64,
    bath_seconds: u64,
    multiplier: f64,
    now: u32,
) {
    let Some(hero) = account.dock.heroes.get_mut(&hero_key(hero_id)) else {
        return;
    };
    let intervals = i64::try_from(bath_seconds).unwrap_or(i64::MAX) / MOOD_BATH_INTERVAL_SECONDS;
    let base = if intervals > 0 {
        i64::from(MOOD_BATH_INTERVAL_RECOVERY)
            .saturating_mul(intervals)
            .min(i64::from(MOOD_BATH_RECOVERY))
    } else {
        i64::from(MOOD_BATH_RECOVERY)
    };
    let recovery = scale_reward(base, multiplier);
    hero.mood = hero
        .mood
        .saturating_add(u32::try_from(recovery.max(0)).unwrap_or(u32::MAX))
        .min(MOOD_MAX as u32);
    let _ = now;
}

fn bathroom_info_payload_from_typed(account: &AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    if account.bathroom.heroes.is_empty() {
        output.extend_from_slice(&[0x0A, 0x00]);
    } else {
        for hero in &account.bathroom.heroes {
            let mut encoded = Vec::new();
            append_varint_field(&mut encoded, 1, hero.hero_id);
            append_varint_field(&mut encoded, 2, u64::from(hero.position));
            append_varint_field(&mut encoded, 3, u64::from(hero.is_auto));
            append_varint_field(&mut encoded, 4, hero.start_time);
            append_varint_field(&mut encoded, 5, hero.bath_time);
            append_varint_field(&mut encoded, 6, u64::from(hero.buff_id));
            append_varint_field(&mut encoded, 7, hero.buff_time);
            append_varint_field(&mut encoded, 8, u64::from(hero.power));
            append_message_field(&mut output, 1, &encoded);
        }
    }
    if account.bathroom.is_all_auto {
        append_varint_field(&mut output, 2, 1);
    }
    output
}

#[cfg(test)]
mod typed_tests {
    use super::*;

    #[test]
    fn typed_bathroom_start_and_end_update_domain_state() {
        let hero_id = blueoath_domain::HeroId::new(9).unwrap();
        let mut account = AccountState::default();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(100).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 0,
                affection: 0,
                hp: 100,
                locked: false,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        let mut start = Vec::new();
        append_varint_field(&mut start, 1, 9);
        append_varint_field(&mut start, 2, 3);
        let mut pushes = Vec::new();
        let result = handle_bathroom_typed(
            &mut account,
            "bathroom.BathStart",
            &start,
            100,
            1.0,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.bathroom.heroes[0].position, 3);

        let mut end = Vec::new();
        append_varint_field(&mut end, 1, 9);
        let result = handle_bathroom_typed(
            &mut account,
            "bathroom.BathEnd",
            &end,
            100,
            1.0,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(account.bathroom.heroes.is_empty());
        assert_eq!(account.dock.heroes[&hero_id].mood, 300_000);
        assert_eq!(pushes.len(), 3);
    }

    #[test]
    fn typed_study_consumes_textbook_and_levels_skill() {
        let hero_id = blueoath_domain::HeroId::new(9).unwrap();
        let textbook_id = blueoath_domain::TemplateId::new(7001).unwrap();
        let mut account = AccountState::default();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(100).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 0,
                affection: 0,
                hp: 100,
                locked: false,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        account.inventory.items.insert(textbook_id, 1);
        let mut start = Vec::new();
        append_varint_field(&mut start, 1, 9);
        append_varint_field(&mut start, 2, 41);
        append_varint_field(&mut start, 3, 7001);
        let mut pushes = Vec::new();
        let result = handle_study_typed(
            &mut account,
            "study.StartStudyPSkill",
            &start,
            100,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(typed_item_count(&account, 7001), 0);

        let mut end = Vec::new();
        append_varint_field(&mut end, 1, 9);
        append_varint_field(&mut end, 2, 41);
        let result =
            handle_study_typed(&mut account, "study.EndStudyPSkill", &end, 200, &mut pushes);
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(account.study.progress.is_empty());
        assert_eq!(account.dock.heroes[&hero_id].pskills.get(&41), Some(&1));
    }
}
