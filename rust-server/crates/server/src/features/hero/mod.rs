//! Hero feature service, compatibility, and state boundary.

use crate::common;
use crate::*;
use blueoath_protocol::*;

#[path = "compat_service.rs"]
pub(crate) mod compat_service;
#[path = "service.rs"]
pub(crate) mod service;
#[allow(unused_imports)]
pub(crate) use crate::hero_state as state;
