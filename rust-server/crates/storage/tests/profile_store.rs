use blueoath_domain::{
    AccountRepository, AccountState, BattleSession, ChapterId, ChatBarrageState, ChatMessageState,
    CopyId, EquipId, EquipmentState, FleetId, FleetRecord, HeroId, HeroState, NewAccountFactory,
    PresetFleetState, ProfileId, ProfileState, TemplateId,
};
use blueoath_storage::{ProfileStore, StorageError, StoredProfileState, StoredShip};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

fn store() -> (ProfileStore, std::path::PathBuf) {
    let suffix = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "blueoath-rust-test-{}-{suffix}",
        std::process::id()
    ));
    (ProfileStore::open(&root).unwrap(), root)
}

fn profile_state(coins: i64) -> StoredProfileState {
    StoredProfileState {
        level: 1,
        fuel: 100,
        coins,
        ..StoredProfileState::default()
    }
}

#[test]
fn profiles_are_upserted_and_isolated() {
    let (store, root) = store();
    store.save("one", "One", &profile_state(10)).unwrap();
    store.save("two", "Two", &profile_state(20)).unwrap();

    assert_eq!(store.load("one").unwrap().unwrap().state.coins, 10);
    assert_eq!(store.load("two").unwrap().unwrap().state.coins, 20);
    assert_eq!(store.list().unwrap(), vec!["one", "two"]);

    store.save("one", "Renamed", &profile_state(11)).unwrap();
    let renamed = store.load("one").unwrap().unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.state.coins, 11);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reset_removes_only_selected_profile() {
    let (store, root) = store();
    store
        .save("one", "One", &StoredProfileState::default())
        .unwrap();
    store
        .save("two", "Two", &StoredProfileState::default())
        .unwrap();
    store.reset("one").unwrap();

    assert!(store.load("one").unwrap().is_none());
    assert!(store.load("two").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_profile_id_is_rejected_before_database_write() {
    let (store, root) = store();
    let error = store
        .save("bad/id", "Bad", &StoredProfileState::default())
        .unwrap_err();

    assert!(matches!(error, StorageError::InvalidProfileId));
    assert!(store.list().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dot_profile_id_is_accepted_like_csharp_server() {
    let (store, root) = store();
    store
        .save("jp.v1", "JP", &StoredProfileState::default())
        .unwrap();
    assert!(store.load("jp.v1").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn updated_timestamp_uses_iso8601_utc_format() {
    let (store, root) = store();
    store
        .save("one", "One", &StoredProfileState::default())
        .unwrap();

    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let timestamp: String = connection
        .query_row(
            "SELECT updated_utc FROM profiles WHERE id = 'one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(timestamp.contains('T') && timestamp.ends_with('Z'));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn normalized_profile_runtime_round_trips_without_json_state_column() {
    let (store, root) = store();
    let state = StoredProfileState {
        level: 4,
        fuel: 80,
        coins: 125,
        ships: vec![StoredShip {
            id: 1001,
            name: "Starter".to_owned(),
            level: 3,
            power: 220,
        }],
        formation_ship_ids: vec![1001],
        completed_stages: 7,
    };
    store.save("typed-profile", "Captain", &state).unwrap();

    let loaded = store.load("typed-profile").unwrap().unwrap();
    assert_eq!(loaded.state, state);

    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let legacy_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('profiles') WHERE name = 'state_json'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_columns, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_social_relations_round_trip_through_normalized_storage() {
    let (store, root) = store();
    let profile_id = ProfileId::new("typed-social").unwrap();
    let mut account = NewAccountFactory::create(profile_id.clone(), "Captain");
    let chapter = ChapterId::new(1).unwrap();
    account.daily_copy.reset_day = 42;
    account.daily_copy.challenge_times.insert(chapter, 3);
    account.daily_copy.select_ex.insert(chapter, true);
    account.battle.passed_copies.insert(CopyId::new(7).unwrap());
    account.social.friends.insert(42);
    account.social.pending.insert(43);
    account.social.blacklist.insert(44);
    account.social.applied.insert(45);

    store.create(&account).unwrap();
    let loaded = store.load_typed_account(&profile_id).unwrap().unwrap();
    assert_eq!(loaded.social, account.social);
    assert_eq!(loaded.daily_copy, account.daily_copy);
    assert_eq!(loaded.battle.passed_copies, account.battle.passed_copies);

    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let relation_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM friend_relations WHERE profile_id = 'typed-social'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(relation_count, 4);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_tower_state_round_trips_through_normalized_storage() {
    let (store, root) = store();
    let profile_id = ProfileId::new("typed-tower").unwrap();
    let mut account = NewAccountFactory::create(profile_id.clone(), "Tower Captain");
    let hero_id = account.dock.heroes.keys().next().copied().unwrap();
    let equip_id = account.dock.equipments.keys().next().copied().unwrap();
    account.tower.chapter_id = 30_001;
    account.tower.area_index = 2;
    account.tower.copy_index = 3;
    account.tower.daily_count = 4;
    account.tower.sf_id_counts.insert(100, 2);
    account.tower.hero_ids.push(hero_id);
    account.tower.lock_equip_ids.push(equip_id);
    account
        .tower
        .save_pass_copy_ids
        .push(CopyId::new(9).unwrap());
    account
        .tower
        .pending_rewards
        .push(blueoath_domain::TowerRewardState {
            reward_type: 1,
            config_id: 2,
            amount: 3,
            instance_id: 4,
        });
    account.activity_tower.activity_id = 7;
    account.activity_tower.quick_number = 2;
    account
        .activity_tower
        .pass_copy_ids
        .push(CopyId::new(10).unwrap());
    account.activity_tower.hero_ids.push(hero_id);

    store.create(&account).unwrap();
    let loaded = store.load_typed_account(&profile_id).unwrap().unwrap();
    assert_eq!(loaded.tower, account.tower);
    assert_eq!(loaded.activity_tower, account.activity_tower);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_from_schema_v6_normalizes_profile_runtime_and_character_fields() {
    let suffix = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "blueoath-rust-migration-test-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let migrations = [
        include_str!("../../../migrations/0001_schema_meta.sql"),
        include_str!("../../../migrations/0002_profiles_accounts.sql"),
        include_str!("../../../migrations/0003_account_revisions.sql"),
        include_str!("../../../migrations/0004_core_account.sql"),
        include_str!("../../../migrations/0005_core_indexes.sql"),
        include_str!("../../../migrations/0006_progress_social_activity.sql"),
    ];
    connection.execute_batch(migrations[0]).unwrap();
    for (index, migration) in migrations.iter().enumerate().skip(1) {
        connection.execute_batch(migration).unwrap();
        connection
            .execute(
                "UPDATE schema_meta SET version = ?1, applied_at = 'now' WHERE id = 1",
                [i64::try_from(index + 1).unwrap()],
            )
            .unwrap();
    }
    drop(connection);

    let store = ProfileStore::open(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let version: i64 = connection
        .query_row("SELECT version FROM schema_meta WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    let state_json_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('profiles') WHERE name = 'state_json'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 18);
    let accounts_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'accounts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(accounts_table, 0);
    assert_eq!(state_json_columns, 0);
    for column in ["class_id", "create_time", "message"] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('characters') WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing characters.{column}");
    }
    assert!(store.list().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn opening_store_is_idempotent_and_records_schema_version() {
    let (store, root) = store();
    drop(store);
    let reopened = ProfileStore::open(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let version: i64 = connection
        .query_row("SELECT version FROM schema_meta WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(version >= 6);
    assert!(reopened.list().unwrap().is_empty());
    let accounts_table: i64 = rusqlite::Connection::open(root.join("profiles.db"))
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'accounts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(accounts_table, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_repository_transaction_commits_domain_mutation() {
    let (store, root) = store();
    let profile_id = ProfileId::new("typed").unwrap();
    let mut account = AccountState {
        profile: Some(ProfileState {
            id: profile_id.clone(),
            name: "Typed Captain".to_owned(),
            revision: 0,
        }),
        resources: Default::default(),
        ..AccountState::default()
    };
    account.character.class_id = 3;
    account.character.create_time = 123;
    account.character.message = "typed hello".to_owned();
    account.chat.channel = 2;
    account.chat.messages.push(ChatMessageState {
        id: 1,
        uid: 10,
        channel: 2,
        receive_uid: 20,
        message: "hello".to_owned(),
        message_type: 1,
        voice: "voice".to_owned(),
        sent_at: 99,
    });
    account.chat.barrages.push(ChatBarrageState {
        id: 7,
        offset: 1,
        content: "wave".to_owned(),
        uid: 10,
        sent_at: 100,
    });
    let hero_id = HeroId::new(10).unwrap();
    let equip_id = EquipId::new(20).unwrap();
    account.dock.heroes.insert(
        hero_id,
        HeroState {
            id: hero_id,
            template_id: TemplateId::new(100).unwrap(),
            name: String::new(),
            change_name_time: 0,
            level: 2,
            exp: 3,
            mood: 90,
            affection: 4,
            hp: 80,
            locked: true,
            equip_slots: vec![Some(equip_id)],
        },
    );
    account.dock.equipments.insert(
        equip_id,
        EquipmentState {
            id: equip_id,
            template_id: TemplateId::new(200).unwrap(),
            enhance_level: 1,
            star: 2,
            enhance_exp: 5,
            hero_id: Some(hero_id),
        },
    );
    account.fleet.fleets.insert(
        FleetId::new(1).unwrap(),
        FleetRecord {
            formation_id: 2,
            tactic_id: 3,
            members: vec![hero_id],
        },
    );
    account.fleet.preset_name_num = 4;
    account.fleet.preset_red_dot = 1;
    account.fleet.presets.push(PresetFleetState {
        name: "Stored preset".to_owned(),
        hero_ids: vec![hero_id],
        ex_hero_ids: Vec::new(),
        mode_id: 3,
        strategy_id: 17,
    });
    account.tasks.progress.insert(7, 8);
    account.tasks.task_types.insert(7, 5);
    account.tasks.completed.insert(7);
    account.tasks.claimed.insert(7);
    account.daily_copy.reset_day = 42;
    account
        .daily_copy
        .challenge_times
        .insert(ChapterId::new(3).unwrap(), 4);
    account.buildings.levels.insert(11, 6);
    account.buildings.template_ids.insert(11, 41);
    account.buildings.land_indices.insert(11, 6);
    account.buildings.hero_assignments.insert(11, vec![hero_id]);
    account
        .inventory
        .items
        .insert(TemplateId::new(30001).unwrap(), 17);
    account
        .activities
        .progress
        .insert("spring\u{1f}merits".to_owned(), 33);
    account.invite_score.have_got_ssr = 1;
    account.invite_score.have_got_fashion = 1;
    account.invite_score.have_first_battle_win = 1;
    account.invite_score.record_version = 7;
    account.talents.active.insert(10, 11);
    account.battle.active = Some(BattleSession {
        chapter_id: ChapterId::new(3).unwrap(),
        copy_id: CopyId::new(300).unwrap(),
        current_fleet: 1,
        started_at: 100,
        expires_at: 200,
        revision: 1,
        remaining_fleet_ids: vec![1, 2],
        hero_ids: vec![hero_id],
        attack_count: 3,
    });
    AccountRepository::create(&store, &account).unwrap();
    AccountRepository::transact(&store, &profile_id, |account| {
        account
            .resources
            .credit(blueoath_domain::CurrencyKind::Gold, 25)
    })
    .unwrap();

    let loaded = AccountRepository::load(&store, &profile_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded
            .resources
            .amount(blueoath_domain::CurrencyKind::Gold)
            .get(),
        25
    );
    assert_eq!(
        loaded.dock.heroes[&hero_id].equip_slots,
        vec![Some(equip_id)]
    );
    assert_eq!(loaded.dock.equipments[&equip_id].hero_id, Some(hero_id));
    assert_eq!(loaded.character.class_id, 3);
    assert_eq!(loaded.character.create_time, 123);
    assert_eq!(loaded.character.message, "typed hello");
    assert_eq!(loaded.chat.channel, 2);
    assert_eq!(loaded.chat.messages[0].message, "hello");
    assert_eq!(loaded.chat.messages[0].voice, "voice");
    assert_eq!(loaded.chat.barrages[0].content, "wave");
    assert_eq!(
        loaded.fleet.fleets[&FleetId::new(1).unwrap()].members,
        vec![hero_id]
    );
    assert_eq!(loaded.fleet.preset_name_num, 4);
    assert_eq!(loaded.fleet.preset_red_dot, 1);
    assert_eq!(loaded.fleet.presets[0].name, "Stored preset");
    assert_eq!(loaded.fleet.presets[0].hero_ids, vec![hero_id]);
    assert_eq!(loaded.tasks.progress.get(&7), Some(&8));
    assert_eq!(loaded.tasks.task_types.get(&7), Some(&5));
    assert!(loaded.tasks.completed.contains(&7));
    assert!(loaded.tasks.claimed.contains(&7));
    assert_eq!(loaded.daily_copy.reset_day, 42);
    assert_eq!(loaded.buildings.levels.get(&11), Some(&6));
    assert_eq!(loaded.buildings.template_ids.get(&11), Some(&41));
    assert_eq!(loaded.buildings.land_indices.get(&11), Some(&6));
    assert_eq!(
        loaded.buildings.hero_assignments.get(&11),
        Some(&vec![hero_id])
    );
    assert_eq!(
        loaded.inventory.items.get(&TemplateId::new(30001).unwrap()),
        Some(&17)
    );
    assert_eq!(
        loaded.activities.progress.get("spring\u{1f}merits"),
        Some(&33)
    );
    assert_eq!(loaded.invite_score.have_got_ssr, 1);
    assert_eq!(loaded.invite_score.have_got_fashion, 1);
    assert_eq!(loaded.invite_score.have_first_battle_win, 1);
    assert_eq!(loaded.invite_score.record_version, 7);
    assert_eq!(loaded.talents.active.get(&10), Some(&11));
    assert_eq!(
        loaded.battle.active.as_ref().map(|session| session.copy_id),
        Some(CopyId::new(300).unwrap())
    );
    let active = loaded.battle.active.as_ref().unwrap();
    assert_eq!(active.remaining_fleet_ids, vec![1, 2]);
    assert_eq!(active.hero_ids, vec![hero_id]);
    assert_eq!(active.attack_count, 3);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_loader_does_not_treat_profile_row_as_complete_account() {
    let (store, root) = store();
    let state = StoredProfileState {
        level: 1,
        fuel: 0,
        coins: 0,
        completed_stages: 0,
        ships: Vec::new(),
        formation_ship_ids: Vec::new(),
    };
    store.save("profile-only", "Profile", &state).unwrap();
    assert!(store
        .load_typed_account(&ProfileId::new("profile-only").unwrap())
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(root);
}
