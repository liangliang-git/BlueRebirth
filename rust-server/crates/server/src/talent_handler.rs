use serde_json::Value;

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let account = &mut *context.account;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

    match method {
        "talentTree.TalentTreeAllList" => {
            let catalog = TALENT_CATALOG.get().cloned().unwrap_or_default();
            Some(talent_tree_payload(
                account_view.unwrap_or(&Value::Null),
                &catalog,
            ))
        }
        "talentTree.GetTalentData" => {
            let catalog = TALENT_CATALOG.get().cloned().unwrap_or_default();
            let talent_id = decode_talent_id(request_args);
            let node = catalog.nodes.get(&talent_id);
            let mut ret = Vec::new();
            if let Some(node) = node {
                let root = if node.belong_talent > 0 {
                    node.belong_talent
                } else {
                    talent_id
                };
                let reached = talent_state(account_view.unwrap_or(&Value::Null))
                    .get(&root)
                    .copied();
                let operate = reached == Some(talent_id);
                append_message_field(
                    &mut ret,
                    1,
                    &encode_talent_data(talent_id, &node.precondition, i32::from(operate)),
                );
            }
            Some(ret)
        }
        "talentTree.UnLockTalent" | "talentTree.UpgradeTalent" => {
            let catalog = TALENT_CATALOG.get().cloned().unwrap_or_default();
            let talent_id = decode_talent_id(request_args);
            if let Some(account) = account.as_deref_mut() {
                match apply_talent_change(account, &catalog, talent_id) {
                    Ok(target) => append_method_push(
                        pre_pushes,
                        "talentTree.TalentChange",
                        talent_change_payload(target),
                    ),
                    Err(error) => {
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                    }
                }
            }
            Some(Vec::new())
        }
        _ => None,
    }
}
