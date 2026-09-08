#![recursion_limit = "256"]

use std::io;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

#[cfg(test)]
pub(crate) use blueoath_protocol::EquipNum;
#[cfg(test)]
use blueoath_protocol::MedalAcquiredTime;
use blueoath_protocol::{
    BagInfoCodec, BuildingSetHeroListRequest, ClientGameWireCodec, CopyStartRequest, Decode,
    EquipListCodec, FashionInfo, FashionList, FashionListCodec, HeroBag, HeroBagCodec, HeroGrid,
    MailIdRequest, MopUpRequest, TMessageCodec, TResponse, TRetLogin, TalentIdRequest, UserInfo,
    UserInfoCodec,
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

#[cfg(test)]
mod account_state;
mod bootstrap;
mod catalog;
mod catalog_loader;
pub mod common;
mod config;
pub(crate) mod features;
mod frame_service;
mod game_login;
#[cfg(test)]
pub(crate) use features::equip::service::{
    equip_activity_payload, equip_new_test_copy_payload, equip_test_copy_payload,
    mark_equip_activity_reward, mark_new_test_reward, resolve_new_test_reward,
};
#[path = "features/hero/state.rs"]
mod hero_state;
mod local_protocol;
mod projection;
#[path = "common/protocol_payload.rs"]
mod protocol_payload;
pub mod router;
mod runtime;
#[path = "common/wire.rs"]
mod wire;

#[cfg(test)]
use account_state::*;
pub use blueoath_game::{
    BattleService, BattleStartContext, ProgressService, ResourceService, RewardService,
};
#[cfg(test)]
pub(crate) use bootstrap::bootstrap_response;
use bootstrap::{
    handle_bootstrap_http, looks_like_http, looks_like_netsocket, read_connection_prefix,
    PrefixedTcpStream,
};
use catalog::*;
use catalog_loader::*;
use common::clock::{Clock, SystemClock};
use common::json::*;
use config::{normalize_multiplier, normalize_profile_id, scale_reward, DEFAULT_PROFILE_ID};
pub use config::{
    BattleOutcome, Formation, ServerConfig, ServerConfigError, ServerState, Ship, Stage,
};
use features::battle::state::*;
#[cfg(test)]
pub(crate) use features::building::service::handle_typed as handle_typed_building;
use features::building::{buildship_state::*, construction_state::*, state::*};
use features::copy::mopup_state::mop_up_pass_rets;
#[cfg(test)]
use features::copy::mopup_state::*;
#[cfg(test)]
use features::equip::state::*;
#[cfg(test)]
use features::progression::study_state::*;
use features::shop::state::*;
#[cfg(test)]
use features::social::state::*;
use features::task::state::*;
#[cfg(test)]
pub(crate) use features::user::state::default_account_snapshot;
#[cfg(test)]
use features::user::state::user_info_from_account;
use features::user::state::user_info_from_typed_account;
pub use frame_service::process_frame;
use frame_service::{prepare_local_request, storage_failure_response};
#[cfg(test)]
use game_login::process_game_login_frame_payload_with_catalogs_typed_mut;
use game_login::process_game_login_frame_payload_with_typed_account;
use hero_state::*;
pub use local_protocol::dispatch;
use projection::*;
use protocol_payload::*;
#[cfg(test)]
use wire::skip_wire;
use wire::{
    append_bytes_field, append_message_field, append_varint, append_varint_field, read_varint,
};

pub(crate) use runtime::current_unix_millis;
#[cfg(test)]
pub(crate) use runtime::normalize_task_state;
pub use runtime::run;

const SEA_DIFFICULTY_UNLOCK_LEVEL: i32 = 60;
#[cfg(test)]
const INITIAL_SUPPLY: i64 = 10_000;
// Client mood values use fixed-point units with a scale of 10,000.
#[cfg(test)]
const MOOD_MIN: i32 = 0;
const MOOD_MAX: i32 = 1_500_000;
const MOOD_INITIAL: i32 = blueoath_domain::HERO_MOOD_INITIAL as i32;
#[cfg(test)]
const MOOD_NORMAL_LIMIT: i32 = 1_190_000;
#[cfg(test)]
const MOOD_RECOVERY_INTERVAL_SECONDS: i64 = 6 * 60;
#[cfg(test)]
const MOOD_NORMAL_RECOVERY: i32 = 100;
#[cfg(test)]
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
    let mut account = transient_typed_account(state);
    let catalogs = GameLoginCatalogs::empty();
    process_game_login_frame_with_typed_account_and_catalogs(stream, state, &mut account, &catalogs)
        .await
}

fn transient_typed_account(state: &ServerState) -> blueoath_domain::AccountState {
    let profile_id = blueoath_domain::ProfileId::new(state.profile_id.clone())
        .unwrap_or_else(|_| blueoath_domain::ProfileId::new("anonymous").expect("static id"));
    let mut account = blueoath_domain::NewAccountFactory::create(profile_id, &state.name);
    let _ = account
        .resources
        .debit(blueoath_domain::CurrencyKind::Supply, 9_900);
    let _ = account
        .resources
        .credit(blueoath_domain::CurrencyKind::PvePoint, 100);
    account
}

pub async fn process_game_login_frame_with_typed_account<S>(
    stream: &mut S,
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let catalogs = GameLoginCatalogs::empty();
    process_game_login_frame_with_typed_account_and_catalogs(stream, state, account, &catalogs)
        .await
}

pub(crate) async fn process_game_login_frame_with_typed_account_and_catalogs<S>(
    stream: &mut S,
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(frame) = NetSocketFrameCodec::read(stream).await? else {
        return Ok(false);
    };
    process_game_login_frame_payload_with_typed_account(stream, state, account, frame, catalogs)
        .await
}

#[cfg(test)]
async fn process_game_login_frame_with_catalogs_typed_mut<S>(
    stream: &mut S,
    state: &ServerState,
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
