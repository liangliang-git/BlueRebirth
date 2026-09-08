use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(crate) fn handles_typed(method: &str) -> bool {
    matches!(
        method,
        "archiveCopy.IsLoad"
            | "copyextra.AddCopyRewardCount"
            | "copyextra.UpdateCopyExtraInfo"
            | "prefs.SavePrefs"
            | "statcount.GetStatCount"
            | "sign.Sign"
            | "miniGame.StartMiniGame"
            | "alchemy.StartAlchemy"
    )
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "copyextra.AddCopyRewardCount" => {
            let Ok(request) = CopyRewardCountRequest::decode(request_args) else {
                return invalid("copy reward request is invalid");
            };
            let chapter_id = request.chapter_id;
            let reward_time = request.reward_time.max(1);
            let key = format!("copyExtraRewardCount:{chapter_id}");
            let total = account.activities.progress.entry(key).or_default();
            *total = total.saturating_add(u64::try_from(reward_time).unwrap_or_default());
            reply(method, copy_reward_times_payload(chapter_id, *total as i64))
        }
        "copyextra.UpdateCopyExtraInfo" => {
            let mut output = Vec::new();
            for (key, value) in &account.activities.progress {
                let Some(chapter_id) = key.strip_prefix("copyExtraRewardCount:") else {
                    continue;
                };
                let Ok(chapter_id) = chapter_id.parse::<i32>() else {
                    continue;
                };
                append_message_field(
                    &mut output,
                    1,
                    &copy_reward_times_payload(chapter_id, *value as i64),
                );
            }
            reply(method, output)
        }
        "sign.Sign" => {
            let Ok(request) = SignDayRequest::decode(request_args) else {
                return invalid("sign request is invalid");
            };
            let day = request.day.max(1);
            account.activities.progress.insert(format!("sign:{day}"), 1);
            HandlerResult::PushOnly
        }
        "alchemy.StartAlchemy" => {
            let Ok(request) = AlchemyRequest::decode(request_args) else {
                return invalid("alchemy request is invalid");
            };
            if request.equip_ids.iter().any(|id| {
                !account
                    .dock
                    .equipments
                    .keys()
                    .any(|equip_id| equip_id.get() == *id)
            }) {
                return invalid("alchemy request is invalid");
            }
            HandlerResult::PushOnly
        }
        "archiveCopy.IsLoad" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return invalid("archive copy id is invalid");
            };
            let copy_id = request.copy_id;
            account
                .activities
                .progress
                .insert("archiveCopy:copyId".to_owned(), copy_id as u64);
            account.activities.progress.insert(
                "archiveCopy:loadedAt".to_owned(),
                u64::from(current_unix_seconds()),
            );
            HandlerResult::PushOnly
        }
        "prefs.SavePrefs" | "statcount.GetStatCount" | "miniGame.StartMiniGame" => {
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn copy_reward_times_payload(chapter_id: i32, reward_time: i64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, chapter_id.max(0) as u64);
    append_varint_field(&mut output, 2, reward_time.max(0) as u64);
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn typed_copy_extra_state_uses_activity_progress() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("misc-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 77);
        append_varint_field(&mut args, 2, 4);
        assert!(matches!(
            handle_typed(&mut account, "copyextra.AddCopyRewardCount", &args),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.activities.progress.get("copyExtraRewardCount:77"),
            Some(&4)
        );
        let HandlerResult::Reply(response) =
            handle_typed(&mut account, "copyextra.UpdateCopyExtraInfo", &[])
        else {
            panic!("expected typed copy extra response");
        };
        let payload = response.payload.into_bytes();
        assert_eq!(decode_repeated_message_field(&payload, 1).len(), 1);
        let mut archive = Vec::new();
        append_varint_field(&mut archive, 1, 91);
        assert!(matches!(
            handle_typed(&mut account, "archiveCopy.IsLoad", &archive),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.activities.progress.get("archiveCopy:copyId"),
            Some(&91)
        );
    }
}
