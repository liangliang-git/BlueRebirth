use super::*;

pub(crate) fn copy_info_payload(
    catalog: &ChapterCatalog,
    copy_type: i32,
    account: &AccountState,
) -> blueoath_protocol::CopyInfoPayload {
    let passed_copy_ids = account
        .battle
        .passed_copies
        .iter()
        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
        .collect::<Vec<_>>();
    let copy_star_levels = account
        .battle
        .copy_stars
        .iter()
        .filter_map(|(copy_id, stars)| {
            Some((
                i32::try_from(copy_id.get()).ok()?,
                i32::try_from(*stars).ok()?,
            ))
        })
        .collect::<Vec<_>>();
    let mubar_passed_copy_ids = passed_copy_ids
        .iter()
        .copied()
        .filter(|copy_id| catalog.mubar.contains(copy_id))
        .collect::<Vec<_>>();
    let (copy_ids, max_copy_id, response_passed) = match copy_type {
        2 => (
            catalog.sea.clone(),
            copy_progress_max_or_initial(&catalog.sea, &passed_copy_ids, catalog.sea_initial),
            passed_copy_ids.to_vec(),
        ),
        33 => (
            catalog.mubar.clone(),
            copy_progress_max_or_first(&catalog.mubar, &mubar_passed_copy_ids),
            mubar_passed_copy_ids,
        ),
        10 => (
            catalog.goods_copy.clone(),
            catalog.goods_copy.iter().copied().max().unwrap_or_default(),
            catalog.goods_copy.clone(),
        ),
        24 => (
            catalog.tower.clone(),
            catalog.tower.iter().copied().max().unwrap_or_default(),
            catalog.tower.clone(),
        ),
        34 => (
            catalog.equip_new_test.clone(),
            catalog
                .equip_new_test
                .iter()
                .copied()
                .max()
                .unwrap_or_default(),
            catalog.equip_new_test.clone(),
        ),
        9 => (
            catalog.daily.clone(),
            catalog.daily.iter().copied().max().unwrap_or_default(),
            catalog.daily.clone(),
        ),
        _ => (
            catalog.plot.clone(),
            copy_progress_max_or_first(&catalog.plot, &passed_copy_ids),
            passed_copy_ids.to_vec(),
        ),
    };
    blueoath_protocol::CopyInfoPayload {
        copy_type,
        chapter_star_infos: copy_chapter_star_infos(catalog, &copy_ids, account),
        copy_ids,
        max_copy_id,
        passed_copy_ids: response_passed,
        passed_copy_counts: Vec::new(),
        copy_star_levels,
        difficulty: if copy_type == 2 {
            account.sea.difficulty.max(1) as i32
        } else {
            1
        },
    }
}

fn copy_chapter_star_infos(
    catalog: &ChapterCatalog,
    copy_ids: &[i32],
    account: &AccountState,
) -> Vec<blueoath_protocol::CopyChapterStarInfo> {
    catalog
        .star_rewards_by_chapter
        .iter()
        .filter(|(_, chapter)| chapter.level_ids.iter().any(|id| copy_ids.contains(id)))
        .map(|(chapter_id, chapter)| {
            let passed_ids = chapter.level_ids.iter().filter_map(|copy_id| {
                let copy_id = u64::try_from(*copy_id)
                    .ok()
                    .and_then(|id| blueoath_domain::CopyId::new(id).ok())?;
                account
                    .battle
                    .passed_copies
                    .contains(&copy_id)
                    .then_some(copy_id)
            });
            let passed_ids = passed_ids.collect::<Vec<_>>();
            let star_num = passed_ids
                .iter()
                .map(|copy_id| {
                    account
                        .battle
                        .copy_stars
                        .get(copy_id)
                        .copied()
                        .unwrap_or(7)
                        .min(7)
                        .count_ones() as i32
                })
                .sum();
            let claimed_reward_indexes = account
                .battle
                .claimed_star_rewards
                .iter()
                .filter_map(|(claimed_chapter, index)| {
                    (*claimed_chapter == *chapter_id as u32)
                        .then(|| i32::try_from(*index).ok())
                        .flatten()
                })
                .collect();
            blueoath_protocol::CopyChapterStarInfo {
                chapter_id: *chapter_id,
                star_num,
                claimed_reward_indexes,
                pass_num: i32::try_from(passed_ids.len()).unwrap_or(i32::MAX),
            }
        })
        .collect()
}

pub(crate) fn copy_type_for_chapter(catalog: &ChapterCatalog, chapter_id: i32) -> i32 {
    let Some(chapter) = catalog.star_rewards_by_chapter.get(&chapter_id) else {
        return 1;
    };
    [
        (2, &catalog.sea),
        (33, &catalog.mubar),
        (10, &catalog.goods_copy),
        (24, &catalog.tower),
        (34, &catalog.equip_new_test),
        (9, &catalog.daily),
    ]
    .into_iter()
    .find_map(|(copy_type, copy_ids)| {
        chapter
            .level_ids
            .iter()
            .any(|copy_id| copy_ids.contains(copy_id))
            .then_some(copy_type)
    })
    .unwrap_or(1)
}

pub(crate) fn copy_type_for_copy_id(catalog: &ChapterCatalog, copy_id: i32) -> i32 {
    [
        (2, &catalog.sea),
        (33, &catalog.mubar),
        (10, &catalog.goods_copy),
        (24, &catalog.tower),
        (34, &catalog.equip_new_test),
        (9, &catalog.daily),
        (1, &catalog.plot),
    ]
    .into_iter()
    .find_map(|(copy_type, copy_ids)| copy_ids.contains(&copy_id).then_some(copy_type))
    .unwrap_or(1)
}

pub(crate) fn copy_progress_payload_for_copy(
    catalog: &ChapterCatalog,
    copy_id: i32,
    account: &AccountState,
) -> Vec<u8> {
    CopyInfoCodec::encode_payload(&copy_info_payload(
        catalog,
        copy_type_for_copy_id(catalog, copy_id),
        account,
    ))
}

pub(crate) fn copy_bottom_index_for_type(copy_type: i32) -> i32 {
    match copy_type {
        2 => 2,
        33 => 3,
        9 => 4,
        _ => 1,
    }
}

pub(crate) fn copy_navigation_prefs_json(stored: Option<&str>, copy_bottom_index: i32) -> String {
    let mut prefs = stored
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    prefs.insert(
        "NewCopyButtomIndex".to_owned(),
        serde_json::Value::from(copy_bottom_index),
    );
    serde_json::Value::Object(prefs).to_string()
}

pub(crate) fn set_copy_navigation_pref(
    account: &mut AccountState,
    catalog: &ChapterCatalog,
    copy_id: i32,
    now: u32,
) -> Vec<u8> {
    let copy_type = copy_type_for_copy_id(catalog, copy_id);
    let copy_bottom_index = copy_bottom_index_for_type(copy_type);
    let prefs = copy_navigation_prefs_json(
        account
            .guide
            .settings
            .get(super::features::user::misc_service::CLIENT_PREFS_SETTING_KEY)
            .map(String::as_str),
        copy_bottom_index,
    );
    account.guide.settings.insert(
        super::features::user::misc_service::CLIENT_PREFS_SETTING_KEY.to_owned(),
        prefs.clone(),
    );
    let mut payload = Vec::new();
    append_message_field(&mut payload, 1, prefs.as_bytes());
    append_varint_field(&mut payload, 2, u64::from(now));
    payload
}
