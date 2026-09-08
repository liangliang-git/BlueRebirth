//! User/account feature boundary.

pub(crate) mod requests;
pub(crate) mod responses;
pub(crate) use super::teaching_handler as teaching_service;
pub(crate) use super::{base_handler as service, misc_handler as misc_service};
#[allow(unused_imports)]
pub(crate) use crate::account_defaults as state;
