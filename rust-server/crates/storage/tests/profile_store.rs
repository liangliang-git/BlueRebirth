use blueoath_domain::{AccountRepository, AccountState, ProfileId, ProfileState};
use blueoath_storage::{ProfileStore, StorageError};
use serde_json::json;
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

#[test]
fn profiles_are_upserted_and_isolated() {
    let (store, root) = store();
    store.save("one", "One", &json!({"coins": 10})).unwrap();
    store.save("two", "Two", &json!({"coins": 20})).unwrap();

    assert_eq!(store.load("one").unwrap().unwrap().state["coins"], 10);
    assert_eq!(store.load("two").unwrap().unwrap().state["coins"], 20);
    assert_eq!(store.list().unwrap(), vec!["one", "two"]);

    store.save("one", "Renamed", &json!({"coins": 11})).unwrap();
    let renamed = store.load("one").unwrap().unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.state["coins"], 11);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn reset_removes_only_selected_profile() {
    let (store, root) = store();
    store.save("one", "One", &json!({})).unwrap();
    store.save("two", "Two", &json!({})).unwrap();
    store.reset("one").unwrap();

    assert!(store.load("one").unwrap().is_none());
    assert!(store.load("two").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_profile_id_is_rejected_before_database_write() {
    let (store, root) = store();
    let error = store.save("bad/id", "Bad", &json!({})).unwrap_err();

    assert!(matches!(error, StorageError::InvalidProfileId));
    assert!(store.list().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dot_profile_id_is_accepted_like_csharp_server() {
    let (store, root) = store();
    store.save("jp.v1", "JP", &json!({})).unwrap();
    assert!(store.load("jp.v1").unwrap().is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn updated_timestamp_uses_iso8601_utc_format() {
    let (store, root) = store();
    store.save("one", "One", &json!({})).unwrap();

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
fn account_json_round_trips_without_losing_unknown_fields() {
    let (store, root) = store();
    let account = json!({
        "profileId": "one",
        "character": {"uid": 1, "name": "One"},
        "futureField": {"preserve": true}
    });

    store.save_account("one", &account).unwrap();
    assert_eq!(store.load_account("one").unwrap(), Some(account));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn account_directory_lists_saved_accounts_in_stable_order() {
    let (store, root) = store();
    store
        .save_account("two", &json!({"profileId": "two", "character": {"uid": 2}}))
        .unwrap();
    store
        .save_account("one", &json!({"profileId": "one", "character": {"uid": 1}}))
        .unwrap();

    let accounts = store.list_accounts().unwrap();
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].0, "one");
    assert_eq!(accounts[0].1["character"]["uid"], 1);
    assert_eq!(accounts[1].0, "two");
    assert_eq!(accounts[1].1["character"]["uid"], 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_account_profile_id_is_rejected() {
    let (store, root) = store();
    let error = store.save_account("bad/id", &json!({})).unwrap_err();

    assert!(matches!(error, StorageError::InvalidProfileId));
    assert!(store.load_account("bad/id").unwrap().is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn account_revision_supports_atomic_compare_and_swap() {
    let (store, root) = store();
    let first = json!({"profileId": "one", "gold": 10});
    let second = json!({"profileId": "one", "gold": 20});

    let revision = store
        .save_account_with_revision("one", &first, None)
        .unwrap();
    assert_eq!(revision, 1);
    assert_eq!(
        store.load_account_with_revision("one").unwrap(),
        Some((first.clone(), 1))
    );

    let next_revision = store
        .save_account_with_revision("one", &second, Some(revision))
        .unwrap();
    assert_eq!(next_revision, 2);

    let error = store
        .save_account_with_revision("one", &first, Some(revision))
        .unwrap_err();
    assert!(matches!(error, StorageError::RevisionConflict { .. }));
    assert_eq!(store.load_account("one").unwrap(), Some(second));
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
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typed_repository_transaction_commits_domain_mutation() {
    let (store, root) = store();
    let profile_id = ProfileId::new("typed").unwrap();
    let account = AccountState {
        profile: Some(ProfileState {
            id: profile_id.clone(),
            name: "Typed Captain".to_owned(),
            revision: 0,
        }),
        resources: Default::default(),
        ..AccountState::default()
    };
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
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    let legacy_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM accounts WHERE id = 'typed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_rows, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn account_snapshot_write_projects_core_rows_into_normalized_tables() {
    let (store, root) = store();
    let account = json!({
        "character": {
            "uid": 7,
            "name": "Captain",
            "level": 3,
            "exp": 12,
            "secretaryId": 1,
            "gold": 100,
            "diamond": 20,
            "supply": 50,
            "pvePt": 4,
            "head": 1021051,
            "headFrame": 2
        },
        "dock": {"heroes": [{
            "heroId": 1,
            "templateId": 10210511,
            "level": 2,
            "exp": 5,
            "mood": 100,
            "affection": 200,
            "curHp": 99,
            "lock": true,
            "equipSlots": [3, 0]
        }]},
        "equip": {"items": [{
            "equipId": 3,
            "templateId": 30091,
            "enhanceLv": 1,
            "star": 2,
            "enhanceExp": 4,
            "heroId": 1
        }]},
        "bag": {"items": [{"templateId": 60000, "num": 8}]},
        "fleet": {"tactics": [{"formationId": 2, "strategyId": 4}]},
        "battleSession": {"copyId": 1001, "startedAt": 10},
        "tasks": {"records": [{"taskId": 9, "type": 1, "progress": 2, "completed": true}]},
        "seaProgress": {"records": [{"copyId": 1001, "starLevel": 7, "passCount": 2}]},
        "copyProgress": {"records": [{"copyId": 2001, "starLevel": 3, "firstPassed": true}]},
        "dailyCopy": {"resetDay": 42, "chapters": [{"chapterId": 8, "groupId": 2, "challengeTimes": 1}]},
        "building": {"buildings": [{"id": 3, "level": 4, "landIndex": 1}]},
        "tower": {"chapterId": 7, "floor": 5, "resetDay": 42}
    });

    store.save_account("normalized", &account).unwrap();
    let typed = store
        .load_typed_account(&ProfileId::new("normalized").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(typed.character.uid, 7);
    assert_eq!(typed.dock.heroes.len(), 1);
    assert_eq!(typed.dock.equipments.len(), 1);
    assert_eq!(
        typed
            .resources
            .amount(blueoath_domain::CurrencyKind::Gold)
            .get(),
        100
    );
    let connection = rusqlite::Connection::open(root.join("profiles.db")).unwrap();
    for (table, expected) in [
        ("characters", 1),
        ("heroes", 1),
        ("equipments", 1),
        ("hero_equip_slots", 2),
        ("inventory", 1),
        ("fleets", 1),
        ("battle_sessions", 1),
        ("tasks", 1),
        ("sea_progress", 1),
        ("copy_progress", 1),
        ("daily_copy_progress", 1),
        ("buildings", 1),
        ("tower_progress", 1),
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "table {table}");
    }
    connection
        .execute("DELETE FROM accounts WHERE id = 'normalized'", [])
        .unwrap();
    let repository_loaded = AccountRepository::load(&store, &ProfileId::new("normalized").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(repository_loaded.character.uid, 7);
    let _ = std::fs::remove_dir_all(root);
}
