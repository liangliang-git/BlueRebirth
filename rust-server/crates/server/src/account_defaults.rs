use serde_json::{json, Value};

use super::*;

pub(super) fn default_account_snapshot(profile_id: &str, name: &str, now: u32) -> Value {
    let reset_day = (i64::from(now) + 8 * 60 * 60) / 86_400;
    let reset_week = (reset_day + 3) / 7;

    json!({
        "profileId": profile_id,
        "character": {
            "uid": 1,
            "name": name,
            "level": 1,
            "class": 1,
            "secretaryId": 1,
            "createTime": now,
            "bath": 0,
            "gold": 0,
            "diamond": 10_000,
            "supply": INITIAL_SUPPLY,
            "pvePt": 0,
            "headFrame": 0,
            "mainGun": 0,
            "torpedo": 0,
            "plane": 0,
            "other": 0,
            "retire": 0,
            "strategy": 0,
            "medal": 0,
            "tower": 0,
            "copyTrainPoint": 0,
            "fashionPoint": 0,
            "guildContri": 0,
            "lucky": 0,
            "teacherMedal": 0,
            "teacherPrestige": 0,
            "battlePassExp": 0,
            "battlePassGold": 0,
            "guildCoinII": 0,
            "urEquipCoin": 0,
            "activityBattlePassExp": 0,
            "getHeroCount": 0,
            "attackCount": 0,
            "marriedNum": 0,
            "achievePoint": 0,
            "exp": 0,
            "message": "",
            "head": 1021051,
            "plotChapterId": 1,
            "seaDifficulty": 1
        },
        "seaProgress": {"records": []},
        "copyProgress": {"records": []},
        "copyRecords": [],
        "goodsCopy": {"entries": []},
        "dailyCopy": {
            "resetDay": reset_day,
            "chapters": [],
            "groups": [],
            "extraGroups": []
        },
        "tasks": {
            "records": [],
            "dailyResetDay": reset_day,
            "weeklyResetWeek": reset_week,
            "teachingPtRewardIds": []
        },
        "construction": {"jobs": [], "nextSequence": 1, "lastProject": null},
        "tower": {
            "chapterId": 30001,
            "areaIndex": 0,
            "copyIndex": 0,
            "topicIndex": 0,
            "dailyCount": 0,
            "dailyCountEx": 0,
            "resetTime": now,
            "heroIds": [],
            "lockEquipList": [],
            "savePassCopyId": []
        },
        "dock": {
            "heroes": [{
                "heroId": 1,
                "templateId": 10210511,
                "level": 1,
                "fashioning": 1021051,
                "exp": 0,
                "createTime": now,
                "updateTime": now,
                "moodUpdateTime": now,
                "affection": 500000,
                "marryTime": 0,
                "mood": MOOD_INITIAL,
                "marryType": 0,
                "curHp": 10000000000i64,
                "equipSlots": [1, 0, 2, 0, 0, 0],
                "equipStates": [0, 0, 0, 0, 0, 0],
                "pSkills": [],
                "intensify": [],
                "equipSlotsByType": {},
                "equipStatesByType": {},
                "combinationInfo": {"comLv": 0, "comGrade": 0, "combine": 0, "beCombined": 0},
                "name": "",
                "changeNameTime": 0,
                "lock": true,
                "advance": 0,
                "advLv": 0,
                "remouldLevel": 0
            }],
            "bagSize": 200
        },
        "bag": {"items": [
            {"templateId": 60000, "num": 1000},
            {"templateId": 60001, "num": 1000},
            {"templateId": 60002, "num": 1000},
            {"templateId": 60003, "num": 1000},
            {"templateId": 10182, "num": 100},
            {"templateId": 10185, "num": 100},
            {"templateId": 10187, "num": 100},
            {"templateId": 10007, "num": 100},
            {"templateId": 10181, "num": 100},
            {"templateId": 12201, "num": 100},
            {"templateId": 10029, "num": 1000},
            {"templateId": 10030, "num": 1000},
            {"templateId": 10031, "num": 100}
        ], "bagSize": 100},
        "fashion": {"entries": []},
        "equip": {
            "items": [
                {"equipId": 1, "templateId": 30091, "enhanceLv": 0, "star": 0, "heroId": 1, "enhanceExp": 0},
                {"equipId": 2, "templateId": 30221, "enhanceLv": 0, "star": 0, "heroId": 1, "enhanceExp": 0}
            ],
            "equipBagSize": 2000
        },
        "equipTestCopy": {"maxDamage": 0, "receivedRewards": []},
        "equipNewTestCopy": {"infos": []},
        "equipActivity": {"infos": []},
        "bath": {"heroList": [], "isAllAuto": 0},
        "study": {"progress": []},
        "illustrate": {"entries": [], "equipEntries": [], "vowHeroIds": []},
        "medals": [],
        "sweep": {"entries": []},
        "shiptask": {
            "currentShipTid": 0,
            "currentHeroTemplateId": 0,
            "setShipTime": 0,
            "tasks": [],
            "achievements": [],
            "extraMvp": []
        },
        "buildState": {"drawCount": {}, "usedBoxInfo": {}, "usedRewardInfo": {}},
        "talent": {"activeTalents": {}},
        "battlePass": {
            "passType": 1,
            "passLevel": 1,
            "passExp": 0,
            "curWeekIndex": 1,
            "claimedRewards": [],
            "claimedTasks": [],
            "tasks": [],
            "refreshCount": 0
        },
        "activityBattlePass": {
            "passType": 1,
            "passLevel": 1,
            "passExp": 0,
            "curWeekIndex": 1,
            "claimedRewards": [],
            "claimedTasks": [],
            "tasks": [],
            "refreshCount": 0
        },
        "battlePassClaimed": [],
        "activityBattlePassClaimed": [],
        "copyRewardCounts": {},
        "copyStarRewards": [],
        "miniGameScores": [],
        "rechargePurchases": {},
        "rechargeTotal": 0,
        "exchangeTimes": {},
        "foodCompose": {"recipes": {}, "lastRecipeId": 0},
        "worldEventProgress": 0,
        "worldEventUserProgress": 0,
        "worldEventClaimedStagesByEvent": {},
        "sign": {"days": []},
        "friend": {"friends": [], "blackList": [], "applyList": [], "applyRecordList": []},
        "chat": {"channel": 0, "messages": [], "barrages": []},
        "adventure": {
            "roles": [
                {"roleId": 1, "level": 1, "hp": 1000},
                {"roleId": 2, "level": 1, "hp": 1000},
                {"roleId": 3, "level": 1, "hp": 1000}
            ],
            "enemies": [
                {"index": 0, "damage": 0},
                {"index": 1, "damage": 0},
                {"index": 2, "damage": 0}
            ],
            "enemyIndex": 0
        },
        "outpost": {
            "buildings": [{
                "id": 1,
                "level": 0,
                "heroList": [],
                "state": 1,
                "useCoin": false,
                "itemInfo": [],
                "tacticList": []
            }],
            "speedUpTime": 0
        },
        "sportsMeet": {"tickCount": 10, "freeCounts": [], "points": 0, "receivedPoints": []},
        "fleet": {
            "tactics": [
                {"modeId": 1, "type": 1, "tacticName": "", "strategyId": 0, "formationId": 2},
                {"modeId": 2, "type": 1, "tacticName": "", "strategyId": 0, "formationId": 2},
                {"modeId": 3, "type": 1, "tacticName": "", "strategyId": 0, "formationId": 2},
                {"modeId": 4, "type": 1, "tacticName": "", "strategyId": 0, "formationId": 2},
                {"modeId": 5, "type": 1, "tacticName": "", "strategyId": 0, "formationId": 2}
            ],
            "maxPower": 0,
            "minPower": 0,
            "isSkip": false
        },
        "presetFleet": {"presetfleet": [], "NameNum": 0, "redDot": 0},
        "strategy": {"list": [], "curCost": 0, "resetNum": 0},
        "support": {"items": []},
        "milestone": {"claimed": []},
        "guide": {"plotRewards": [], "settings": {}},
        "jopen": {"fetchHeroTime": 0, "fetchEquipTime": 0},
        "building": {
            "buildings": [
                {"id": 1, "tid": 2, "level": 2, "heroIds": [], "status": 1, "lastUpdateTime": now, "lastBuildUpdateTime": now},
                {"id": 2, "tid": 41, "level": 1, "heroIds": [], "status": 1, "lastUpdateTime": now, "lastBuildUpdateTime": now}
            ],
            "lands": [
                {"index": 1, "buildingId": 1},
                {"index": 6, "buildingId": 2}
            ],
            "workerStrength": 1500000,
            "workerRecover": 10,
            "foodMax": 100,
            "electricMax": 100
        },
        "profileDisplayName": name
    })
}

pub(super) fn user_info_from_account(state: &ServerState, account: Option<&Value>) -> UserInfo {
    let fallback = UserInfo {
        uid: 1,
        uname: state.name.clone(),
        level: state.level,
        class_id: 1,
        secretary_id: 1,
        supply: state.fuel.clamp(0, i64::from(i32::MAX)) as i32,
        gold: state.coins.clamp(0, i64::from(i32::MAX)) as i32,
        head: 1021051,
        pve_pt: 100,
        ..UserInfo::default()
    };
    let Some(character) = account.and_then(|value| value.get("character")) else {
        return fallback;
    };
    UserInfo {
        uid: json_u64(character, "uid").unwrap_or(fallback.uid),
        uname: json_string(character, "name").unwrap_or(fallback.uname),
        level: json_i32(character, "level").unwrap_or_default(),
        class_id: json_i32(character, "class").unwrap_or_default(),
        secretary_id: json_u64(character, "secretaryId")
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default(),
        create_time: json_i32(character, "createTime")
            .filter(|value| *value != 0)
            .unwrap_or_else(|| current_unix_seconds() as i32),
        gold: json_i32(character, "gold").unwrap_or_default(),
        diamond: json_i32(character, "diamond").unwrap_or_default(),
        supply: json_i32(character, "supply").unwrap_or_default(),
        pve_pt: json_i32(character, "pvePt").unwrap_or_default(),
        head: json_i32(character, "head").unwrap_or(1021051),
        head_frame: json_i32(character, "headFrame").unwrap_or_default(),
        bath: json_i32(character, "bath").unwrap_or_default(),
        main_gun: json_i32(character, "mainGun").unwrap_or_default(),
        torpedo: json_i32(character, "torpedo").unwrap_or_default(),
        plane: json_i32(character, "plane").unwrap_or_default(),
        other: json_i32(character, "other").unwrap_or_default(),
        retire: json_i32(character, "retire").unwrap_or_default(),
        strategy: json_i32(character, "strategy").unwrap_or_default(),
        medal: json_i32(character, "medal").unwrap_or_default(),
        tower: json_i32(character, "tower").unwrap_or_default(),
        copy_train_point: json_i32(character, "copyTrainPoint").unwrap_or_default(),
        fashion_point: json_i32(character, "fashionPoint").unwrap_or_default(),
        guild_contri: json_i32(character, "guildContri").unwrap_or_default(),
        lucky: json_i32(character, "lucky").unwrap_or_default(),
        teacher_medal: json_i32(character, "teacherMedal").unwrap_or_default(),
        teacher_prestige: json_i32(character, "teacherPrestige").unwrap_or_default(),
        battle_pass_exp: json_i32(character, "battlePassExp").unwrap_or_default(),
        battle_pass_gold: json_i32(character, "battlePassGold").unwrap_or_default(),
        guild_coin_ii: json_i32(character, "guildCoinII").unwrap_or_default(),
        ur_equip_coin: json_i32(character, "urEquipCoin").unwrap_or_default(),
        activity_battle_pass_exp: json_i32(character, "activityBattlePassExp").unwrap_or_default(),
        get_hero_count: json_i32(character, "getHeroCount").unwrap_or_default(),
        attack_count: json_i32(character, "attackCount").unwrap_or_default(),
        married_num: json_i32(character, "marriedNum").unwrap_or_default(),
        achieve_point: json_i32(character, "achievePoint").unwrap_or_default(),
        message: json_string(character, "message").unwrap_or_default(),
        medal_acquired_times: account
            .and_then(|value| value.get("medals"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|medal| {
                Some(MedalAcquiredTime {
                    medal_id: json_i32(medal, "medalId").or_else(|| json_i32(medal, "medal_id"))?,
                    time: json_i32(medal, "time").or_else(|| json_i32(medal, "acquiredTime"))?,
                })
            })
            .filter(|medal| medal.medal_id > 0 && medal.time > 0)
            .collect(),
        exp: json_i32(character, "exp").unwrap_or_default(),
        ..UserInfo::default()
    }
}

pub(super) fn user_info_from_typed_account(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
) -> UserInfo {
    let character = &account.character;
    let resource = |kind| i32::try_from(account.resources.amount(kind).get()).unwrap_or(i32::MAX);
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
