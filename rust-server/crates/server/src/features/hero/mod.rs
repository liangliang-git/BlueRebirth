//! Hero feature service, compatibility, and state boundary.

#[cfg(test)]
use crate::account_state::*;
use crate::common;
#[cfg(test)]
use crate::common::json::*;
use crate::*;
use blueoath_protocol::*;

#[path = "compat_service.rs"]
pub(crate) mod compat_service;
#[path = "service.rs"]
pub(crate) mod service;
#[path = "state.rs"]
pub(crate) mod state;
pub(crate) use state::*;
