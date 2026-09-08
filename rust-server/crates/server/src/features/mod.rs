//! Server feature modules.
//!
//! Wire dispatch lives in `game_login`; feature handlers live here so feature
//! boundaries do not depend on dispatcher module tree.

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
