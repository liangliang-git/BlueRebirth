//! Equipment feature service and state boundary.

use crate::common;
use crate::*;
use blueoath_protocol::*;

#[path = "service.rs"]
pub(crate) mod service;
#[path = "state.rs"]
pub(crate) mod state;
pub(crate) use state::*;
