use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    _request_args: &[u8],
) -> Option<Vec<u8>> {
    let account = context.account.as_deref_mut()?;
    let adventure = adventure_state_mut(account);
    match method {
        "adventure.GetAdventure" => Some(adventure_payload(adventure)),
        "adventure.LevelUp" => {
            for role in adventure
                .get_mut("roles")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let level = json_i64(role, "level").unwrap_or(1).clamp(1, 99) + 1;
                role["level"] = json!(level);
                role["hp"] = json!(level.saturating_mul(1_000));
            }
            Some(adventure_payload(adventure))
        }
        "adventure.Attack" => {
            let enemy_index = json_i64(adventure, "enemyIndex").unwrap_or(0).max(0);
            let mut is_killed = false;
            if let Some(enemy) = adventure
                .get_mut("enemies")
                .and_then(Value::as_array_mut)
                .and_then(|enemies| {
                    enemies
                        .iter_mut()
                        .find(|enemy| json_i64(enemy, "index") == Some(enemy_index))
                })
            {
                let damage = json_i64(enemy, "damage").unwrap_or(0).saturating_add(100);
                enemy["damage"] = json!(damage);
                is_killed = damage >= 1_000;
            }
            if is_killed {
                adventure["enemyIndex"] = json!(enemy_index.saturating_add(1));
            }
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, 100);
            append_varint_field(&mut output, 2, u64::from(is_killed));
            Some(output)
        }
        _ => None,
    }
}

fn adventure_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("adventure".to_owned())
        .or_insert_with(|| {
            json!({
                "roles": [
                    {"roleId": 1, "level": 1, "hp": 1000},
                    {"roleId": 2, "level": 1, "hp": 1000},
                    {"roleId": 3, "level": 1, "hp": 1000}
                ],
                "enemies": [
                    {"index": 0, "damage": 0},
                    {"index": 1, "damage": 0},
                    {"index": 2, "damage": 0}
                ],
                "enemyIndex": 0
            })
        })
}

fn adventure_payload(adventure: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for role in adventure
        .get("roles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(role, "roleId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(role, "level").unwrap_or(1).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(role, "hp").unwrap_or(1_000).max(0) as u64,
        );
        append_message_field(&mut output, 1, &encoded);
    }
    for enemy in adventure
        .get("enemies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(enemy, "index").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(enemy, "damage").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    append_varint_field(
        &mut output,
        3,
        json_i64(adventure, "enemyIndex").unwrap_or_default().max(0) as u64,
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adventure_payload_contains_roles_enemies_and_index() {
        let adventure = json!({
            "roles": [{"roleId": 2, "level": 4, "hp": 4000}],
            "enemies": [{"index": 1, "damage": 20}],
            "enemyIndex": 1
        });
        let payload = adventure_payload(&adventure);
        let roles = decode_repeated_message_field(&payload, 1);
        let enemies = decode_repeated_message_field(&payload, 2);
        assert_eq!(decode_varint_field(&roles[0], 1), 2);
        assert_eq!(decode_varint_field(&roles[0], 2), 4);
        assert_eq!(decode_varint_field(&enemies[0], 2), 20);
        assert_eq!(decode_varint_field(&payload, 3), 1);
    }
}
