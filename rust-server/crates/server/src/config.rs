use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use blueoath_storage::{LocalProfileState, LocalShip, ProfileStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServerConfigError {
    #[error("missing value for {flag}")]
    MissingValue { flag: &'static str },
    #[error("invalid value for {flag}: {value}")]
    InvalidValue { flag: &'static str, value: String },
}

#[derive(Clone, Debug)]
pub(crate) struct SharedPush {
    pub(crate) recipient_uid: u64,
    pub(crate) method: String,
    pub(crate) payload: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypedCoopUser {
    pub(crate) uid: u64,
    pub(crate) name: String,
    pub(crate) head: u32,
    pub(crate) fashioning: u32,
    pub(crate) is_ready: bool,
    pub(crate) enter_time: u32,
    pub(crate) hero_ids: Vec<i32>,
    pub(crate) auto_ready: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypedCoopRoom {
    pub(crate) room_id: u64,
    pub(crate) copy_id: i32,
    pub(crate) owner_id: u64,
    pub(crate) is_public: bool,
    pub(crate) capacity: u32,
    pub(crate) create_time: u32,
    pub(crate) state: u32,
    pub(crate) password: u64,
    pub(crate) focus: bool,
    pub(crate) users: Vec<TypedCoopUser>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypedBattleRoom {
    pub(crate) room_id: u64,
    pub(crate) owner_id: u64,
    pub(crate) users: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypedMatchQueueEntry {
    pub(crate) uid: u64,
    pub(crate) match_type: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypedBattleSession {
    pub(crate) battle_id: u64,
    pub(crate) users: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct SharedSocialState {
    pub(crate) typed_rooms: std::collections::BTreeMap<u64, TypedCoopRoom>,
    pub(crate) typed_battle_rooms: std::collections::BTreeMap<u64, TypedBattleRoom>,
    pub(crate) typed_match_queue: Vec<TypedMatchQueueEntry>,
    pub(crate) typed_battles: std::collections::BTreeMap<u64, TypedBattleSession>,
    pub(crate) pending_pushes: std::collections::BTreeMap<u64, Vec<(String, Vec<u8>)>>,
    pub(crate) push_tx: broadcast::Sender<SharedPush>,
}

impl Default for SharedSocialState {
    fn default() -> Self {
        let (push_tx, _) = broadcast::channel(256);
        Self {
            typed_rooms: std::collections::BTreeMap::new(),
            typed_battle_rooms: std::collections::BTreeMap::new(),
            typed_match_queue: Vec::new(),
            typed_battles: std::collections::BTreeMap::new(),
            pending_pushes: std::collections::BTreeMap::new(),
            push_tx,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    pub profile_id: String,
    pub name: String,
    pub version: String,
    /// Port used by battle/session control-plane payloads. Runtime replaces the
    /// default after binding the front-door listener, including when port=0.
    pub(crate) battle_port: u16,
    pub coins: i64,
    pub fuel: i64,
    pub level: i32,
    pub ships: Vec<Ship>,
    pub formation: Formation,
    pub completed_stages: i32,
    pub drop_multiplier: f64,
    pub ship_exp_multiplier: f64,
    pub commander_exp_multiplier: f64,
    pub ship_stat_multiplier: f64,
    pub mood_recovery_multiplier: f64,
    pub affection_multiplier: f64,
    pub building_oil_multiplier: f64,
    pub building_gold_multiplier: f64,
    pub(crate) social_store: Option<ProfileStore>,
    pub(crate) shared_social: Arc<Mutex<SharedSocialState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ship {
    pub id: i32,
    pub name: String,
    pub level: i32,
    pub power: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Formation {
    #[serde(rename = "shipIds")]
    pub ship_ids: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stage {
    pub id: i32,
    pub name: String,
    pub enemies: Vec<Ship>,
    #[serde(rename = "fuelCost")]
    pub fuel_cost: i64,
    #[serde(rename = "coinReward")]
    pub coin_reward: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BattleOutcome {
    pub victory: bool,
    #[serde(rename = "fuelSpent")]
    pub fuel_spent: i64,
    #[serde(rename = "coinsGained")]
    pub coins_gained: i64,
    #[serde(rename = "completedStages")]
    pub completed_stages: i32,
    pub message: String,
}

impl ServerState {
    pub fn new(
        profile_id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            name: name.into(),
            version: version.into(),
            battle_port: 7080,
            coins: 0,
            fuel: 100,
            level: 1,
            ships: vec![
                Ship {
                    id: 1001,
                    name: "Starter".to_owned(),
                    level: 1,
                    power: 100,
                },
                Ship {
                    id: 1002,
                    name: "Scout".to_owned(),
                    level: 1,
                    power: 80,
                },
            ],
            formation: Formation {
                ship_ids: vec![1001],
            },
            completed_stages: 0,
            drop_multiplier: 1.0,
            ship_exp_multiplier: 1.0,
            commander_exp_multiplier: 1.0,
            ship_stat_multiplier: 1.0,
            mood_recovery_multiplier: 1.0,
            affection_multiplier: 1.0,
            building_oil_multiplier: 1.0,
            building_gold_multiplier: 1.0,
            social_store: None,
            shared_social: Arc::new(Mutex::new(SharedSocialState::default())),
        }
    }

    pub(crate) fn snapshot(&self) -> Value {
        json!({
            "profileId": self.profile_id,
            "name": self.name,
            "level": self.level,
            "fuel": self.fuel,
            "coins": self.coins,
            "ships": self.ships,
            "formation": self.formation,
            "completedStages": self.completed_stages,
        })
    }

    pub(crate) fn local_profile_state(&self) -> LocalProfileState {
        LocalProfileState {
            level: self.level,
            fuel: self.fuel,
            coins: self.coins,
            ships: self
                .ships
                .iter()
                .map(|ship| LocalShip {
                    id: ship.id,
                    name: ship.name.clone(),
                    level: ship.level,
                    power: ship.power,
                })
                .collect(),
            formation_ship_ids: self.formation.ship_ids.clone(),
            completed_stages: self.completed_stages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub game_login_port: Option<u16>,
    pub kcp_game_login_port: Option<u16>,
    pub profile_id: String,
    pub profile_name: String,
    pub version: String,
    pub data_root: PathBuf,
    pub client_path: Option<PathBuf>,
    pub drop_multiplier: f64,
    pub ship_exp_multiplier: f64,
    pub commander_exp_multiplier: f64,
    pub ship_stat_multiplier: f64,
    pub mood_recovery_multiplier: f64,
    pub affection_multiplier: f64,
    pub building_oil_multiplier: f64,
    pub building_gold_multiplier: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ServerFileConfig {
    port: Option<u16>,
    #[serde(alias = "gameLoginPort")]
    game_login_port: Option<u16>,
    #[serde(alias = "kcpGameLoginPort")]
    kcp_game_login_port: Option<u16>,
    #[serde(alias = "profileId")]
    profile_id: Option<String>,
    #[serde(alias = "profileName")]
    profile_name: Option<String>,
    version: Option<String>,
    #[serde(alias = "dataRoot")]
    data: Option<PathBuf>,
    #[serde(alias = "clientPath")]
    client_path: Option<PathBuf>,
    #[serde(alias = "dropMultiplier")]
    drop_multiplier: Option<f64>,
    #[serde(alias = "shipExpMultiplier")]
    ship_exp_multiplier: Option<f64>,
    #[serde(alias = "commanderExpMultiplier")]
    commander_exp_multiplier: Option<f64>,
    #[serde(alias = "shipStatMultiplier")]
    ship_stat_multiplier: Option<f64>,
    #[serde(alias = "moodRecoveryMultiplier")]
    mood_recovery_multiplier: Option<f64>,
    #[serde(alias = "affectionMultiplier")]
    affection_multiplier: Option<f64>,
    #[serde(alias = "buildingOilMultiplier")]
    building_oil_multiplier: Option<f64>,
    #[serde(alias = "buildingGoldMultiplier")]
    building_gold_multiplier: Option<f64>,
}

pub(super) const DEFAULT_PROFILE_ID: &str = "local-player";

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 0,
            game_login_port: None,
            kcp_game_login_port: None,
            profile_id: DEFAULT_PROFILE_ID.to_owned(),
            profile_name: DEFAULT_PROFILE_ID.to_owned(),
            version: "1.4.0".to_owned(),
            data_root: default_data_root(),
            client_path: default_client_path(),
            drop_multiplier: 1.0,
            ship_exp_multiplier: 1.0,
            commander_exp_multiplier: 1.0,
            ship_stat_multiplier: 1.0,
            mood_recovery_multiplier: 1.0,
            affection_multiplier: 1.0,
            building_oil_multiplier: 1.0,
            building_gold_multiplier: 1.0,
        }
    }
}

fn default_data_root() -> PathBuf {
    bundled_catalog_root()
        .map(|root| root.join("data"))
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
                .map(|path| path.join("data"))
                .unwrap_or_else(|| PathBuf::from("data"))
        })
}

fn default_client_path() -> Option<PathBuf> {
    bundled_catalog_root().map(|root| root.join("config"))
}

fn bundled_catalog_root() -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let mut cursor = Some(parent);
            for _ in 0..8 {
                let Some(base) = cursor else { break };
                bases.push(base.to_path_buf());
                cursor = base.parent();
            }
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        let mut cursor = Some(current_dir.as_path());
        for _ in 0..8 {
            let Some(base) = cursor else { break };
            bases.push(base.to_path_buf());
            cursor = base.parent();
        }
    }
    bases.into_iter().find_map(|base| {
        [
            base.join("catalog"),
            base.join("rust-server").join("catalog"),
        ]
        .into_iter()
        .find(|root| {
            let config = root.join("config");
            let data = root.join("data");
            config.is_dir()
                && data.is_dir()
                && (config.join("config_chapter.db").is_file()
                    || config.join("config_shop.db").is_file()
                    || config.join("config_chapter.json").is_file()
                    || config.join("config_shop.json").is_file())
                && data.join("gm-goods.json").is_file()
        })
    })
}

impl ServerConfig {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, ServerConfigError> {
        let mut config = Self::default();
        let args = args.into_iter().collect::<Vec<_>>();
        let explicit_config = args.iter().enumerate().find_map(|(index, arg)| {
            strip_prefix_ci(arg, "--config=")
                .map(PathBuf::from)
                .or_else(|| {
                    arg.eq_ignore_ascii_case("--config")
                        .then(|| args.get(index + 1).cloned().map(PathBuf::from))
                        .flatten()
                })
        });
        let config_path = explicit_config.or_else(|| {
            let exe = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.join("server.json")));
            exe.filter(|path| path.is_file()).or_else(|| {
                let path = PathBuf::from("server.json");
                path.is_file().then_some(path)
            })
        });
        if let Some(path) = config_path.filter(|path| path.is_file()) {
            let raw =
                std::fs::read_to_string(&path).map_err(|_| ServerConfigError::InvalidValue {
                    flag: "--config",
                    value: path.display().to_string(),
                })?;
            let file: ServerFileConfig =
                serde_json::from_str(&raw).map_err(|_| ServerConfigError::InvalidValue {
                    flag: "--config",
                    value: path.display().to_string(),
                })?;
            if let Some(value) = file.port {
                config.port = value;
            }
            if let Some(value) = file.game_login_port {
                config.game_login_port = Some(value);
            }
            if let Some(value) = file.kcp_game_login_port {
                config.kcp_game_login_port = Some(value);
            }
            if let Some(value) = file.profile_id {
                config.profile_id = normalize_profile_id(&value);
            }
            if let Some(value) = file.profile_name {
                config.profile_name = value;
            }
            if let Some(value) = file.version {
                config.version = value;
            }
            if let Some(value) = file.data {
                config.data_root = value;
            }
            if let Some(value) = file.client_path {
                config.client_path = Some(value);
            }
            if let Some(value) = file.drop_multiplier {
                config.drop_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.ship_exp_multiplier {
                config.ship_exp_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.commander_exp_multiplier {
                config.commander_exp_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.ship_stat_multiplier {
                config.ship_stat_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.mood_recovery_multiplier {
                config.mood_recovery_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.affection_multiplier {
                config.affection_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.building_oil_multiplier {
                config.building_oil_multiplier = normalize_multiplier(value);
            }
            if let Some(value) = file.building_gold_multiplier {
                config.building_gold_multiplier = normalize_multiplier(value);
            }
        }
        let mut profile_name = None;
        let mut args = args.into_iter().peekable();
        while let Some(arg) = args.next() {
            if let Some(value) = strip_prefix_ci(&arg, "--port=") {
                config.port = parse_port("--port", value)?;
            } else if arg.eq_ignore_ascii_case("--port") {
                config.port = parse_port("--port", next_value(&mut args, "--port")?.as_str())?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--game-login-port=") {
                config.game_login_port = Some(parse_port("--game-login-port", value)?);
            } else if arg.eq_ignore_ascii_case("--game-login-port") {
                config.game_login_port = Some(parse_port(
                    "--game-login-port",
                    next_value(&mut args, "--game-login-port")?.as_str(),
                )?);
            } else if let Some(value) = strip_prefix_ci(&arg, "--kcp-game-login-port=") {
                config.kcp_game_login_port = Some(parse_port("--kcp-game-login-port", value)?);
            } else if arg.eq_ignore_ascii_case("--kcp-game-login-port") {
                config.kcp_game_login_port = Some(parse_port(
                    "--kcp-game-login-port",
                    next_value(&mut args, "--kcp-game-login-port")?.as_str(),
                )?);
            } else if let Some(value) = strip_prefix_ci(&arg, "--profile-id=") {
                config.profile_id = normalize_profile_id(value);
            } else if arg.eq_ignore_ascii_case("--profile-id") {
                let value = next_value(&mut args, "--profile-id")?;
                config.profile_id = normalize_profile_id(&value);
            } else if let Some(value) = strip_prefix_ci(&arg, "--profile-name=") {
                profile_name = Some(value.to_owned());
            } else if arg.eq_ignore_ascii_case("--profile-name") {
                let value = next_value(&mut args, "--profile-name")?;
                profile_name = Some(value);
            } else if let Some(value) = strip_prefix_ci(&arg, "--region=") {
                if value.eq_ignore_ascii_case("cn") {
                    config.version = "1.5.20".to_owned();
                }
            } else if let Some(value) = strip_prefix_ci(&arg, "--data=") {
                if !value.is_empty() {
                    config.data_root = PathBuf::from(value);
                }
            } else if arg.eq_ignore_ascii_case("--data") {
                let value = next_value(&mut args, "--data")?;
                if !value.is_empty() {
                    config.data_root = PathBuf::from(value);
                }
            } else if let Some(value) = strip_prefix_ci(&arg, "--client-path=") {
                if !value.is_empty() {
                    config.client_path = Some(PathBuf::from(value));
                }
            } else if arg.eq_ignore_ascii_case("--client-path") {
                let value = next_value(&mut args, "--client-path")?;
                if !value.is_empty() {
                    config.client_path = Some(PathBuf::from(value));
                }
            } else if arg.eq_ignore_ascii_case("--config") {
                let _ = next_value(&mut args, "--config")?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--drop-multiplier=") {
                config.drop_multiplier = parse_multiplier("--drop-multiplier", value)?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--ship-exp-multiplier=") {
                config.ship_exp_multiplier = parse_multiplier("--ship-exp-multiplier", value)?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--commander-exp-multiplier=") {
                config.commander_exp_multiplier =
                    parse_multiplier("--commander-exp-multiplier", value)?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--ship-stat-multiplier=") {
                config.ship_stat_multiplier = parse_multiplier("--ship-stat-multiplier", value)?;
            } else if arg.eq_ignore_ascii_case("--ship-stat-multiplier") {
                config.ship_stat_multiplier = parse_multiplier(
                    "--ship-stat-multiplier",
                    next_value(&mut args, "--ship-stat-multiplier")?.as_str(),
                )?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--mood-recovery-multiplier=") {
                config.mood_recovery_multiplier =
                    parse_multiplier("--mood-recovery-multiplier", value)?;
            } else if arg.eq_ignore_ascii_case("--mood-recovery-multiplier") {
                config.mood_recovery_multiplier = parse_multiplier(
                    "--mood-recovery-multiplier",
                    next_value(&mut args, "--mood-recovery-multiplier")?.as_str(),
                )?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--affection-multiplier=") {
                config.affection_multiplier = parse_multiplier("--affection-multiplier", value)?;
            } else if arg.eq_ignore_ascii_case("--affection-multiplier") {
                config.affection_multiplier = parse_multiplier(
                    "--affection-multiplier",
                    next_value(&mut args, "--affection-multiplier")?.as_str(),
                )?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--building-oil-multiplier=") {
                config.building_oil_multiplier =
                    parse_multiplier("--building-oil-multiplier", value)?;
            } else if arg.eq_ignore_ascii_case("--building-oil-multiplier") {
                config.building_oil_multiplier = parse_multiplier(
                    "--building-oil-multiplier",
                    next_value(&mut args, "--building-oil-multiplier")?.as_str(),
                )?;
            } else if let Some(value) = strip_prefix_ci(&arg, "--building-gold-multiplier=") {
                config.building_gold_multiplier =
                    parse_multiplier("--building-gold-multiplier", value)?;
            } else if arg.eq_ignore_ascii_case("--building-gold-multiplier") {
                config.building_gold_multiplier = parse_multiplier(
                    "--building-gold-multiplier",
                    next_value(&mut args, "--building-gold-multiplier")?.as_str(),
                )?;
            }
        }
        config.profile_name = normalize_profile_name(profile_name.as_deref(), &config.profile_id);
        Ok(config)
    }
}

fn strip_prefix_ci<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

pub(super) fn normalize_profile_id(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .take(64)
        .collect();
    if normalized.is_empty() {
        DEFAULT_PROFILE_ID.to_owned()
    } else {
        normalized
    }
}

fn normalize_profile_name(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_owned();
    };
    let normalized: String = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(32)
        .collect();
    if normalized.is_empty() {
        fallback.to_owned()
    } else {
        normalized
    }
}

fn next_value<I>(
    args: &mut std::iter::Peekable<I>,
    flag: &'static str,
) -> Result<String, ServerConfigError>
where
    I: Iterator<Item = String>,
{
    if args.peek().is_some_and(|value| is_known_flag(value)) {
        return Err(ServerConfigError::MissingValue { flag });
    }
    args.next().ok_or(ServerConfigError::MissingValue { flag })
}

fn is_known_flag(value: &str) -> bool {
    [
        "--port",
        "--game-login-port",
        "--kcp-game-login-port",
        "--profile-id",
        "--profile-name",
        "--region",
        "--data",
        "--client-path",
        "--config",
        "--ship-stat-multiplier",
        "--mood-recovery-multiplier",
        "--affection-multiplier",
    ]
    .into_iter()
    .any(|flag| {
        value.eq_ignore_ascii_case(flag) || strip_prefix_ci(value, &format!("{flag}=")).is_some()
    })
}

pub(super) fn normalize_multiplier(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value.min(1_000_000.0)
    } else {
        1.0
    }
}

fn parse_multiplier(flag: &'static str, value: &str) -> Result<f64, ServerConfigError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| ServerConfigError::InvalidValue {
            flag,
            value: value.to_owned(),
        })?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(ServerConfigError::InvalidValue {
            flag,
            value: value.to_owned(),
        });
    }
    Ok(parsed.min(1_000_000.0))
}

pub(super) fn scale_reward(value: i64, multiplier: f64) -> i64 {
    if value <= 0 {
        return 0;
    }
    let multiplier = normalize_multiplier(multiplier);
    ((value as f64) * multiplier)
        .round()
        .clamp(0.0, i64::MAX as f64) as i64
}

fn parse_port(flag: &'static str, value: &str) -> Result<u16, ServerConfigError> {
    value
        .parse::<u16>()
        .map_err(|_| ServerConfigError::InvalidValue {
            flag,
            value: value.to_owned(),
        })
}
