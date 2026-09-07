use blueoath_domain::{
    AccountRepository, AccountState, BattleSession, ChapterId, ChatBarrageState, ChatMessageState,
    CopyId, EquipId, EquipmentState, FleetId, FleetRecord, GuildApplicationState, GuildMemberState,
    GuildState, HeroId, HeroState, NewAccountFactory, PresetFleetState, ProfileId, ProfileState,
    TemplateId,
};
use blueoath_storage::{LocalProfileState, LocalShip, ProfileStore, StorageError};
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

fn profile_state(coins: i64) -> LocalProfileState {
    LocalProfileState {
        level: 1,
        fuel: 100,
        coins,
        ..LocalProfileState::default()
    }
}

#[test]
fn profiles_are_upserted_and_isolated() {
    let (store, root) = store();
    store.save_local("one", "One", &profile_state(10)).unwrap();
    store.save_local("two", "Two", &profile_state(20)).unwrap();

    assert_eq!(store.load_local("one").unwrap().unwrap().state.coins, 10);
    assert_eq!(store.load_local("two").unwrap().unwrap().state.coins, 20);
    assert_eq!(store.list().unwrap(), vec!["one", "two"]);

    store
        .save_local("one", "Renamed", &profile_state(11))
        .unwrap();
    let renamed = store.load_local("one").unwrap().unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.state.coins, 11);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reset_removes_only_selected_profile() {
    let (store, root) = store();
    store
        .save_local("one", "One", &LocalProfileState::default())
        .unwrap();
    store
        .save_local("two", "Two", &LocalProfileState::default())
        .unwrap();
    store.reset("one").unwrap();

    assert!(store.load_local("one").unwrap().is_none());
    assert!(store.load_local("two").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_profile_id_is_rejected_before_database_write() {
    let (store, root) = store();
    let error = store
        .save_local("bad/id", "Bad", &LocalProfileState::default())
        .unwrap_err();

    assert!(matches!(error, StorageError::InvalidProfileId));
    assert!(store.list().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dot_profile_id_is_accepted_like_csharp_server() {
    let (store, root) = store();
    store
        .save_local("jp.v1", "JP", &LocalProfileState::default())
        .unwrap();
    assert!(store.load_local("jp.v1").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn updated_timestamp_uses_iso8601_utc_format() {
    let (store, root) = store();
    store
        .save_local("one", "One", &LocalProfileState::default())
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
fn local_profile_runtime_round_trips_without_json_state_column() {
    let (store, root) = store();
    let state = LocalProfileState {
        level: 4,
        fuel: 80,
        coins: 125,
        ships: vec![LocalShip {
            id: 1001,
            name: "Starter".to_owned(),
            level: 3,
            power: 220,
        }],
        formation_ship_ids: vec![1001],
        completed_stages: 7,
    };
    store
        .save_local("typed-profile", "Captain", &state)
        .unwrap();

    let loaded = store.load_local("typed-profile").unwrap().unwrap();
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
    account.daily_copy.ex_stars.insert(chapter, 5);
    account.daily_copy.group_success_times.insert(1, 3);
    account.daily_copy.extra_group_success_times.insert(1, 4);
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
fn migration_from_schema_v6_normalizes_local_runtime_and_character_fields() {
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
    assert_eq!(version, 32);
    let accounts_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'accounts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(accounts_table, 0);
    for table in ["local_runtime", "local_ships", "local_formation"] {
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1, "missing {table}");
    }
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
fn future_schema_version_rejects_startup() {
    let suffix = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "blueoath-rust-future-schema-test-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                version INTEGER NOT NULL,
                applied_at TEXT NOT NULL
             );
             INSERT INTO schema_meta(id, version, applied_at)
             VALUES (1, 999, 'future');",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        ProfileStore::open(&root),
        Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery))
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn account_revision_is_cascaded_with_profile() {
    let (store, root) = store();
    let profile_id = ProfileId::new("revision-cascade").unwrap();
    let account = NewAccountFactory::create(profile_id.clone(), "Revision Captain");
    AccountRepository::create(&store, &account).unwrap();

    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let foreign_key: i64 = connection
        .query_row(
            r#"SELECT COUNT(*) FROM pragma_foreign_key_list('account_revisions')
            WHERE "table" = 'profiles' AND on_delete = 'CASCADE'"#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(foreign_key, 1);
    drop(connection);

    store.reset(profile_id.as_str()).unwrap();
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let revisions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM account_revisions WHERE profile_id = ?1",
            [profile_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 0);
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
            pskills: [(41, 2)].into_iter().collect(),
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
    account.sea.difficulty = 3;
    account.battle.claimed_star_rewards.insert((3, 1));
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
    account
        .guide
        .settings
        .insert("tutorial".to_owned(), "closed".to_owned());
    account.guide.plot_rewards.insert(42);
    account.supply.hero_ids = vec![hero_id];
    account
        .support
        .entries
        .push(blueoath_domain::SupportEntryState {
            id: 1,
            support_id: 7001,
            start_time: 1234,
            hero_ids: vec![hero_id],
        });
    account.invite_score.have_got_ssr = 1;
    account.invite_score.have_got_fashion = 1;
    account.invite_score.have_first_battle_win = 1;
    account.invite_score.record_version = 7;
    account.talents.active.insert(10, 11);
    account.sports_meet.tick_count = 10;
    account.sports_meet.points = 20;
    account.sports_meet.free_counts.insert(3001, 2);
    account.sports_meet.received_points.insert(20);
    account.bathroom.is_all_auto = true;
    account
        .bathroom
        .heroes
        .push(blueoath_domain::BathroomHeroState {
            hero_id: 10,
            position: 2,
            is_auto: true,
            start_time: 100,
            bath_time: 40,
            buff_id: 7,
            buff_time: 8,
            power: 9,
        });
    account
        .study
        .progress
        .push(blueoath_domain::StudyProgressState {
            hero_id: 10,
            skill_id: 41,
            textbook_id: 7001,
            begin_time: 100,
            end_time: 160,
        });
    account.build_ship.draw_counts.insert(106, 10);
    account
        .build_ship
        .used_box_info
        .entry(106)
        .or_default()
        .insert(10);
    account
        .build_ship
        .used_reward_info
        .entry(106)
        .or_default()
        .insert(20);
    account.guild = Some(GuildState {
        id: 9000001,
        name: "Typed Fleet".to_owned(),
        emblem: 2,
        level: 3,
        leader_id: 10,
        leader_name: "Captain".to_owned(),
        member_num: 1,
        my_post: 1,
        members: vec![GuildMemberState {
            uid: 10,
            name: "Captain".to_owned(),
            post: 1,
            contribute: 7,
            today_contribute: 2,
            power: 99,
        }],
        applications: vec![GuildApplicationState {
            uid: 11,
            name: "Applicant".to_owned(),
            time: 100,
            quality: 4,
        }],
        ..Default::default()
    });
    account.guild_box.progress = 12;
    account.guild_box.anonymous = true;
    account.guild_box.points_box_count = 3;
    account
        .guild_box
        .share_boxes
        .push(blueoath_domain::GuildBoxItemState {
            box_id: 77,
            end_time: 1000,
            box_uid: 10,
            is_picked: false,
            recharge_id: 5,
            recharge_name: "recharge".to_owned(),
        });
    account.adventure.roles[0].level = 4;
    account.adventure.roles[0].hp = 4_000;
    account.adventure.enemies[1].damage = 900;
    account.adventure.enemy_index = 1;
    account.ship_task.current_ship_tid = 12;
    account.ship_task.current_hero_template_id = 1200;
    account.exchange_times.insert(7001, 3);
    account.food_compose.last_recipe_id = 41;
    account.food_compose.recipes.insert(41, 2);
    account.world_event.progress = 8;
    account.world_event.user_progress = 6;
    account.world_event.stages.push(3);
    account
        .world_event
        .claimed_stages_by_event
        .entry(5001)
        .or_default()
        .insert(3);
    account.battle_pass.pass_type = 2;
    account.battle_pass.pass_level = 7;
    account.battle_pass.pass_exp = 19;
    account.battle_pass.claimed_rewards.insert((2, 3));
    account.battle_pass.claimed_tasks.insert(101);
    account.battle_pass.tasks.insert(101, 4);
    account.activity_battle_pass.pass_level = 5;
    account.magazine.heroes.push(10);
    account.magazine.votes.insert(2);
    account.magazine.unlocked.insert(3);
    account.magazine.claimed_rewards.insert(4);
    account.interaction_items.crystal_ball_toy = 8;
    account.interaction_items.rewards.insert(9);
    account.interaction_items.visible.insert(10, true);
    account.interaction_items.groups.insert(11, 2);
    account.interaction_items.posters.insert(12, 3);
    account
        .sweep
        .entries
        .push(blueoath_domain::SweepEntryState {
            fleet_id: 1,
            copy_id: 9,
            start_time: 100,
            end_time: 101,
            sweep_counts: 2,
            chapter_id: 0,
        });
    account
        .ship_task
        .tasks
        .push(blueoath_domain::ShipTaskTaskState {
            ship_tid: 12,
            task_id: 3,
            status: 1,
            count: 2,
        });
    account
        .ship_task
        .achievements
        .push(blueoath_domain::ShipTaskAchievementState {
            ship_tid: 12,
            id: 4,
            claimed: true,
        });
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
    account
        .battle
        .records
        .push(blueoath_domain::CopyRecordState {
            copy_id: CopyId::new(300).unwrap(),
            hero_ids: vec![hero_id],
            pass_time: 12,
            secret_id: 2,
            strategy_id: 3,
            power: 99,
            record_time: 200,
            ex_buffs: vec![7001],
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
    assert_eq!(loaded.dock.heroes[&hero_id].pskills.get(&41), Some(&2));
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
    assert_eq!(loaded.sea.difficulty, 3);
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
    assert_eq!(
        loaded.guide.settings.get("tutorial"),
        Some(&"closed".to_owned())
    );
    assert!(loaded.guide.plot_rewards.contains(&42));
    assert_eq!(loaded.supply.hero_ids, vec![hero_id]);
    assert_eq!(loaded.support.entries[0].support_id, 7001);
    assert_eq!(loaded.support.entries[0].hero_ids, vec![hero_id]);
    assert_eq!(loaded.invite_score.have_got_ssr, 1);
    assert_eq!(loaded.invite_score.have_got_fashion, 1);
    assert_eq!(loaded.invite_score.have_first_battle_win, 1);
    assert_eq!(loaded.invite_score.record_version, 7);
    assert_eq!(loaded.talents.active.get(&10), Some(&11));
    assert_eq!(loaded.sports_meet.tick_count, 10);
    assert_eq!(loaded.sports_meet.points, 20);
    assert_eq!(loaded.sports_meet.free_counts.get(&3001), Some(&2));
    assert!(loaded.sports_meet.received_points.contains(&20));
    assert!(loaded.bathroom.is_all_auto);
    assert_eq!(loaded.bathroom.heroes[0].hero_id, 10);
    assert_eq!(loaded.bathroom.heroes[0].position, 2);
    assert!(loaded.bathroom.heroes[0].is_auto);
    assert_eq!(loaded.bathroom.heroes[0].buff_id, 7);
    assert_eq!(loaded.study.progress.len(), 1);
    assert_eq!(loaded.study.progress[0].skill_id, 41);
    assert_eq!(loaded.build_ship.draw_counts.get(&106), Some(&10));
    assert!(loaded
        .build_ship
        .used_box_info
        .get(&106)
        .is_some_and(|claims| claims.contains(&10)));
    assert!(loaded
        .build_ship
        .used_reward_info
        .get(&106)
        .is_some_and(|claims| claims.contains(&20)));
    let guild = loaded.guild.as_ref().unwrap();
    assert_eq!(guild.name, "Typed Fleet");
    assert_eq!(guild.members[0].contribute, 7);
    assert_eq!(guild.applications[0].quality, 4);
    assert_eq!(loaded.guild_box.progress, 12);
    assert!(loaded.guild_box.anonymous);
    assert_eq!(loaded.guild_box.points_box_count, 3);
    assert_eq!(loaded.guild_box.share_boxes[0].box_id, 77);
    assert_eq!(loaded.guild_box.share_boxes[0].recharge_name, "recharge");
    assert_eq!(loaded.adventure.roles[0].level, 4);
    assert_eq!(loaded.adventure.roles[0].hp, 4_000);
    assert_eq!(loaded.adventure.enemies[1].damage, 900);
    assert_eq!(loaded.adventure.enemy_index, 1);
    assert_eq!(loaded.ship_task.current_ship_tid, 12);
    assert_eq!(loaded.ship_task.tasks[0].count, 2);
    assert!(loaded.ship_task.achievements[0].claimed);
    assert_eq!(loaded.exchange_times.get(&7001), Some(&3));
    assert_eq!(loaded.food_compose.last_recipe_id, 41);
    assert_eq!(loaded.food_compose.recipes.get(&41), Some(&2));
    assert_eq!(loaded.world_event.progress, 8);
    assert_eq!(loaded.world_event.user_progress, 6);
    assert_eq!(loaded.world_event.stages, vec![3]);
    assert!(loaded
        .world_event
        .claimed_stages_by_event
        .get(&5001)
        .is_some_and(|stages| stages.contains(&3)));
    assert_eq!(loaded.battle_pass.pass_type, 2);
    assert_eq!(loaded.battle_pass.pass_level, 7);
    assert_eq!(loaded.battle_pass.pass_exp, 19);
    assert!(loaded.battle_pass.claimed_rewards.contains(&(2, 3)));
    assert!(loaded.battle_pass.claimed_tasks.contains(&101));
    assert_eq!(loaded.battle_pass.tasks.get(&101), Some(&4));
    assert_eq!(loaded.activity_battle_pass.pass_level, 5);
    assert_eq!(loaded.magazine.heroes, vec![10]);
    assert!(loaded.magazine.votes.contains(&2));
    assert!(loaded.magazine.unlocked.contains(&3));
    assert!(loaded.magazine.claimed_rewards.contains(&4));
    assert_eq!(loaded.interaction_items.crystal_ball_toy, 8);
    assert!(loaded.interaction_items.rewards.contains(&9));
    assert_eq!(loaded.interaction_items.visible.get(&10), Some(&true));
    assert_eq!(loaded.interaction_items.groups.get(&11), Some(&2));
    assert_eq!(loaded.interaction_items.posters.get(&12), Some(&3));
    assert_eq!(loaded.sweep.entries[0].fleet_id, 1);
    assert_eq!(loaded.sweep.entries[0].copy_id, 9);
    assert_eq!(loaded.sweep.entries[0].sweep_counts, 2);
    assert_eq!(
        loaded.battle.active.as_ref().map(|session| session.copy_id),
        Some(CopyId::new(300).unwrap())
    );
    let active = loaded.battle.active.as_ref().unwrap();
    assert_eq!(active.remaining_fleet_ids, vec![1, 2]);
    assert_eq!(active.hero_ids, vec![hero_id]);
    assert_eq!(active.attack_count, 3);
    assert_eq!(loaded.battle.records[0].hero_ids, vec![hero_id]);
    assert_eq!(loaded.battle.records[0].ex_buffs, vec![7001]);
    assert_eq!(loaded.battle.records[0].pass_time, 12);
    assert_eq!(loaded.battle.records[0].secret_id, 2);
    assert!(loaded.battle.claimed_star_rewards.contains(&(3, 1)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_repository_transaction_rolls_back_partial_storage_failure() {
    let (store, root) = store();
    let profile_id = ProfileId::new("atomic").unwrap();
    let account = NewAccountFactory::create(profile_id.clone(), "Atomic Captain");
    AccountRepository::create(&store, &account).unwrap();

    let result = AccountRepository::transact(&store, &profile_id, |account| {
        account.character.exp = u64::MAX;
        Ok::<_, blueoath_domain::DomainError>(())
    });

    assert!(result.is_err());
    let loaded = AccountRepository::load(&store, &profile_id)
        .unwrap()
        .expect("original account must survive failed transaction");
    assert_eq!(loaded.character.exp, 0);
    assert_eq!(loaded.profile.expect("profile must survive").revision, 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_loader_does_not_treat_profile_row_as_complete_account() {
    let (store, root) = store();
    let state = LocalProfileState {
        level: 1,
        fuel: 0,
        coins: 0,
        completed_stages: 0,
        ships: Vec::new(),
        formation_ship_ids: Vec::new(),
    };
    store.save_local("profile-only", "Profile", &state).unwrap();
    assert!(store
        .load_typed_account(&ProfileId::new("profile-only").unwrap())
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(root);
}
