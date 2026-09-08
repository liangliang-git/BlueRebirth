//! Building, construction, outpost, and ship-building boundaries.

use crate::common;
use crate::*;
use blueoath_protocol::*;

#[path = "buildship_service.rs"]
pub(crate) mod buildship_service;
#[path = "buildship_state.rs"]
pub(crate) mod buildship_state;
#[path = "construction.rs"]
pub(crate) mod construction_state;
#[path = "outpost_service.rs"]
pub(crate) mod outpost_service;
#[path = "service.rs"]
pub(crate) mod service;
#[path = "state.rs"]
pub(crate) mod state;
pub(crate) use buildship_state::*;
pub(crate) use construction_state::*;
pub(crate) use state::*;
