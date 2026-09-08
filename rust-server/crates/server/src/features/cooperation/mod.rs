//! Cooperative room and battle boundary.

use crate::common;
pub(crate) use crate::router::{GameMethod, MethodFamily};
use crate::*;
use blueoath_protocol::*;

#[path = "service.rs"]
pub(crate) mod service;
