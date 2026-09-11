//! Battle feature service and state boundary.

use crate::*;
use blueoath_protocol::*;

#[path = "state.rs"]
pub(crate) mod state;
pub(crate) use state::*;

pub(crate) mod service;
