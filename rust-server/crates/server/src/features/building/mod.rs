//! Building, construction, outpost, and ship-building boundaries.

pub(crate) use super::{
    building_handler as service, buildship_handler as buildship_service,
    outpost_handler as outpost_service,
};
#[allow(unused_imports)]
pub(crate) use crate::{building_state, buildship_state, construction_state};
