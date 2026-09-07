use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = current_talent_catalog();
    match method {
        "talentTree.TalentTreeAllList" => {
            reply(method, talent_tree_payload_typed(account, &catalog))
        }
        "talentTree.GetTalentData" => reply(
            method,
            talent_data_payload_typed(
                account,
                &catalog,
                TalentIdRequest::decode(request_args)
                    .map(|request| request.talent_id)
                    .unwrap_or_default(),
            ),
        ),
        "talentTree.UnLockTalent" | "talentTree.UpgradeTalent" => {
            let Ok(request) = TalentIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "talent request is invalid",
                ));
            };
            let talent_id = request.talent_id;
            match apply_talent_change_typed(account, &catalog, talent_id) {
                Ok(target) => effects.push_pre(Response::raw(
                    "talentTree.TalentChange",
                    talent_change_payload(target),
                )),
                Err(error) => return HandlerResult::Error(GameError::InvalidRequest(error)),
            }
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_handler_exposes_domain_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("talent").unwrap(),
            "Talent",
        );
        let mut effects = ResponseEffects::default();
        assert!(matches!(
            handle_typed(&mut account, "unknown", &[], &mut effects),
            HandlerResult::Empty
        ));
    }
}
