use super::super::*;

use crate::common::response::ResponseEffects;

pub(crate) struct TypedBattleContext<'a> {
    pub(crate) battle_catalog: Option<&'a BattleCatalog>,
    pub(crate) fashion_catalog: Option<&'a FashionList>,
    pub(crate) hero_level_catalog: Option<&'a HeroLevelCatalog>,

    pub(crate) drop_multiplier: f64,
    pub(crate) ship_stat_multiplier: f64,
    pub(crate) commander_exp_multiplier: f64,
    pub(crate) ship_exp_multiplier: f64,
    pub(crate) affection_multiplier: f64,

    pub(crate) server_state: Option<&'a ServerState>,
    pub(crate) effects: &'a mut ResponseEffects,
}

impl<'a> TypedBattleContext<'a> {
    pub(crate) fn new(
        battle_catalog: Option<&'a BattleCatalog>,
        fashion_catalog: Option<&'a FashionList>,
        hero_level_catalog: Option<&'a HeroLevelCatalog>,
        drop_multiplier: f64,
        ship_stat_multiplier: f64,
        commander_exp_multiplier: f64,
        ship_exp_multiplier: f64,
        effects: &'a mut ResponseEffects,
    ) -> Self {
        Self {
            battle_catalog,
            fashion_catalog,
            hero_level_catalog,
            drop_multiplier,
            ship_stat_multiplier,
            commander_exp_multiplier,
            ship_exp_multiplier,
            affection_multiplier: 1.0,
            server_state: None,
            effects,
        }
    }

    pub(crate) fn with_affection_multiplier(mut self, multiplier: f64) -> Self {
        self.affection_multiplier = multiplier;
        self
    }

    pub(crate) fn with_server_state(mut self, state: &'a ServerState) -> Self {
        self.server_state = Some(state);
        self
    }
}
