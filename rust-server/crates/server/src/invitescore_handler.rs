use serde_json::{json, Value};

use super::common::response::HandlerResult;
use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "invitescore.SetInviteStateByType" => {
            let payload = {
                let Some(account) = context.account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                let invite = invite_state_mut(account);
                match decode_varint_field(request_args, 1) {
                    1 => invite["haveGotSSR"] = json!(1),
                    2 => invite["haveGotFaishon"] = json!(1),
                    3 => invite["haveFirstBattleWin"] = json!(1),
                    _ => {}
                }
                invite_payload(account)
            };
            append_invite_refresh(context, payload);
            HandlerResult::PushOnly
        }
        "invitescore.CheckAndResetInviteState" => {
            let payload = {
                let Some(account) = context.account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                let invite = invite_state_mut(account);
                invite["recordInviteScoreVersion"] = json!(decode_varint_field(request_args, 1));
                invite_payload(account)
            };
            append_invite_refresh(context, payload);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn invite_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("inviteScore".to_owned())
        .or_insert_with(|| {
            json!({
                "haveGotSSR": 0,
                "haveGotFaishon": 0,
                "haveFirstBattleWin": 0,
                "recordInviteScoreVersion": 0,
            })
        })
}

fn invite_payload(account: &Value) -> Vec<u8> {
    let invite = account.get("inviteScore").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    for (field, key) in [
        (1, "haveGotSSR"),
        (2, "haveGotFaishon"),
        (3, "haveFirstBattleWin"),
        (4, "recordInviteScoreVersion"),
    ] {
        append_varint_field(
            &mut output,
            field,
            invite
                .get(key)
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0) as u64,
        );
    }
    output
}

fn append_invite_refresh<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    payload: Vec<u8>,
) {
    append_method_push(
        context.post_pushes,
        "invitescore.RefreshInviteScore",
        payload,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_payload_encodes_all_flags() {
        let account = json!({
            "inviteScore": {
                "haveGotSSR": 1,
                "haveGotFaishon": 1,
                "haveFirstBattleWin": 1,
                "recordInviteScoreVersion": 7
            }
        });
        assert_eq!(invite_payload(&account), vec![8, 1, 16, 1, 24, 1, 32, 7]);
    }
}
