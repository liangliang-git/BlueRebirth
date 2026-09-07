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
