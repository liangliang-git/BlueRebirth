#![recursion_limit = "256"]

use std::io;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

#[cfg(test)]
pub(crate) use blueoath_protocol::EquipNum;
use blueoath_protocol::{
    BagInfoCodec, ClientGameWireCodec, EquipListCodec, FashionInfo, FashionList, FashionListCodec,
    HeroBag, HeroBagCodec, HeroGrid, MedalAcquiredTime, TMessageCodec, TResponse, TRetLogin,
    UserInfo, UserInfoCodec,
};
use blueoath_storage::StorageError;
use blueoath_transport::{
    FrameCodec, FrameError, KcpConnection, KcpPacket, NetSocketFrameCodec, NetSocketFrameError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

mod account_defaults;
mod account_state;
mod battle_state;
mod bootstrap;
mod building_state;
mod buildship_state;
mod catalog;
mod catalog_loader;
pub mod common;
mod config;
mod construction_state;
mod equip_state;
mod frame_service;
mod game_login;
#[cfg(test)]
pub(crate) use game_login::equip_handler::{
    equip_activity_payload, equip_new_test_copy_payload, equip_test_copy_payload,
    mark_equip_activity_reward, mark_new_test_reward, resolve_new_test_reward,
};
mod guild_state;
mod hero_state;
mod local_protocol;
mod mopup_state;
mod projection;
mod protocol_payload;
pub mod router;
mod runtime;
mod shop_state;
mod study_state;
mod task_state;
mod wire;

use account_defaults::{
    default_account_snapshot, user_info_from_account, user_info_from_typed_account,
};
use account_state::*;
use battle_state::*;
pub use blueoath_game::{BattleService, ProgressService, ResourceService, RewardService};
#[cfg(test)]
pub(crate) use bootstrap::bootstrap_response;
use bootstrap::{
    handle_bootstrap_http, looks_like_http, looks_like_netsocket, read_connection_prefix,
    PrefixedTcpStream,
};
use building_state::*;
use buildship_state::*;
use catalog::*;
use catalog_loader::*;
use common::clock::{Clock, SystemClock};
use config::{normalize_multiplier, normalize_profile_id, scale_reward, DEFAULT_PROFILE_ID};
pub use config::{
    BattleOutcome, Formation, ServerConfig, ServerConfigError, ServerState, Ship, Stage,
};
use construction_state::*;
use equip_state::*;
pub use frame_service::process_frame;
use frame_service::{prepare_local_request, storage_failure_response};
#[cfg(test)]
pub(crate) use game_login::building_handler::handle_typed as handle_typed_building;
use game_login::process_game_login_frame_payload_with_catalogs_typed_mut;
#[cfg(test)]
pub(crate) use game_login::sync_typed_battle_state;
use guild_state::*;
use hero_state::*;
pub use local_protocol::dispatch;
use mopup_state::*;
use projection::*;
use protocol_payload::*;
use shop_state::*;
use study_state::*;
use task_state::*;
use wire::{
    append_bytes_field, append_message_field, append_varint, append_varint_field,
    read_string_slice, read_varint, skip_wire,
};

pub use runtime::run;
pub(crate) use runtime::{current_unix_millis, normalize_task_state};

const SEA_DIFFICULTY_UNLOCK_LEVEL: i32 = 60;
const INITIAL_SUPPLY: i64 = 10_000;
// Client mood values use fixed-point units with a scale of 10,000.
const MOOD_MIN: i32 = 0;
const MOOD_MAX: i32 = 1_500_000;
const MOOD_INITIAL: i32 = MOOD_MAX;
const MOOD_NORMAL_LIMIT: i32 = 1_190_000;
const MOOD_RECOVERY_INTERVAL_SECONDS: i64 = 6 * 60;
const MOOD_NORMAL_RECOVERY: i32 = 100;
const MOOD_MARRIED_RECOVERY_BONUS: i32 = 100;
const MOOD_BATH_RECOVERY: i32 = 300_000;
const MOOD_BATH_INTERVAL_RECOVERY: i32 = 40_000;
const MOOD_BATH_INTERVAL_SECONDS: i64 = 600;
const MOOD_AFFECTION_BONUS_THRESHOLD: i64 = 1_200_000;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Unknown message: {0}")]
    UnknownMessage(String),
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("frame error: {0}")]
    Frame(#[from] FrameError),
    #[error("NetSocket frame error: {0}")]
    NetSocket(#[from] NetSocketFrameError),
    #[error("protocol error: {0}")]
    Protocol(#[from] blueoath_protocol::ProtocolError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("blocking storage task failed: {0}")]
    StorageTask(String),
    #[error("catalog validation failed: {0}")]
    Catalog(String),
}

/// Processes one game-login NetSocket frame. Account-backed reads and high-frequency profile,
/// hero, fleet, and battle routes are handled directly; unknown methods keep empty-response
/// compatibility until their full state model is ported.
pub async fn process_game_login_frame<S>(
    stream: &mut S,
    state: &ServerState,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    process_game_login_frame_with_account(stream, state, None).await
}

pub async fn process_game_login_frame_with_account<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&Value>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    process_game_login_frame_with_account_and_catalog(stream, state, account, None, None).await
}

pub(crate) async fn process_game_login_frame_with_account_and_catalog<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&Value>,
    fashion_catalog: Option<&FashionList>,
    equip_catalog: Option<&EquipCatalog>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let catalogs = GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        ..GameLoginCatalogs::empty()
    };
    process_game_login_frame_with_catalog(stream, state, account, &catalogs).await
}

async fn process_game_login_frame_with_catalog<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&Value>,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut account_owned = account.cloned();
    process_game_login_frame_with_catalogs_mut(stream, state, account_owned.as_mut(), catalogs)
        .await
}

/// Compatibility adapter for focused route tests that need to override one catalog.
/// Production paths use [`GameLoginCatalogs`] directly.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn process_game_login_frame_with_catalog_mut<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&mut Value>,
    fashion_catalog: Option<&FashionList>,
    equip_catalog: Option<&EquipCatalog>,
    hero_level_catalog: Option<&HeroLevelCatalog>,
    shop_catalog: Option<&ShopCatalog>,
    mail_catalog: Option<&[MailTemplate]>,
    handbook_behaviours: Option<&[i32]>,
    hero_memories: Option<&[(i32, i32)]>,
    chapter_catalog: Option<&ChapterCatalog>,
    task_catalog: Option<&TaskCatalog>,
    battle_catalog: Option<&BattleCatalog>,
    hero_breakdown_catalog: Option<&HeroBreakdownCatalog>,
    building_catalog: Option<&BuildingCatalog>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let catalogs = GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        hero_level: hero_level_catalog,
        shop: shop_catalog,
        mails: mail_catalog,
        handbook_behaviours,
        hero_memories,
        chapters: chapter_catalog,
        tasks: task_catalog,
        battle: battle_catalog,
        hero_breakdown: hero_breakdown_catalog,
        buildings: building_catalog,
        equip_new_test: None,
        affection: None,
        combination: None,
    };
    process_game_login_frame_with_catalogs_mut(stream, state, account, &catalogs).await
}

async fn process_game_login_frame_with_catalogs_mut<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&mut Value>,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    process_game_login_frame_with_catalogs_typed_mut(stream, state, account, None, catalogs).await
}

pub(crate) async fn process_game_login_frame_with_catalogs_typed_mut<S>(
    stream: &mut S,
    state: &ServerState,
    account: Option<&mut Value>,
    typed_account: Option<&mut blueoath_domain::AccountState>,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(frame) = NetSocketFrameCodec::read(stream).await? else {
        return Ok(false);
    };
    process_game_login_frame_payload_with_catalogs_typed_mut(
        stream,
        state,
        account,
        typed_account,
        frame,
        catalogs,
    )
    .await
}

fn current_unix_seconds() -> u32 {
    SystemClock.now()
}

#[cfg(test)]
mod tests;
