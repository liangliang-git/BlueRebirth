use super::*;

pub(crate) fn legacy_only_method(method: &str) -> bool {
    if coop_handler::handles_typed(method) {
        return false;
    }
    if activity_handler::handles_typed(method) || compat_feature::handles_typed(method) {
        return false;
    }
    if misc_handler::handles_typed(method) {
        return false;
    }
    if method == "copy.PassMiniGame" {
        return false;
    }
    if matches!(
        method,
        "copy.StarReward" | "copy.FetchRewardBox" | "copy.DotBase" | "copyinfo.DotBase"
    ) {
        return false;
    }
    if matches!(
        method,
        "copy.AttackBase"
            | "copy.GetCopy"
            | "copy.PassBase"
            | "copy.PvpStartBase"
            | "copy.StartBase"
            | "copy.QuitBase"
            | "copy.GetRandomFactors"
            | "copy.ChooseSfLv"
            | "copy.UnLockCopy"
            | "copy.GetRecord"
            | "copy.DeleteRecord"
            | "copy.TacticOn"
            | "copyinfo.GetCopyInfo"
            | "dailycopy.CopyEnter"
            | "dailycopy.GetData"
            | "dailycopy.SelectEx"
            | "dailycopy.UpdateDailyCopyData"
            | "bag.GetBagInfo"
            | "player.CreateUser"
            | "player.GetUserList"
            | "player.Login"
            | "tactic.GetHerosTactic"
            | "tactic.SetHerosTactic"
    ) {
        return false;
    }
    if activity_handler::handles(method) {
        return true;
    }
    if GameMethod::parse(method).family() == MethodFamily::Unknown {
        return true;
    }
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::MatchServer
            | MethodFamily::Room
            | MethodFamily::Battle
            | MethodFamily::Copy
            | MethodFamily::DailyCopy
    )
}
