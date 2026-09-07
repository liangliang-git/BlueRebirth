use serde_json::{json, Value};

use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    method: &str,
) -> HandlerResult {
    match method {
        "boss.GetBossUserDamageRankList" => HandlerResult::Reply(Response::raw(
            method,
            typed_user_rank_payload(state, account),
        )),
        "boss.GetBossGuildDamageRankList" => {
            HandlerResult::Reply(Response::raw(method, typed_guild_rank_payload(account)))
        }
        "boss.GetBossData" | "boss.UpdateBossData" => {
            HandlerResult::Reply(Response::raw(method, typed_boss_payload(account)))
        }
        _ => HandlerResult::Empty,
    }
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    _request_args: &[u8],
) -> HandlerResult {
    let state = context.state;
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    match method {
        "boss.GetBossData" | "boss.UpdateBossData" => reply(method, boss_payload(account)),
        "boss.GetBossUserDamageRankList" => reply(method, user_rank_payload(state, account)),
        "boss.GetBossGuildDamageRankList" => reply(method, guild_rank_payload(account)),
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn typed_boss_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let damage = account
        .activities
        .progress
        .get("boss\u{1f}damage:1")
        .copied()
        .unwrap_or_default();
    let mut boss = Vec::new();
    append_varint_field(&mut boss, 1, 1);
    append_varint_field(&mut boss, 2, 1_000_000_000);
    append_varint_field(&mut boss, 6, damage);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 0);
    append_message_field(&mut output, 2, &boss);
    append_varint_field(&mut output, 3, 1);
    output
}

fn typed_user_rank_payload(
    state: &ServerState,
    current: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let mut entries = state
        .social_store
        .as_ref()
        .and_then(|store| store.list_typed_accounts().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|account| {
            (
                account.character.uid,
                account
                    .activities
                    .progress
                    .get("boss\u{1f}damage:1")
                    .copied()
                    .unwrap_or_default(),
                account,
            )
        })
        .collect::<Vec<_>>();
    let current_damage = current
        .activities
        .progress
        .get("boss\u{1f}damage:1")
        .copied()
        .unwrap_or_default();
    if let Some(existing) = entries
        .iter_mut()
        .find(|(uid, _, _)| *uid == current.character.uid)
    {
        existing.1 = current_damage;
        existing.2 = current.clone();
    } else {
        entries.push((current.character.uid, current_damage, current.clone()));
    }
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut output = Vec::new();
    for (index, (uid, damage, _account)) in entries.iter().take(50).enumerate() {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, *uid);
        append_varint_field(&mut row, 2, index.saturating_add(1) as u64);
        append_message_field(
            &mut row,
            3,
            &super::base_handler::other_user_payload_typed(state, current, *uid),
        );
        append_varint_field(&mut row, 4, *damage);
        append_message_field(&mut output, 1, &row);
    }
    let current_rank = entries
        .iter()
        .position(|(uid, _, _)| *uid == current.character.uid)
        .map(|rank| rank.saturating_add(1))
        .unwrap_or(1);
    let mut current_row = Vec::new();
    append_varint_field(&mut current_row, 1, current.character.uid);
    append_varint_field(&mut current_row, 2, current_rank as u64);
    append_message_field(
        &mut current_row,
        3,
        &super::base_handler::other_user_payload_typed(state, current, current.character.uid),
    );
    append_varint_field(&mut current_row, 4, current_damage);
    append_message_field(&mut output, 2, &current_row);
    output
}

fn typed_guild_rank_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let points = account
        .activities
        .progress
        .iter()
        .filter(|(key, _)| key.starts_with("guildBigActivity\u{1f}points:"))
        .map(|(_, points)| *points)
        .sum::<u64>();
    let mut rank = Vec::new();
    append_varint_field(&mut rank, 1, 0);
    append_bytes_field(&mut rank, 2, b"BlueOath");
    append_bytes_field(&mut rank, 3, b"");
    append_varint_field(&mut rank, 4, 1);
    append_varint_field(&mut rank, 5, points);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &rank);
    append_message_field(&mut output, 2, &rank);
    output
}

fn boss_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("boss".to_owned())
        .or_insert_with(|| {
            json!({
                "attackCount": 0,
                "bosses": [{
                    "bossId": 1,
                    "hp": 1_000_000_000i64,
                    "count": 0,
                    "unlockTime": 0,
                    "activeTime": 0,
                    "damage": 0
                }]
            })
        })
}

fn boss_payload(account: &mut Value) -> Vec<u8> {
    let boss = boss_state_mut(account);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(boss, "attackCount").unwrap_or_default().max(0) as u64,
    );
    for entry in boss
        .get("bosses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(entry, "bossId").unwrap_or(1).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(entry, "hp").unwrap_or(0).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(entry, "count").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            4,
            json_i64(entry, "unlockTime").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            5,
            json_i64(entry, "activeTime").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    append_varint_field(&mut output, 3, 1);
    output
}

fn user_rank_payload(state: &ServerState, account: &mut Value) -> Vec<u8> {
    let mut entries = account_directory(state, account);
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut output = Vec::new();
    for (index, (uid, entry)) in entries.iter().take(50).enumerate() {
        append_message_field(
            &mut output,
            1,
            &boss_rank_row(state, account, *uid, *entry, index.saturating_add(1)),
        );
    }
    let uid = account_uid(account);
    let current_rank = entries
        .iter()
        .position(|(entry_uid, _)| *entry_uid == uid)
        .map(|rank| rank.saturating_add(1))
        .unwrap_or(1);
    append_message_field(
        &mut output,
        2,
        &boss_rank_row(state, account, uid, boss_damage(account), current_rank),
    );
    output
}

fn account_directory(state: &ServerState, current: &Value) -> Vec<(u64, u64)> {
    let mut entries = state
        .social_store
        .as_ref()
        .and_then(|store| store.list_typed_accounts().ok())
        .into_iter()
        .flatten()
        .map(|account| {
            let uid = account.character.uid;
            let damage = account
                .activities
                .progress
                .get("boss\u{1f}damage:1")
                .copied()
                .unwrap_or_default();
            (uid, damage)
        })
        .collect::<Vec<_>>();
    let uid = account_uid(current);
    let damage = boss_damage(current);
    if let Some(existing) = entries.iter_mut().find(|(entry_uid, _)| *entry_uid == uid) {
        existing.1 = damage;
    } else {
        entries.push((uid, damage));
    }
    entries
}

fn account_uid(account: &Value) -> u64 {
    account
        .get("character")
        .and_then(|character| json_u64(character, "uid"))
        .filter(|uid| *uid > 0)
        .unwrap_or(1)
}

fn boss_damage(account: &Value) -> u64 {
    account
        .get("boss")
        .and_then(|boss| boss.get("bosses"))
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .and_then(|entry| json_i64(entry, "damage"))
        .unwrap_or_default()
        .max(0) as u64
}

fn boss_rank_row(
    state: &ServerState,
    current: &Value,
    uid: u64,
    damage: u64,
    rank: usize,
) -> Vec<u8> {
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, uid);
    append_varint_field(&mut row, 2, rank.max(1) as u64);
    append_message_field(
        &mut row,
        3,
        &super::base_handler::other_user_payload(state, current, uid),
    );
    append_varint_field(&mut row, 4, damage);
    row
}

fn guild_rank_payload(account: &Value) -> Vec<u8> {
    let guild = account.get("guild").unwrap_or(&Value::Null);
    let mut rank = Vec::new();
    append_varint_field(&mut rank, 1, json_u64(guild, "guildId").unwrap_or(0));
    append_bytes_field(
        &mut rank,
        2,
        json_string(guild, "name").unwrap_or_default().as_bytes(),
    );
    append_bytes_field(&mut rank, 3, b"");
    append_varint_field(&mut rank, 4, 1);
    append_varint_field(&mut rank, 5, 0);
    append_varint_field(&mut rank, 6, 0);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &rank);
    append_message_field(&mut output, 2, &rank);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_payload_contains_attack_count_and_boss_row() {
        let mut account = json!({
            "boss": {"attackCount": 3, "bosses": [{"bossId": 8, "hp": 900, "count": 2}]}
        });
        let payload = boss_payload(&mut account);
        assert_eq!(decode_varint_field(&payload, 1), 3);
        let bosses = decode_repeated_message_field(&payload, 2);
        assert_eq!(decode_varint_field(&bosses[0], 1), 8);
        assert_eq!(decode_varint_u64_field(&bosses[0], 2), 900);
    }

    #[test]
    fn user_damage_rank_aggregates_saved_accounts() {
        let root = std::env::temp_dir().join(format!(
            "blueoath-boss-rank-{}-{}",
            std::process::id(),
            current_unix_millis()
        ));
        let store = blueoath_storage::ProfileStore::open(&root).unwrap();
        let mut lower = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("lower").unwrap(),
            "Lower",
        );
        lower.character.uid = 1;
        lower
            .activities
            .progress
            .insert("boss\u{1f}damage:1".to_owned(), 100);
        store.save_typed_account(&mut lower).unwrap();
        let mut higher = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("higher").unwrap(),
            "Higher",
        );
        higher.character.uid = 2;
        higher
            .activities
            .progress
            .insert("boss\u{1f}damage:1".to_owned(), 200);
        store.save_typed_account(&mut higher).unwrap();
        let mut server_state = ServerState::new("local", "Local", "1.4.0");
        server_state.social_store = Some(store.clone());
        let mut current = json!({
            "character": {"uid": 1, "name": "Lower"},
            "boss": {"bosses": [{"damage": 100}]}
        });

        let payload = user_rank_payload(&server_state, &mut current);
        let rows = decode_repeated_message_field(&payload, 1);
        assert_eq!(rows.len(), 2);
        assert_eq!(decode_varint_u64_field(&rows[0], 1), 2);
        assert_eq!(decode_varint_field(&rows[0], 2), 1);
        assert_eq!(decode_varint_u64_field(&rows[1], 1), 1);
        assert_eq!(decode_varint_field(&rows[1], 2), 2);
        assert_eq!(
            decode_varint_field(&decode_repeated_message_field(&payload, 2)[0], 2),
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
