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
            let damage = account
                .activities
                .progress
                .get("boss\u{1f}damage:1")
                .copied()
                .unwrap_or_default();
            (account.character.uid, damage, account)
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
    for (index, (uid, damage, _)) in entries.iter().take(50).enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_boss_payload_contains_damage() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("boss").unwrap(),
            "Boss",
        );
        account
            .activities
            .progress
            .insert("boss\u{1f}damage:1".to_owned(), 77);
        let payload = typed_boss_payload(&account);
        let bosses = decode_repeated_message_field(&payload, 2);
        assert_eq!(decode_varint_field(&bosses[0], 6), 77);
    }
}
