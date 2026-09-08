//! Server feature modules.
//!
//! Wire dispatch lives in `game_login`; feature handlers live here so feature
//! boundaries do not depend on dispatcher module tree.

#[allow(unused_imports)]
use crate::router::{GameMethod, MethodFamily};
use crate::*;
use blueoath_protocol::*;

#[path = "building/service.rs"]
pub(crate) mod building_handler;
#[path = "building/buildship_service.rs"]
pub(crate) mod buildship_handler;
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
#[path = "hero/service.rs"]
pub(crate) mod hero_handler;
#[path = "building/outpost_service.rs"]
pub(crate) mod outpost_handler;
#[path = "progression/service.rs"]
pub(crate) mod progression_handler;
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
