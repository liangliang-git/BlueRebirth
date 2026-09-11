use super::*;

pub(crate) fn user_info_from_typed_account(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
) -> UserInfo {
    let character = &account.character;
    let resource = |kind| i32::try_from(account.resources.amount(kind).get()).unwrap_or(i32::MAX);
    let compat = |item_id: i32| {
        i32::try_from(
            account
                .activities
                .progress
                .get(&format!("compat:currency:{item_id}"))
                .copied()
                .unwrap_or_default(),
        )
        .unwrap_or(i32::MAX)
    };
    let medal_acquired_times = account
        .activities
        .progress
        .iter()
        .filter_map(|(key, value)| {
            let medal_id = key.strip_prefix("compat:medal:")?.parse::<i32>().ok()?;
            let time = i32::try_from(*value).ok()?;
            (medal_id > 0 && time > 0).then_some(MedalAcquiredTime { medal_id, time })
        })
        .collect();
    UserInfo {
        uid: character.uid,
        uname: if character.name.is_empty() {
            state.name.clone()
        } else {
            character.name.clone()
        },
        level: i32::try_from(character.level).unwrap_or(i32::MAX),
        class_id: i32::try_from(character.class_id).unwrap_or(i32::MAX),
        secretary_id: character
            .secretary_id
            .map(|id| u32::try_from(id.get()).unwrap_or(u32::MAX))
            .unwrap_or(1),
        create_time: if character.create_time == 0 {
            current_unix_seconds() as i32
        } else {
            i32::try_from(character.create_time).unwrap_or(i32::MAX)
        },
        gold: resource(blueoath_domain::CurrencyKind::Gold),
        diamond: resource(blueoath_domain::CurrencyKind::Diamond),
        supply: resource(blueoath_domain::CurrencyKind::Supply),
        pve_pt: resource(blueoath_domain::CurrencyKind::PvePoint),
        bath: compat(13),
        main_gun: compat(8),
        torpedo: compat(9),
        plane: compat(10),
        other: compat(11),
        retire: compat(12),
        strategy: compat(14),
        medal: compat(15),
        tower: compat(18),
        copy_train_point: compat(22),
        fashion_point: compat(23),
        guild_contri: compat(24),
        lucky: compat(25),
        teacher_medal: compat(26),
        teacher_prestige: compat(27),
        battle_pass_exp: compat(28),
        battle_pass_gold: compat(29),
        guild_coin_ii: compat(31),
        ur_equip_coin: compat(32),
        activity_battle_pass_exp: compat(33),
        medal_acquired_times,
        head: if character.head == 0 {
            1021051
        } else {
            i32::try_from(character.head).unwrap_or(i32::MAX)
        },
        head_frame: i32::try_from(character.head_frame).unwrap_or(i32::MAX),
        exp: i32::try_from(character.exp).unwrap_or(i32::MAX),
        message: character.message.clone(),
        ..UserInfo::default()
    }
}
