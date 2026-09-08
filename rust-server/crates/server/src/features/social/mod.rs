//! Social feature boundaries: friends, chat, guild, and boss activity.

use crate::common;
pub(crate) use crate::common::error::GameError;
pub(crate) use crate::router::{GameMethod, MethodFamily};
use crate::*;
use blueoath_protocol::*;

#[path = "boss_service.rs"]
pub(crate) mod boss_handler;
#[path = "chat_service.rs"]
pub(crate) mod chat_handler;
#[path = "friend_service.rs"]
pub(crate) mod friend_handler;
#[path = "guild_extension_service.rs"]
pub(crate) mod guild_extension_handler;
#[path = "guild_service.rs"]
pub(crate) mod guild_handler;
#[path = "guild_box_service.rs"]
pub(crate) mod guildbox_handler;
#[path = "guild_task_service.rs"]
pub(crate) mod guildtask_handler;
#[path = "state.rs"]
pub(crate) mod state;
pub(crate) use state::*;
