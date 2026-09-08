//! Server feature modules.
//!
//! Wire dispatch lives in `game_login`; feature handlers live here so feature
//! boundaries do not depend on dispatcher module tree.

use crate::common::error::GameError;
use crate::router::{GameMethod, MethodFamily};
use crate::*;
use blueoath_protocol::*;

#[path = "activity/extra_service.rs"]
pub(crate) mod activity_extra_handler;
#[path = "activity/service.rs"]
pub(crate) mod activity_handler;
#[path = "activity/adventure_service.rs"]
pub(crate) mod adventure_handler;
#[path = "battle/service.rs"]
pub(crate) mod battle_handler;
#[path = "social/boss_service.rs"]
pub(crate) mod boss_handler;
#[path = "building/service.rs"]
pub(crate) mod building_handler;
#[path = "building/buildship_service.rs"]
pub(crate) mod buildship_handler;
#[path = "social/chat_service.rs"]
pub(crate) mod chat_handler;
#[path = "shop/service.rs"]
pub(crate) mod commerce_handler;
#[path = "hero/compat_service.rs"]
pub(crate) mod compat_feature;
#[path = "cooperation/service.rs"]
pub(crate) mod coop_handler;
#[path = "copy/service.rs"]
pub(crate) mod daily_copy_handler;
#[path = "equip/service.rs"]
pub(crate) mod equip_handler;
#[path = "activity/extended_service.rs"]
pub(crate) mod extended_handler;
#[path = "social/friend_service.rs"]
pub(crate) mod friend_handler;
#[path = "social/guild_extension_service.rs"]
pub(crate) mod guild_extension_handler;
#[path = "social/guild_service.rs"]
pub(crate) mod guild_handler;
#[path = "social/guild_box_service.rs"]
pub(crate) mod guildbox_handler;
#[path = "social/guild_task_service.rs"]
pub(crate) mod guildtask_handler;
#[path = "hero/service.rs"]
pub(crate) mod hero_handler;
#[path = "activity/invite_score_service.rs"]
pub(crate) mod invitescore_handler;
#[path = "activity/misc_service.rs"]
pub(crate) mod misc_extended_handler;
#[path = "building/outpost_service.rs"]
pub(crate) mod outpost_handler;
#[path = "progression/service.rs"]
pub(crate) mod progression_handler;
#[path = "activity/ship_task_service.rs"]
pub(crate) mod shiptask_handler;
#[path = "activity/sports_meet_service.rs"]
pub(crate) mod sportsmeet_handler;
#[path = "activity/talent_service.rs"]
pub(crate) mod talent_handler;
#[path = "task/service.rs"]
pub(crate) mod task_handler;
#[path = "tower/service.rs"]
pub(crate) mod tower_handler;

pub(crate) mod activity;
pub(crate) mod battle;
pub(crate) mod building;
pub(crate) mod cooperation;
pub(crate) mod copy;
pub(crate) mod equip;
pub(crate) mod hero;
pub(crate) mod progression;
pub(crate) mod shop;
pub(crate) mod social;
pub(crate) mod task;
pub(crate) mod tower;
pub(crate) mod user;
