use crate::common::response::{HandlerResult, ResponseEffects};
use crate::TaskCatalog;
use crate::{BattleCatalog, ChapterCatalog, ServerState};

mod context;
mod payload;
mod progression;
mod rewards;
mod settlement;
mod stars;
mod start;
mod sweep;

pub(crate) use context::TypedBattleContext;

pub(crate) fn handle_typed_with_catalog(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    context: TypedBattleContext<'_>,
) -> HandlerResult {
    match method {
        "copy.StartBase"
        | "copy.PvpStartBase"
        | "dailycopy.CopyEnter"
        | "copy.AttackBase"
        | "copy.QuitBase" => start::handle(account, method, request_args, context),

        "copy.PassBase" => settlement::handle_pass_base(method, account, request_args, context),

        "copy.PassMiniGame" => {
            progression::handle_pass_mini_game(method, account, request_args, context)
        }

        "copy.GetRecord"
        | "copy.DeleteRecord"
        | "copy.TacticOn"
        | "copyinfo.GetCopyInfo"
        | "copy.GetRandomFactors" => progression::handle(account, method, request_args, context),

        _ => HandlerResult::Empty,
    }
}

pub(crate) fn handle_typed_copy_star_reward(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    task_catalog: Option<&TaskCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    stars::handle(
        account,
        method,
        request_args,
        chapter_catalog,
        task_catalog,
        effects,
    )
}

pub(crate) fn handle_typed_mop_up(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    battle_catalog: Option<&BattleCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    sweep::handle(
        state,
        account,
        method,
        request_args,
        battle_catalog,
        effects,
    )
}
