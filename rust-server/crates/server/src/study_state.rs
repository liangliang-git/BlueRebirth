#![allow(dead_code)]

use serde_json::{json, Value};

use super::*;

pub(super) fn study_skill_state(account: &mut Value, hero_id: u64, skill_id: i32) -> bool {
    if hero_id == 0 || skill_id <= 0 {
        return false;
    }
    let Some(hero) = find_hero_mut(account, hero_id) else {
        return false;
    };
    let skills = hero
        .entry("pSkills".to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut();
    let Some(skills) = skills else {
        return false;
    };
    if let Some(skill) = skills.iter_mut().find(|value| {
        json_i32(value, "pSkillId").or_else(|| json_i32(value, "pskillId")) == Some(skill_id)
    }) {
        // C# snapshots used pSkillLv/pskillLv; normalize before incrementing so
        // legacy level 1 becomes level 2 instead of resetting to 1 forever.
        let level = json_i32(skill, "level")
            .or_else(|| json_i32(skill, "pSkillLv"))
            .or_else(|| json_i32(skill, "pskillLv"))
            .unwrap_or(1)
            .max(1);
        skill["level"] = json!(level.saturating_add(1));
    } else {
        skills.push(json!({
            "pSkillId": skill_id,
            "pSkillExp": 0,
            "level": 2,
            "replace": 0
        }));
    }
    true
}

/// TStopStudyPSkillArg contains only HeroId. Resolve the active study row so
/// CancelStudyPSkill and EndStudyPSkill operate on the client protocol shape.
pub(super) fn resolve_study_skill_id(
    account: &Value,
    hero_id: u64,
    requested_skill_id: i32,
) -> Option<i32> {
    if hero_id == 0 {
        return None;
    }
    account
        .get("study")
        .and_then(|study| study.get("progress"))
        .and_then(Value::as_array)?
        .iter()
        .find(|row| {
            json_u64(row, "heroId") == Some(hero_id)
                && (requested_skill_id <= 0
                    || json_i32(row, "pSkillId") == Some(requested_skill_id))
        })
        .and_then(|row| json_i32(row, "pSkillId"))
        .filter(|skill_id| *skill_id > 0)
}

pub(super) fn study_info_payload(account: &Value, now: u32) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, 2);
    if let Some(progress) = account
        .get("study")
        .and_then(|v| v.get("progress"))
        .and_then(Value::as_array)
    {
        for row in progress {
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, json_u64(row, "heroId").unwrap_or_default());
            append_varint_field(
                &mut item,
                2,
                json_i64(row, "pSkillId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                3,
                json_i64(row, "textbookId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                4,
                json_i64(row, "beginTime").unwrap_or(i64::from(now)).max(0) as u64,
            );
            append_varint_field(
                &mut item,
                5,
                json_i64(row, "endTime").unwrap_or(i64::from(now)).max(0) as u64,
            );
            append_message_field(&mut out, 2, &item);
        }
    }
    out
}

pub(super) fn start_study_state(
    account: &mut Value,
    hero_id: u64,
    skill_id: i32,
    textbook_id: i32,
    now: u32,
) -> bool {
    if hero_id == 0
        || skill_id <= 0
        || textbook_id <= 0
        || find_hero_mut(account, hero_id).is_none()
    {
        return false;
    }
    let active = account
        .get("study")
        .and_then(|v| v.get("progress"))
        .and_then(Value::as_array);
    if active.is_some_and(|p| {
        p.len() >= 2 || p.iter().any(|row| json_u64(row, "heroId") == Some(hero_id))
    }) {
        return false;
    }
    if bag_item_count(account, textbook_id) < 1 {
        return false;
    }
    consume_bag_item(account, textbook_id, 1);
    let Some(account) = account.as_object_mut() else {
        return false;
    };
    let study = account
        .entry("study".to_owned())
        .or_insert_with(|| json!({"progress": []}));
    let Some(study) = study.as_object_mut() else {
        return false;
    };
    let progress = study
        .entry("progress".to_owned())
        .or_insert_with(|| json!([]));
    let Some(progress) = progress.as_array_mut() else {
        return false;
    };
    progress.push(json!({"heroId": hero_id, "pSkillId": skill_id, "textbookId": textbook_id, "beginTime": now, "endTime": now.saturating_add(60)}));
    true
}

pub(super) fn finish_study_state(
    account: &mut Value,
    hero_id: u64,
    skill_id: i32,
    now: u32,
) -> Option<Vec<u8>> {
    finish_study_state_inner(account, hero_id, skill_id, now, false)
}

pub(super) fn finish_study_state_force(
    account: &mut Value,
    hero_id: u64,
    skill_id: i32,
    now: u32,
) -> Option<Vec<u8>> {
    finish_study_state_inner(account, hero_id, skill_id, now, true)
}

pub(super) fn finish_study_state_inner(
    account: &mut Value,
    hero_id: u64,
    skill_id: i32,
    now: u32,
    force: bool,
) -> Option<Vec<u8>> {
    if !force {
        let end_time = account
            .get("study")
            .and_then(|v| v.get("progress"))
            .and_then(Value::as_array)
            .and_then(|rows| {
                rows.iter().find(|row| {
                    json_u64(row, "heroId") == Some(hero_id)
                        && json_i32(row, "pSkillId") == Some(skill_id)
                })
            })
            .and_then(|row| json_i64(row, "endTime"))?;
        if end_time > i64::from(now) {
            return None;
        }
    }
    let row = {
        let progress = account
            .get_mut("study")?
            .get_mut("progress")?
            .as_array_mut()?;
        let index = progress.iter().position(|row| {
            json_u64(row, "heroId") == Some(hero_id) && json_i32(row, "pSkillId") == Some(skill_id)
        })?;
        progress.remove(index)
    };
    let before = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .and_then(|heroes| {
            heroes.iter().find(|hero| {
                json_u64(hero, "heroId") == Some(hero_id)
                    && hero
                        .get("pSkills")
                        .and_then(Value::as_array)
                        .is_some_and(|skills| {
                            skills.iter().any(|s| {
                                json_i32(s, "pSkillId").or_else(|| json_i32(s, "pskillId"))
                                    == Some(skill_id)
                            })
                        })
            })
        })
        .and_then(|hero| hero.get("pSkills"))
        .and_then(Value::as_array)
        .and_then(|skills| {
            skills.iter().find(|s| {
                json_i32(s, "pSkillId").or_else(|| json_i32(s, "pskillId")) == Some(skill_id)
            })
        })
        .and_then(|s| {
            json_i32(s, "level")
                .or_else(|| json_i32(s, "pSkillLv"))
                .or_else(|| json_i32(s, "pskillLv"))
        })
        .unwrap_or(0);
    if !study_skill_state(account, hero_id, skill_id) {
        return None;
    }
    let after = before.saturating_add(1).max(1);
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, hero_id);
    append_varint_field(&mut out, 2, skill_id as u64);
    append_varint_field(&mut out, 3, before.max(0) as u64);
    append_varint_field(&mut out, 4, after as u64);
    append_varint_field(
        &mut out,
        5,
        json_i64(&row, "textbookId").unwrap_or_default().max(0) as u64,
    );
    let _ = now;
    Some(out)
}
