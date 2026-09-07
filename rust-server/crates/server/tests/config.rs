use blueoath_server::{ServerConfig, ServerConfigError};

#[test]
fn parses_equals_and_separate_value_arguments() {
    let config = ServerConfig::from_args([
        "--port".to_owned(),
        "3917".to_owned(),
        "--game-login-port=3918".to_owned(),
        "--profile-id".to_owned(),
        "player-1".to_owned(),
        "--profile-name=Captain".to_owned(),
        "--region=cn".to_owned(),
        "--data".to_owned(),
        "profiles".to_owned(),
        "--client-path".to_owned(),
        "game".to_owned(),
        "--mood-recovery-multiplier=2.5".to_owned(),
        "--affection-multiplier".to_owned(),
        "0.5".to_owned(),
        "--building-oil-multiplier=2".to_owned(),
        "--building-gold-multiplier".to_owned(),
        "0.25".to_owned(),
    ])
    .expect("valid arguments");

    assert_eq!(config.port, 3917);
    assert_eq!(config.game_login_port, Some(3918));
    assert_eq!(config.kcp_game_login_port, None);
    assert_eq!(config.profile_id, "player-1");
    assert_eq!(config.profile_name, "Captain");
    assert_eq!(config.version, "1.5.20");
    assert_eq!(config.data_root, std::path::PathBuf::from("profiles"));
    assert_eq!(config.client_path, Some(std::path::PathBuf::from("game")));
    assert_eq!(config.mood_recovery_multiplier, 2.5);
    assert_eq!(config.affection_multiplier, 0.5);
    assert_eq!(config.building_oil_multiplier, 2.0);
    assert_eq!(config.building_gold_multiplier, 0.25);
}

#[test]
fn parses_kcp_game_login_port() {
    let config = ServerConfig::from_args(["--kcp-game-login-port".to_owned(), "3920".to_owned()])
        .expect("valid arguments");
    assert_eq!(config.kcp_game_login_port, Some(3920));
}

#[test]
fn rejects_invalid_numeric_arguments() {
    let error = ServerConfig::from_args(["--port=not-a-port".to_owned()]).unwrap_err();

    assert!(matches!(
        error,
        ServerConfigError::InvalidValue { flag, value }
            if flag == "--port" && value == "not-a-port"
    ));

    let error = ServerConfig::from_args(["--affection-multiplier=-1".to_owned()]).unwrap_err();
    assert!(matches!(
        error,
        ServerConfigError::InvalidValue { flag, value }
            if flag == "--affection-multiplier" && value == "-1"
    ));
}

#[test]
fn mood_and_affection_multipliers_default_to_one() {
    let config = ServerConfig::from_args(Vec::<String>::new()).expect("valid defaults");

    assert_eq!(config.mood_recovery_multiplier, 1.0);
    assert_eq!(config.affection_multiplier, 1.0);
    assert_eq!(config.building_oil_multiplier, 1.0);
    assert_eq!(config.building_gold_multiplier, 1.0);
}

#[test]
fn rejects_missing_values_for_known_flags() {
    let error = ServerConfig::from_args(["--game-login-port".to_owned()]).unwrap_err();

    assert!(matches!(
        error,
        ServerConfigError::MissingValue { flag } if flag == "--game-login-port"
    ));

    let error =
        ServerConfig::from_args(["--profile-id".to_owned(), "--region=cn".to_owned()]).unwrap_err();
    assert!(matches!(
        error,
        ServerConfigError::MissingValue { flag } if flag == "--profile-id"
    ));
}

#[test]
fn normalizes_profile_id_and_accepts_case_insensitive_flags() {
    let config = ServerConfig::from_args([
        "--PROFILE-ID=  Captain/One  ".to_owned(),
        "--REGION=CN".to_owned(),
    ])
    .expect("valid arguments");

    assert_eq!(config.profile_id, "CaptainOne");
    assert_eq!(config.version, "1.5.20");
}

#[test]
fn invalid_profile_id_falls_back_to_default_and_long_ids_are_truncated() {
    let config = ServerConfig::from_args(["--profile-id=!!!".to_owned()]).expect("valid arguments");
    assert_eq!(config.profile_id, "local-player");

    let long_id = "a".repeat(65);
    let config =
        ServerConfig::from_args([format!("--profile-id={long_id}")]).expect("valid arguments");
    assert_eq!(config.profile_id.len(), 64);

    let config = ServerConfig::from_args([
        "--profile-id=valid".to_owned(),
        "--profile-id=!!!".to_owned(),
    ])
    .expect("valid arguments");
    assert_eq!(config.profile_id, "local-player");
}

#[test]
fn profile_name_defaults_to_normalized_profile_id() {
    let config =
        ServerConfig::from_args(["--profile-id=alice".to_owned()]).expect("valid arguments");
    assert_eq!(config.profile_name, "alice");

    let config = ServerConfig::from_args([
        "--profile-id=alice".to_owned(),
        "--profile-name=   ".to_owned(),
    ])
    .expect("valid arguments");
    assert_eq!(config.profile_name, "alice");
}
