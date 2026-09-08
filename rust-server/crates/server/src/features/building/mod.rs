//! Building, construction, outpost, and ship-building boundaries.

use crate::common;
use crate::*;
use blueoath_protocol::*;

#[path = "buildship_service.rs"]
pub(crate) mod buildship_service;
#[path = "outpost_service.rs"]
pub(crate) mod outpost_service;
#[path = "service.rs"]
pub(crate) mod service;
#[allow(unused_imports)]
pub(crate) use crate::{building_state, buildship_state, construction_state};
