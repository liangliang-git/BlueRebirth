//! User/account feature boundary.

#[cfg(test)]
use crate::account_state::*;
use crate::common;
pub(crate) use crate::common::error::GameError;
#[cfg(test)]
use crate::common::json::*;
use crate::*;
use blueoath_protocol::*;

#[path = "misc_service.rs"]
pub(crate) mod misc_service;
pub(crate) mod requests;
pub(crate) mod responses;
#[path = "service.rs"]
pub(crate) mod service;
#[path = "state.rs"]
pub(crate) mod state;
#[path = "teaching_service.rs"]
pub(crate) mod teaching_service;
pub(crate) use state::*;
