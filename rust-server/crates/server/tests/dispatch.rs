use blueoath_server::{dispatch, ServerState};
use serde_json::json;

#[test]
fn login_uses_selected_profile_and_returns_version() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(&mut state, "login", json!({"profileId": "ignored"})).unwrap();

    assert_eq!(response, json!({"profileId": "slot-a", "version": "1.4.0"}));
}

#[test]
fn state_returns_current_profile_snapshot() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(&mut state, "state", json!({})).unwrap();

    assert_eq!(response["profileId"], "slot-a");
    assert_eq!(response["name"], "Captain");
}

#[test]
fn state_matches_csharp_player_state_shape() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(&mut state, "state", json!({})).unwrap();

    assert_eq!(response["level"], 1);
    assert_eq!(response["fuel"], 100);
    assert_eq!(response["coins"], 0);
    assert_eq!(response["completedStages"], 0);
    assert_eq!(
        response["ships"][0],
        json!({
            "id": 1001,
            "name": "Starter",
            "level": 1,
            "power": 100
        })
    );
    assert_eq!(response["formation"]["shipIds"], json!([1001]));
}

#[test]
fn unknown_message_is_rejected() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let error = dispatch(&mut state, "future_message", json!({})).unwrap_err();

    assert!(error.to_string().contains("Unknown message"));
}

#[test]
fn set_formation_validates_and_persists_ship_ids() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(
        &mut state,
        "set_formation",
        json!({"shipIds": [1001, 1002]}),
    )
    .unwrap();

    assert_eq!(state.formation.ship_ids, vec![1001, 1002]);
    assert_eq!(response["formation"]["shipIds"], json!([1001, 1002]));
}

#[test]
fn set_formation_rejects_unknown_or_duplicate_ships() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let error = dispatch(
        &mut state,
        "set_formation",
        json!({"shipIds": [1001, 1001]}),
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("Formation contains invalid ships"));
    assert_eq!(state.formation.ship_ids, vec![1001]);
}

#[test]
fn enter_stage_returns_csharp_compatible_stage_shape() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(&mut state, "enter_stage", json!({"stageId": 1})).unwrap();

    assert_eq!(response["id"], 1);
    assert_eq!(response["name"], "Tutorial Waters");
    assert_eq!(response["fuelCost"], 10);
    assert_eq!(response["coinReward"], 100);
    assert_eq!(response["enemies"][0]["id"], 9001);
}

#[test]
fn battle_result_updates_state_and_returns_outcome() {
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");
    let response = dispatch(
        &mut state,
        "battle_result",
        json!({"stageId": 1, "win": true}),
    )
    .unwrap();

    assert_eq!(response["state"]["fuel"], 90);
    assert_eq!(response["state"]["coins"], 100);
    assert_eq!(response["state"]["completedStages"], 1);
    assert_eq!(response["outcome"]["victory"], true);
    assert_eq!(response["outcome"]["fuelSpent"], 10);
    assert_eq!(response["outcome"]["coinsGained"], 100);
    assert_eq!(response["outcome"]["message"], "Victory");
}
