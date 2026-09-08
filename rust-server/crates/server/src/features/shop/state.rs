#[cfg(test)]
use serde_json::Value;

use crate::common::response::Response;

use super::*;

#[cfg(test)]
pub(crate) fn apply_mail_reward(account: &mut Value, mail: &MailTemplate) {
    if mail.goods_type == 5 {
        if let Some(key) = currency_character_key(mail.config_id) {
            add_character_i64(account, key, mail.num);
        }
    } else {
        add_bag_item(account, mail.config_id, mail.num);
    }
}

pub(crate) fn apply_typed_mail_reward(
    account: &mut blueoath_domain::AccountState,
    mail: &MailTemplate,
) -> Option<ShopReward> {
    let reward = ShopReward {
        goods_type: mail.goods_type,
        item_id: mail.config_id,
        num: mail.num,
        instance_id: 0,
    };
    let typed_reward = ShopReward {
        goods_type: if mail.goods_type == 5 { 5 } else { 1 },
        ..reward
    };
    if !task_state::can_grant_typed_task_reward(account, &typed_reward)
        || !task_state::grant_typed_task_reward(account, &typed_reward)
    {
        return None;
    }
    Some(reward)
}

pub(crate) fn encode_mail_list_response(
    mails: &[MailTemplate],
    now: u32,
    rewards: &[ShopReward],
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, mails.len() as u64);
    for mail in mails {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, mail.mid);
        append_varint_field(&mut item, 2, 0);
        append_varint_field(&mut item, 7, u64::from(now));
        append_varint_field(&mut item, 8, 0);
        append_bytes_field(&mut item, 9, mail.subject.as_bytes());
        append_bytes_field(&mut item, 10, mail.content.as_bytes());
        append_varint_field(&mut item, 11, 0);
        let mut attachment = Vec::new();
        append_varint_field(&mut attachment, 1, mail.goods_type.max(0) as u64);
        append_varint_field(&mut attachment, 2, mail.config_id.max(0) as u64);
        append_varint_field(&mut attachment, 3, mail.num.max(0) as u64);
        append_message_field(&mut item, 13, &attachment);
        append_varint_field(&mut item, 14, 0);
        append_message_field(&mut output, 3, &item);
    }
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut item, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 4, &item);
    }
    output
}

pub(crate) fn return_shop_buy_response(
    good_id: i32,
    buy_num: i32,
    reward: Option<ShopReward>,
) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(reward) = reward {
        let mut nested = Vec::new();
        append_varint_field(&mut nested, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut nested, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut nested, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut nested, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 1, &nested);
    }
    append_varint_field(&mut output, 2, good_id.max(0) as u64);
    append_varint_field(&mut output, 3, buy_num.max(0) as u64);
    output
}

pub(crate) fn encode_quality_buy_goods_response(
    rewards: &[ShopReward],
    good_ids: &[i32],
) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut nested = Vec::new();
        append_varint_field(&mut nested, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut nested, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut nested, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut nested, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 1, &nested);
    }
    for good_id in good_ids {
        append_varint_field(&mut output, 2, (*good_id).max(0) as u64);
    }
    output
}

pub(crate) trait ResponsePushBuffer {
    fn push_response(&mut self, response: Response);
}

impl ResponsePushBuffer for Vec<Response> {
    fn push_response(&mut self, response: Response) {
        self.push(response);
    }
}

impl ResponsePushBuffer for Vec<Vec<u8>> {
    fn push_response(&mut self, response: Response) {
        self.push(response.encode_push(current_unix_seconds()));
    }
}

pub(crate) fn append_method_push<P: ResponsePushBuffer>(
    pushes: &mut P,
    method: &str,
    ret: Vec<u8>,
) {
    pushes.push_response(Response::raw(method, ret));
}

#[allow(dead_code)]
#[cfg(test)]
pub(crate) fn apply_shop_good(
    account: &mut Value,
    good: &ShopGood,
    buy_num: i32,
    now: u32,
    fashion_catalog: Option<&FashionList>,
) -> Option<ShopReward> {
    if good.goods_type == 5 && currency_character_key(good.item_id).is_none() {
        return None;
    }
    if !deduct_shop_costs(account, &good.costs, buy_num) {
        return None;
    }
    let total = good.num.max(1).saturating_mul(buy_num.max(1));
    match good.goods_type {
        2 => {
            let mut last_id = 0;
            for _ in 0..total {
                last_id = add_equip_item(account, good.item_id);
            }
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: last_id,
            })
        }
        3 => Some(ShopReward {
            goods_type: good.goods_type,
            item_id: good.item_id,
            num: 1,
            instance_id: add_ship_items(account, good.item_id, total, now),
        }),
        5 => {
            let key = currency_character_key(good.item_id)?;
            add_character_i64(account, key, total);
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
        18 => {
            add_fashion_item(account, good.item_id, fashion_catalog);
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
        _ => {
            add_bag_item(account, good.item_id, total);
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
    }
}

#[allow(dead_code)]
#[cfg(test)]
pub(crate) fn deduct_shop_costs(account: &mut Value, costs: &[ShopCost], buy_num: i32) -> bool {
    let buy_num = i64::from(buy_num.max(1));
    let mut totals = std::collections::BTreeMap::<(i32, i32), i64>::new();
    for cost in costs {
        let Some(amount) = cost.amount.checked_mul(buy_num) else {
            return false;
        };
        let entry = totals.entry((cost.goods_type, cost.item_id)).or_default();
        let Some(total) = entry.checked_add(amount) else {
            return false;
        };
        *entry = total;
    }
    for (&(goods_type, item_id), &amount) in &totals {
        if amount <= 0 {
            return false;
        }
        if goods_type == 5 {
            if currency_character_key(item_id)
                .map(|key| character_i64(account, key) < amount)
                .unwrap_or(true)
            {
                return false;
            }
        } else if i32::try_from(amount).is_err() || bag_item_count(account, item_id) < amount {
            return false;
        }
    }
    for (&(goods_type, item_id), &amount) in &totals {
        if goods_type == 5 {
            let Some(key) = currency_character_key(item_id) else {
                return false;
            };
            adjust_character_i64(account, key, -amount);
        } else if let Ok(amount) = i32::try_from(amount) {
            consume_bag_item(account, item_id, amount);
        }
    }
    true
}

pub(crate) fn shop_info_payload(catalog: Option<&ShopCatalog>) -> Vec<u8> {
    // RetShopsInfo.ShopInfo: all configured shop ids must exist. The client indexes this
    // table from homepage red-dot logic before it sends shop.GetShopsInfo.
    const SHOP_IDS: &[u32] = &[
        1, 3, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 27, 29, 30,
        101, 102, 104, 105, 106, 107, 110, 111, 200, 201, 202, 205, 206, 207, 208, 300, 302, 303,
        305, 306, 401, 901, 902, 903, 911, 912, 913, 914, 915, 916, 917, 918, 919, 920, 924, 930,
        931, 934, 935, 936, 940, 950, 951, 954, 955, 956, 957, 958, 1001, 1002, 1003, 1004, 1006,
        1010, 1011, 1012, 1013, 1014, 1015, 1020, 1021, 1022, 1023, 1024, 1025, 1026, 1030, 1040,
        1041, 1042, 1043, 1044, 1051, 1052, 1071, 1072, 1073, 1074, 1201, 1202,
    ];
    let mut payload = Vec::new();
    // GM goods are server authority. Client config shelf_list contains historical
    // entries (805/10000/...) and must not replace current GM goods IDs.
    let server_goods = catalog.filter(|catalog| !catalog.goods_by_id.is_empty());
    let configured = catalog.filter(|catalog| !catalog.goods_by_shop.is_empty());
    let shop_ids = if server_goods.is_some() {
        SHOP_IDS.iter().map(|id| *id as i32).collect::<Vec<_>>()
    } else {
        configured
            .map(|catalog| catalog.goods_by_shop.keys().copied().collect::<Vec<_>>())
            .unwrap_or_else(|| SHOP_IDS.iter().map(|id| *id as i32).collect())
    };
    for shop_id in shop_ids {
        let mut shop = Vec::new();
        append_varint_field(&mut shop, 1, shop_id.max(0) as u64);
        let goods = server_goods
            .and_then(|catalog| catalog.goods_by_shop.get(&shop_id))
            .or_else(|| configured.and_then(|catalog| catalog.goods_by_shop.get(&shop_id)))
            .cloned()
            .unwrap_or_default();
        if goods.is_empty() {
            // Empty ShopGoodsData list still needs one element; Lua assumes non-nil table.
            append_message_field(&mut shop, 3, &[]);
        } else {
            for goods_id in goods {
                let mut item = Vec::new();
                append_varint_field(&mut item, 1, goods_id.max(0) as u64);
                append_varint_field(&mut item, 2, 0);
                append_varint_field(&mut item, 3, 0);
                append_message_field(&mut shop, 3, &item);
            }
        }
        append_varint_field(&mut shop, 4, 0);
        append_varint_field(&mut shop, 5, 0);
        append_varint_field(&mut shop, 6, 0);
        payload.push(0x0A);
        append_varint(&mut payload, shop.len() as u64);
        payload.extend_from_slice(&shop);
    }
    payload
}

pub(crate) fn shop_refresh_payload(
    catalog: Option<&ShopCatalog>,
    shop_id: i32,
) -> Result<Vec<u8>, &'static str> {
    let goods = catalog
        .and_then(|value| value.goods_by_shop.get(&shop_id))
        .cloned()
        .unwrap_or_default();
    if goods.is_empty() {
        return Err("shop was not found");
    }
    let mut payload = Vec::new();
    append_varint_field(&mut payload, 1, shop_id.max(0) as u64);
    append_varint_field(&mut payload, 2, 0);
    for goods_id in goods {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, goods_id.max(0) as u64);
        append_varint_field(&mut item, 2, 0);
        append_varint_field(&mut item, 3, 0);
        append_message_field(&mut payload, 3, &item);
    }
    append_varint_field(&mut payload, 4, 0);
    append_varint_field(&mut payload, 5, 0);
    append_varint_field(&mut payload, 6, 0);
    Ok(payload)
}

#[cfg(test)]
mod response_push_tests {
    use super::{append_method_push, Response};
    use blueoath_protocol::TMessageCodec;

    #[test]
    fn typed_push_buffer_defers_wire_encoding() {
        let mut responses = Vec::<Response>::new();
        append_method_push(&mut responses, "bag.UpdateBagData", vec![1, 2, 3]);
        assert_eq!(responses[0].method, "bag.UpdateBagData");
        assert_eq!(responses[0].payload, vec![1, 2, 3]);
    }

    #[test]
    fn legacy_push_buffer_still_encodes_compatibility_bytes() {
        let mut pushes = Vec::<Vec<u8>>::new();
        append_method_push(&mut pushes, "bag.UpdateBagData", vec![1, 2, 3]);
        let response = TMessageCodec::decode_response(&pushes[0]).expect("encoded push");
        assert_eq!(response.method, "bag.UpdateBagData");
        assert_eq!(response.ret, Some(vec![1, 2, 3]));
    }
}
