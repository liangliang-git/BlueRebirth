use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handles(method: &str) -> bool {
    GameMethod::parse(method).is_family(MethodFamily::GuildTask)
}

pub(super) fn handles_typed(method: &str) -> bool {
    handles(method)
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let task_id = match method {
        "guildtask.GuildTaskAccept" | "guildtask.GuildTaskFinish" | "guildtask.Donate" => {
            decode_varint_field(request_args, 2)
        }
        _ => decode_varint_field(request_args, 1),
    };
    match method {
        "guildtask.UpdateGuildTaskData" => reply(method, guild_task_data_payload_typed(account)),
        "guildtask.UpdateGuildTaskUserData" => {
            reply(method, guild_task_user_payload_typed(account))
        }
        "guildtask.AcceptTask" | "guildtask.GuildTaskAccept" => {
            if task_id <= 0 {
                return invalid("guild task id is invalid");
            }
            account
                .activities
                .progress
                .insert(format!("guildTask:accepted:{task_id}"), 1);
            account
                .activities
                .progress
                .insert("guildTask:lastTaskId".to_owned(), task_id as u64);
            HandlerResult::PushOnly
        }
        "guildtask.GuildTaskFinish" => {
            if task_id <= 0 {
                return invalid("guild task id is invalid");
            }
            account
                .activities
                .progress
                .insert(format!("guildTask:finished:{task_id}"), 1);
            account
                .activities
                .progress
                .insert("guildTask:lastTaskId".to_owned(), task_id as u64);
            HandlerResult::PushOnly
        }
        "guildtask.Donate" => handle_typed_donate(account, request_args, task_id, pre_pushes),
        "guildtask.DrawTaskReward" | "guildtask.ConstantRewardPoolGetReward" => {
            HandlerResult::Error(GameError::InvalidRequest(
                "guild task reward requires typed reward catalog",
            ))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "guild task method is unsupported",
        )),
    }
}

fn handle_typed_donate(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
    task_id: i32,
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let contribute = decode_varint_field(request_args, 4);
    let items = decode_repeated_message_field(request_args, 3)
        .into_iter()
        .filter_map(|item| {
            let goods_type = decode_varint_field(&item, 1);
            let item_id = decode_varint_field(&item, 2);
            let amount = decode_varint_field(&item, 3);
            (goods_type > 0 && item_id > 0 && amount > 0).then_some((goods_type, item_id, amount))
        })
        .collect::<Vec<_>>();
    if task_id <= 0 || contribute <= 0 || items.is_empty() {
        return invalid("guild task donation is invalid");
    }
    if items.iter().any(|(goods_type, item_id, amount)| {
        !typed_donation_available(account, *goods_type, *item_id, *amount)
    }) {
        return invalid("guild task donation items are insufficient");
    }
    for (goods_type, item_id, amount) in items {
        typed_donation_consume(account, goods_type, item_id, amount);
    }
    let progress = account
        .activities
        .progress
        .entry("guildTask:contribute".to_owned())
        .or_default();
    *progress = progress.saturating_add(u64::try_from(contribute).unwrap_or_default());
    account
        .activities
        .progress
        .insert("guildTask:lastTaskId".to_owned(), task_id as u64);
    pre_pushes.push(BagInfoCodec::encode(&bag_info_from_typed_account(account)));
    HandlerResult::PushOnly
}

fn typed_donation_available(
    account: &blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if goods_type == 5 {
        return typed_currency_for_guild_task(item_id)
            .is_some_and(|currency| account.resources.amount(currency).get() >= amount);
    }
    if goods_type == 1 || goods_type == 6 {
        return blueoath_domain::TemplateId::new(item_id as u64)
            .ok()
            .and_then(|id| account.inventory.items.get(&id).copied())
            .is_some_and(|current| current >= amount);
    }
    false
}

fn typed_donation_consume(
    account: &mut blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) {
    let Ok(amount) = u64::try_from(amount) else {
        return;
    };
    if goods_type == 5 {
        if let Some(currency) = typed_currency_for_guild_task(item_id) {
            let _ = account.resources.debit(currency, amount);
        }
    } else if let Ok(template_id) = blueoath_domain::TemplateId::new(item_id as u64) {
        if let Some(current) = account.inventory.items.get_mut(&template_id) {
            *current = current.saturating_sub(amount);
        }
    }
}

fn typed_currency_for_guild_task(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn guild_task_user_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let progress = &account.activities.progress;
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        progress
            .get("guildTask:contribute")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        2,
        progress
            .get("guildTask:todayAcceptCount")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        3,
        progress
            .get("guildTask:todayFinishStepCount")
            .copied()
            .unwrap_or_default(),
    );
    if let Some(task_id) = progress.get("guildTask:lastTaskId").copied() {
        let mut task = Vec::new();
        append_varint_field(&mut task, 1, 1);
        append_varint_field(&mut task, 2, task_id);
        append_varint_field(&mut task, 3, 1);
        append_message_field(&mut output, 4, &task);
    }
    output
}

fn guild_task_data_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let accepted = account
        .activities
        .progress
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("guildTask:accepted:")
                .and_then(|task_id| task_id.parse::<u64>().ok().map(|id| (id, *value)))
        })
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for (index, (task_id, _)) in accepted.iter().enumerate() {
        let mut task = Vec::new();
        append_varint_field(&mut task, 1, (index as u64).saturating_add(1));
        append_varint_field(&mut task, 2, *task_id);
        append_varint_field(&mut task, 3, 0);
        append_varint_field(&mut task, 4, 1);
        append_varint_field(&mut task, 5, 0);
        append_message_field(&mut output, 1, &task);
    }
    append_varint_field(
        &mut output,
        2,
        account
            .activities
            .progress
            .get("guildTask:todayAcceptCount")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        3,
        account
            .activities
            .progress
            .get("guildTask:todayFinishStepCount")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(&mut output, 4, u64::from(accepted.is_empty()));
    append_varint_field(&mut output, 6, 1);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_guild_task_uses_activity_progress_and_typed_inventory() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("guild-task-typed").unwrap(),
            "Captain",
        );
        let item = blueoath_domain::TemplateId::new(100).unwrap();
        account.inventory.items.insert(item, 5);
        let mut pushes = Vec::new();
        let mut accept = Vec::new();
        append_varint_field(&mut accept, 1, 7);
        assert!(matches!(
            handle_typed(&mut account, "guildtask.AcceptTask", &accept, &mut pushes),
            HandlerResult::PushOnly
        ));
        assert!(account
            .activities
            .progress
            .contains_key("guildTask:accepted:7"));

        let mut donate_item = Vec::new();
        append_varint_field(&mut donate_item, 1, 1);
        append_varint_field(&mut donate_item, 2, 100);
        append_varint_field(&mut donate_item, 3, 2);
        let mut donate = Vec::new();
        append_varint_field(&mut donate, 2, 7);
        append_message_field(&mut donate, 3, &donate_item);
        append_varint_field(&mut donate, 4, 3);
        assert!(matches!(
            handle_typed(&mut account, "guildtask.Donate", &donate, &mut pushes),
            HandlerResult::PushOnly
        ));
        assert_eq!(account.inventory.items.get(&item), Some(&3));
        assert_eq!(
            account.activities.progress.get("guildTask:contribute"),
            Some(&3)
        );
    }
}
