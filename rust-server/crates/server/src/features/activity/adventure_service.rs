use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{AccountState, AdventureState};

pub(crate) fn handle_typed(account: &mut AccountState, method: &str) -> HandlerResult {
    match method {
        "adventure.GetAdventure" => reply(method, typed_adventure_payload(&account.adventure)),
        "adventure.LevelUp" => {
            for role in &mut account.adventure.roles {
                role.level = role.level.clamp(1, 99).saturating_add(1);
                role.hp = role.level.saturating_mul(1_000);
            }
            reply(method, typed_adventure_payload(&account.adventure))
        }
        "adventure.Attack" => {
            let enemy_index = account.adventure.enemy_index;
            let mut is_killed = false;
            if let Some(enemy) = account
                .adventure
                .enemies
                .iter_mut()
                .find(|enemy| enemy.index == enemy_index)
            {
                enemy.damage = enemy.damage.saturating_add(100);
                is_killed = enemy.damage >= 1_000;
            }
            if is_killed {
                account.adventure.enemy_index = enemy_index.saturating_add(1);
            }
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, 100);
            append_varint_field(&mut output, 2, u64::from(is_killed));
            reply(method, output)
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn typed_adventure_payload(adventure: &AdventureState) -> Vec<u8> {
    let mut output = Vec::new();
    for role in &adventure.roles {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, role.role_id);
        append_varint_field(&mut encoded, 2, role.level);
        append_varint_field(&mut encoded, 3, role.hp);
        append_message_field(&mut output, 1, &encoded);
    }
    for enemy in &adventure.enemies {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, enemy.index);
        append_varint_field(&mut encoded, 2, enemy.damage);
        append_message_field(&mut output, 2, &encoded);
    }
    append_varint_field(&mut output, 3, adventure.enemy_index);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_adventure_level_up_and_attack_update_domain_state() {
        let mut account = AccountState::default();

        let result = handle_typed(&mut account, "adventure.LevelUp");
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.adventure.roles[0].level, 2);
        assert_eq!(account.adventure.roles[0].hp, 2_000);

        for _ in 0..10 {
            let _ = handle_typed(&mut account, "adventure.Attack");
        }
        assert_eq!(account.adventure.enemies[0].damage, 1_000);
        assert_eq!(account.adventure.enemy_index, 1);
    }
}
