//! Activity feature services.

use crate::common;
pub(crate) use crate::common::error::GameError;
pub(crate) use crate::router::{GameMethod, MethodFamily};
use crate::*;
use blueoath_protocol::*;

#[path = "adventure_service.rs"]
pub(crate) mod adventure_service;
#[path = "extended_service.rs"]
pub(crate) mod extended_service;
#[path = "extra_service.rs"]
pub(crate) mod extra_service;
#[path = "invite_score_service.rs"]
pub(crate) mod invite_score_service;
#[path = "misc_service.rs"]
pub(crate) mod misc_service;
#[path = "service.rs"]
pub(crate) mod service;
#[path = "ship_task_service.rs"]
pub(crate) mod ship_task_service;
#[path = "sports_meet_service.rs"]
pub(crate) mod sports_meet_service;
#[path = "talent_service.rs"]
pub(crate) mod talent_handler;
