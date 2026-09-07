use super::common::response::HandlerResult;
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "invitescore.SetInviteStateByType" => {
            let request = match InviteStateTypeRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "invite state type is invalid",
                    ));
                }
            };
            match request.state_type {
                1 => account.invite_score.have_got_ssr = 1,
                2 => account.invite_score.have_got_fashion = 1,
                3 => account.invite_score.have_first_battle_win = 1,
                _ => {}
            }
            append_method_push(
                post_pushes,
                "invitescore.RefreshInviteScore",
                invite_payload_typed(account),
            );
            HandlerResult::PushOnly
        }
        "invitescore.CheckAndResetInviteState" => {
            let request = match InviteRecordVersionRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "invite state version is invalid",
                    ));
                }
            };
            account.invite_score.record_version = request.version;
            append_method_push(
                post_pushes,
                "invitescore.RefreshInviteScore",
                invite_payload_typed(account),
            );
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn invite_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let invite = &account.invite_score;
    let mut output = Vec::new();
    for (field, value) in [
        (1, invite.have_got_ssr),
        (2, invite.have_got_fashion),
        (3, invite.have_first_battle_win),
        (4, invite.record_version),
    ] {
        append_varint_field(&mut output, field, value);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_invite_score_updates_domain_state_and_pushes_refresh() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("invite").unwrap(),
            "Invite",
        );
        let mut pushes = Vec::new();
        assert!(matches!(
            handle_typed(
                &mut account,
                "invitescore.SetInviteStateByType",
                &[8, 1],
                &mut pushes,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(account.invite_score.have_got_ssr, 1);
        assert_eq!(pushes.len(), 1);
    }
}
