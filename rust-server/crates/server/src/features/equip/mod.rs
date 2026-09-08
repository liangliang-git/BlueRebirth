//! Equipment feature service and state boundary.

use crate::common;
use crate::*;
use blueoath_protocol::*;

#[path = "service.rs"]
pub(crate) mod service;
#[allow(unused_imports)]
pub(crate) use crate::equip_state as state;
