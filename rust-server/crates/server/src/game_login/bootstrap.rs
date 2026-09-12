use super::*;

#[cfg(not(test))]
pub(crate) async fn write_typed_bootstrap_push<S>(
    stream: &mut S,
    trace_methods: bool,
    method: &'static str,
    payload: Vec<u8>,
    now: u32,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if trace_methods {
        tracing::debug!(method, push_bytes = payload.len(), "game-login push");
    }
    Ok(
        NetSocketFrameCodec::write(stream, 0, &Response::new(method, payload).encode_push(now))
            .await?,
    )
}

#[cfg(not(test))]
pub(crate) async fn write_typed_user_info_bootstrap<S>(
    stream: &mut S,
    state: &ServerState,
    account: &AccountState,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: _equip_catalog,
        shop: shop_catalog,
        handbook_behaviours,
        chapters: chapter_catalog,
        tasks: task_catalog,
        buildings: building_catalog,
        ..
    } = *catalogs;
    let now = current_unix_seconds();

    let mut login_time = Vec::new();
    append_varint_field(&mut login_time, 1, u64::from(now));
    append_varint_field(&mut login_time, 2, u64::from(now.saturating_sub(3600)));
    write_typed_bootstrap_push(
        stream,
        state.trace_methods,
        "user.UpdateLoginTime",
        login_time,
        now,
    )
    .await?;

    let mut server_time = Vec::new();
    append_varint_field(&mut server_time, 1, u64::from(now));
    append_varint_field(&mut server_time, 2, u64::from(now));
    write_typed_bootstrap_push(
        stream,
        state.trace_methods,
        "user.UpdateSvrTime",
        server_time,
        now,
    )
    .await?;

    if state.trace_methods {
        let secretary_id = account
            .character
            .secretary_id
            .map(|id| id.get())
            .unwrap_or(1);
        let secretary_fashioning = account
            .dock
            .heroes
            .get(&blueoath_domain::HeroId::new(secretary_id).expect("secretary id is positive"))
            .map(|hero| hero.fashioning);
        tracing::debug!(
            heroes = account.dock.heroes.len(),
            secretary_id,
            secretary_fashioning = ?secretary_fashioning,
            "game-login hero bootstrap"
        );
    }

    macro_rules! write_payload {
        ($method:expr, $payload:expr $(,)?) => {
            write_typed_bootstrap_push(stream, state.trace_methods, $method, $payload, now).await?;
        };
    }

    write_payload!(
        "user.GetUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    );

    for (method, payload) in [
        (
            "build.BuildsInfo",
            building_handler::typed_construction_info_payload(account, now),
        ),
        (
            "bathroom.BathroomInfo",
            progression_handler::bathroom_info_payload_from_typed(account),
        ),
        (
            "study.GetStudyInfo",
            progression_handler::study_info_payload_from_typed(account, now),
        ),
        (
            "task.TaskInfo",
            task_info_payload_from_typed_account(account, task_catalog),
        ),
    ] {
        write_payload!(method, payload);
    }

    write_payload!(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    );
    write_payload!(
        "fashion.updateData",
        FashionListCodec::encode(&fashion_list_from_typed_account(account, fashion_catalog)),
    );
    write_payload!(
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_typed_account(account)),
    );
    write_payload!(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    );
    write_payload!(
        "building.UpdateBuildingInfo",
        UserBuildingInfoCodec::encode(&building_info_from_typed_account_with_catalog(
            account,
            now,
            building_catalog,
        )),
    );
    write_payload!(
        "tactic.GetHerosTactic",
        FleetInfoCodec::encode(&fleet_info_from_typed_account(account)),
    );

    write_payload!("shop.UpdateShopInfo", shop_info_payload(shop_catalog),);
    write_payload!("recharge.RechargeInfo", vec![0x1A, 0x00]);
    write_payload!(
        "buildship.BuildShipInfo",
        buildship_info_payload_from_typed(account, now),
    );
    write_payload!(
        "presetfleet.PresetFleetsInfo",
        PresetFleetCodec::encode(&preset_fleet_info_from_typed_account(account)),
    );

    let template_ids = account
        .dock
        .heroes
        .values()
        .map(|hero| hero.template_id.get() as i32)
        .collect::<Vec<_>>();
    for (method, payload) in [
        (
            "illustrate.IllustrateInfo",
            illustrate_info_payload_for_templates(&template_ids, handbook_behaviours),
        ),
        ("illustrate.OldIllustrateInfo", Vec::new()),
        (
            "illustrate.Memory",
            story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
        ),
    ] {
        write_payload!(method, payload);
    }

    let talent_catalog = current_talent_catalog();
    write_payload!(
        "talentTree.TalentTreeAllList",
        talent_tree_payload_typed(account, &talent_catalog),
    );
    Ok(())
}

pub(crate) fn append_typed_user_login_bootstrap(
    effects: &mut ResponseEffects,
    state: &ServerState,
    account: &AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
    _battle_catalog: Option<&BattleCatalog>,
) {
    let fallback_catalog;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback_catalog = ChapterCatalog::fallback();
            &fallback_catalog
        }
    };
    let now = current_unix_seconds();
    effects.push_pre(super::common::response::Response::raw(
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "guide.GuideInfo",
        GuideInfoCodec::encode_progress(&account.guide.settings),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "strategy.GetStrategy",
        base_handler::typed_strategy_info_payload(account),
    ));
    // Match original client/server contract: route selection lives in the
    // client's PlayerPrefs. Login only supplies copy data, in fixed order.
    for copy_type in [1, 2, 33, 9, 10] {
        effects.push_pre(super::common::response::Response::raw(
            "copy.GetCopy",
            CopyInfoCodec::encode_payload(&copy_info_payload(catalog, copy_type, account)),
        ));
    }
    effects.push_pre(super::common::response::Response::raw(
        "dailycopy.UpdateDailyCopyData",
        daily_copy_snapshot_payload_from_typed_account(account, chapter_catalog, now),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.IllustrateInfo",
        Vec::new(),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.OldIllustrateInfo",
        Vec::new(),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.Memory",
        story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
    ));
}
