use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response};
use crate::*;
use blueoath_protocol::{CopyIdRequest, Decode};

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> HandlerResult {
    match method {
        "tower.GetTowerInfo" => reply(
            method,
            tower_info_payload_typed(account, chapter_catalog, now),
        ),
        "tower.Reset" => {
            account.tower.daily_count = 0;
            account.tower.daily_count_ex = 0;
            account.tower.reset_time = u64::from(now);
            account.tower.hero_ids.clear();
            account.tower.lock_equip_ids.clear();
            account.tower.is_reset = true;
            HandlerResult::PushOnly
        }
        "tower.ResetChangeHeroIdList" => {
            account.tower.hero_ids.clear();
            HandlerResult::PushOnly
        }
        "tower.Receive" => {
            account.tower.daily_count = account.tower.daily_count.saturating_add(1);
            reply(method, tower_reward_payload_typed(account, None))
        }
        "tower.Replacement" => {
            account.tower.topic_index = account.tower.topic_index.saturating_add(1);
            reply(
                method,
                tower_info_payload_typed(account, chapter_catalog, now),
            )
        }
        "tower.SendUpgrade" => {
            account.tower.max_level = account.tower.max_level.saturating_add(1);
            account.tower.is_new_level = true;
            reply(
                method,
                tower_info_payload_typed(account, chapter_catalog, now),
            )
        }
        "tower.ReceiveBuff" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "tower copy request is invalid",
                ));
            };
            let copy_id = request.copy_id;
            let copy_id_value = u32::try_from(copy_id).ok();
            if let Ok(copy_id) = blueoath_domain::CopyId::new(copy_id as u64) {
                if !account.tower.save_pass_copy_ids.contains(&copy_id) {
                    account.tower.save_pass_copy_ids.push(copy_id);
                }
            }
            reply(method, tower_reward_payload_typed(account, copy_id_value))
        }
        _ => HandlerResult::Empty,
    }
}

pub(crate) fn handle_activity_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
) -> HandlerResult {
    match method {
        "activityTower.ActivityTower" | "activityTower.GetActivityTower" => {
            reply(method, activity_tower_payload_typed(account, now))
        }
        "activityTower.Reset" => {
            account.activity_tower.reset_time = u64::from(now);
            account.activity_tower.small_reset_number =
                account.activity_tower.small_reset_number.saturating_add(1);
            account.activity_tower.quick_number = 0;
            account.activity_tower.pass_copy_ids.clear();
            account.activity_tower.save_pass_copy_ids.clear();
            account.activity_tower.save_pass_stage_copy_ids.clear();
            reply(method, activity_tower_payload_typed(account, now))
        }
        "activityTower.ReceiveBuff" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "activity tower copy request is invalid",
                ));
            };
            let copy_id = request.copy_id;
            if let Ok(copy_id) = blueoath_domain::CopyId::new(copy_id as u64) {
                if !account.activity_tower.pass_copy_ids.contains(&copy_id) {
                    account.activity_tower.pass_copy_ids.push(copy_id);
                }
            }
            reply(method, activity_tower_payload_typed(account, now))
        }
        "activityTower.QuickPass" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "activity tower copy request is invalid",
                ));
            };
            let copy_id = request.copy_id;
            if let Ok(copy_id) = blueoath_domain::CopyId::new(copy_id as u64) {
                if !account.activity_tower.pass_copy_ids.contains(&copy_id) {
                    account.activity_tower.pass_copy_ids.push(copy_id);
                }
            }
            account.activity_tower.quick_number =
                account.activity_tower.quick_number.saturating_add(1);
            account.activity_tower.history_max = account
                .activity_tower
                .history_max
                .max(account.activity_tower.quick_number);
            reply(method, activity_tower_payload_typed(account, now))
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

pub(crate) fn tower_info_payload_typed(
    account: &blueoath_domain::AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> Vec<u8> {
    let tower = &account.tower;
    let chapter_id = if tower.chapter_id > 0 {
        tower.chapter_id
    } else {
        chapter_catalog
            .map(|catalog| catalog.tower_chapter_id)
            .filter(|id| *id > 0)
            .and_then(|id| u32::try_from(id).ok())
            .unwrap_or(30_001)
    };
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, u64::from(chapter_id));
    append_varint_field(&mut output, 2, u64::from(tower.area_index));
    append_varint_field(&mut output, 3, u64::from(tower.copy_index));
    append_varint_field(&mut output, 4, u64::from(tower.topic_index));
    append_varint_field(&mut output, 5, u64::from(tower.daily_count));
    append_varint_field(
        &mut output,
        6,
        if tower.reset_time > 0 {
            tower.reset_time
        } else {
            u64::from(now)
        },
    );
    for (sf_id, count) in &tower.sf_id_counts {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, *sf_id);
        append_varint_field(&mut encoded, 2, u64::from(*count));
        append_message_field(&mut output, 7, &encoded);
    }
    for hero_id in &tower.hero_ids {
        append_varint_field(&mut output, 9, hero_id.get());
    }
    for equip_id in &tower.lock_equip_ids {
        append_varint_field(&mut output, 10, equip_id.get());
    }
    append_varint_field(&mut output, 11, u64::from(tower.pass_last_chapter_id));
    append_varint_field(&mut output, 12, u64::from(tower.is_reset));
    append_varint_field(&mut output, 13, u64::from(tower.max_level));
    append_varint_field(&mut output, 14, u64::from(tower.max_area));
    append_varint_field(&mut output, 15, u64::from(tower.max_copy));
    append_varint_field(&mut output, 16, u64::from(tower.daily_count_ex));
    append_varint_field(&mut output, 17, u64::from(tower.is_new_level));
    for copy_id in &tower.save_pass_copy_ids {
        append_varint_field(&mut output, 18, copy_id.get());
    }
    output
}

pub(crate) fn activity_tower_payload_typed(
    account: &blueoath_domain::AccountState,
    now: u32,
) -> Vec<u8> {
    let tower = &account.activity_tower;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, u64::from(tower.activity_id));
    append_varint_field(
        &mut output,
        2,
        if tower.reset_time > 0 {
            tower.reset_time
        } else {
            u64::from(now)
        },
    );
    append_varint_field(&mut output, 3, u64::from(tower.small_reset_number));
    append_varint_field(&mut output, 4, u64::from(tower.quick_number));
    append_varint_field(&mut output, 5, u64::from(tower.history_max));
    for copy_id in &tower.save_pass_copy_ids {
        append_varint_field(&mut output, 6, copy_id.get());
    }
    for copy_id in &tower.pass_copy_ids {
        append_varint_field(&mut output, 7, copy_id.get());
    }
    for equip_id in &tower.lock_equip_ids {
        append_varint_field(&mut output, 9, equip_id.get());
    }
    for hero_id in &tower.hero_ids {
        append_varint_field(&mut output, 10, hero_id.get());
    }
    for copy_id in &tower.save_pass_stage_copy_ids {
        append_varint_field(&mut output, 11, copy_id.get());
    }
    output
}

fn tower_reward_payload_typed(
    account: &blueoath_domain::AccountState,
    copy_id: Option<u32>,
) -> Vec<u8> {
    let tower = &account.tower;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, u64::from(tower.chapter_id));
    append_varint_field(&mut output, 2, u64::from(tower.area_index));
    append_varint_field(&mut output, 3, u64::from(tower.copy_index));
    append_varint_field(&mut output, 4, u64::from(tower.topic_index));
    for reward in &tower.pending_rewards {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, u64::from(reward.reward_type));
        append_varint_field(&mut encoded, 2, reward.config_id);
        append_varint_field(&mut encoded, 3, reward.amount);
        append_varint_field(&mut encoded, 4, reward.instance_id);
        append_message_field(&mut output, 5, &encoded);
    }
    if let Some(copy_id) = copy_id {
        append_varint_field(&mut output, 6, u64::from(copy_id));
    }
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handlers_expose_typed_results() {
        let _: fn(
            &mut blueoath_domain::AccountState,
            &str,
            &[u8],
            Option<&ChapterCatalog>,
            u32,
        ) -> HandlerResult = handle_typed;
        let _: fn(&mut blueoath_domain::AccountState, &str, &[u8], u32) -> HandlerResult =
            handle_activity_typed;
    }

    #[test]
    fn typed_tower_routes_use_domain_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("tower").unwrap(),
            "Tower",
        );
        assert!(matches!(
            handle_typed(&mut account, "tower.SendUpgrade", &[], None, 100),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.tower.max_level, 1);
        assert!(matches!(
            handle_activity_typed(&mut account, "activityTower.QuickPass", &[8, 9], 100),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.activity_tower.quick_number, 1);
    }
}
