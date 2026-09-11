pub(crate) fn load_shop_catalog(catalog_path: Option<&Path>) -> ShopCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShopCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let valid_goods = read_config_rows_or_empty(&dir.join("config_shop_goods.db"))
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::HashSet<_>>();
    if valid_goods.is_empty() {
        return ShopCatalog::default();
    }
    let mut goods_by_shop = std::collections::BTreeMap::new();
    let mut costs_by_good_id = std::collections::BTreeMap::new();
    for (good_id, value) in read_config_rows_or_empty(&dir.join("config_shop_goods.db")) {
        let costs = shop_costs_from_value(&value);
        if !costs.is_empty() {
            costs_by_good_id.insert(good_id, costs);
        }
    }
    for (shop_id, value) in read_config_rows_or_empty(&dir.join("config_shop.db")) {
        if shop_id <= 0 {
            continue;
        }
        let mut goods = value_array_i64(&value, &["shelf_list", "shelfList"])
            .into_iter()
            .filter_map(|id| i32::try_from(id).ok())
            .filter(|id| valid_goods.contains(id))
            .collect::<Vec<_>>();
        goods.sort_unstable();
        goods.dedup();
        goods_by_shop.insert(shop_id, goods);
    }
    ShopCatalog {
        goods_by_shop,
        goods_by_id: std::collections::BTreeMap::new(),
        costs_by_good_id,
    }
}

pub(crate) fn shop_costs_from_value(value: &Value) -> Vec<ShopCost> {
    let Some(currencies) = value.get("currency").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(prices) = value.get("price").and_then(Value::as_array) else {
        return Vec::new();
    };
    currencies
        .iter()
        .zip(prices)
        .filter_map(|(currency, price)| {
            let currency = currency.as_array()?;
            let goods_type = i32::try_from(currency.first()?.as_i64()?).ok()?;
            let item_id = i32::try_from(currency.get(1)?.as_i64()?).ok()?;
            let amount = price.as_array()?.first()?.as_i64()?;
            (goods_type > 0 && item_id > 0 && amount > 0).then_some(ShopCost {
                goods_type,
                item_id,
                amount,
            })
        })
        .collect()
}

pub(crate) fn load_server_shop_goods(
    catalog: &mut ShopCatalog,
    database_path: Option<&Path>,
) -> Result<(), String> {
    // Server-local database is authoritative. Never retain client shelf IDs.
    catalog.goods_by_shop.clear();
    catalog.goods_by_id.clear();
    let database_path = database_path
        .ok_or_else(|| "server config database path is not configured for shop goods".to_owned())?;
    let goods = config_db::load_server_shop_goods(database_path).map_err(|error| {
        format!(
            "load server shop goods from {}: {error}",
            database_path.display()
        )
    })?;
    for good in goods {
        catalog
            .goods_by_shop
            .entry(good.shop_id)
            .or_default()
            .push(good.good_id);
        catalog.goods_by_id.insert(
            good.good_id,
            ShopGood {
                shop_id: good.shop_id,
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: good.num,
                costs: good
                    .costs
                    .into_iter()
                    .map(|cost| ShopCost {
                        goods_type: cost.goods_type,
                        item_id: cost.item_id,
                        amount: cost.amount,
                    })
                    .collect(),
            },
        );
    }
    normalize_server_shop_goods(catalog);
    Ok(())
}

fn normalize_server_shop_goods(catalog: &mut ShopCatalog) {
    for goods in catalog.goods_by_shop.values_mut() {
        goods.sort_unstable();
        goods.dedup();
    }
}

pub(crate) fn load_reward_definitions(dir: &Path) -> std::collections::BTreeMap<i32, Vec<ShopReward>> {
    read_config_rows_or_empty(&dir.join("config_rewards.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let rows = value.get("rewards")?.as_array()?;
            let rewards = rows
                .iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    if row.len() < 3 {
                        return None;
                    }
                    let goods_type = i32::try_from(row[0].as_i64()?).ok()?;
                    let item_id = i32::try_from(row[1].as_i64()?).ok()?;
                    let num = i32::try_from(row[2].as_i64()?).ok()?;
                    (goods_type > 0 && item_id > 0 && num > 0).then_some(ShopReward {
                        goods_type,
                        item_id,
                        num,
                        instance_id: 0,
                    })
                })
                .collect::<Vec<_>>();
            Some((id, rewards))
        })
        .collect()
}
