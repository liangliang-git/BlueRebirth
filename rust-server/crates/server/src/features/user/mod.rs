//! User/account feature boundary.

use crate::common;
pub(crate) use crate::common::error::GameError;
use crate::*;
use blueoath_protocol::*;

#[path = "misc_service.rs"]
pub(crate) mod misc_service;
pub(crate) mod requests;
pub(crate) mod responses;
#[path = "service.rs"]
pub(crate) mod service;
#[path = "teaching_service.rs"]
pub(crate) mod teaching_service;
#[allow(unused_imports)]
pub(crate) use crate::account_defaults as state;
