//! Hero feature service, compatibility, and state boundary.

pub(crate) use super::{compat_feature as compat_service, hero_handler as service};
#[allow(unused_imports)]
pub(crate) use crate::hero_state as state;
