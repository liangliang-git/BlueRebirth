//! Central game-login method classification.
//!
//! Handlers still receive the original wire name because it is part of the
//! client protocol. Prefix matching belongs here, at the routing boundary,
//! instead of being repeated throughout the dispatcher.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodFamily {
    Activity,
    ActivityBattlePass,
    ActivityTower,
    Adventure,
    Bathroom,
    Bag,
    Battle,
    BattlePass,
    BigActivity,
    Boss,
    Build,
    Building,
    BuildNotes,
    BuildShip,
    Chat,
    Copy,
    CopyInfo,
    DailyCopy,
    Discuss,
    Equip,
    EquipActivity,
    EquipNewTestCopy,
    EquipTestCopy,
    Exchange,
    FoodCompose,
    Friend,
    Fashion,
    Guide,
    Guild,
    GuildBox,
    GuildBigActivity,
    GuildOffer,
    GuildOfferRank,
    GuildTask,
    GuildWar,
    Hero,
    HeroAwaken,
    InteractionItem,
    InviteScore,
    Illustrate,
    Jopen,
    Magazine,
    Mail,
    MatchServer,
    Milestone,
    MopUp,
    Outpost,
    PresetFleet,
    Recharge,
    Room,
    ShipTask,
    Shop,
    SportsMeet,
    SportsMeetRank,
    Strategy,
    Study,
    Supply,
    SupportFleet,
    TalentTree,
    Task,
    TeachingServer,
    Tower,
    User,
    UserServer,
    WorldEvent,
    Unknown,
}

/// Exact protocol routes used by core handlers.
///
/// Family classification remains useful for feature modules, but exact routes
/// belong in one registry so the dispatcher does not duplicate wire strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownMethod {
    PlayerLogin,
    PlayerGetUserList,
    PlayerCreateUser,
    UserGetUserInfo,
    UserLogin,
    TacticGetHeros,
    TacticSetHeros,
    BagGetInfo,
    PresetFleetInfo,
    PresetFleetSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMethod<'a> {
    name: &'a str,
    family: MethodFamily,
}

impl<'a> GameMethod<'a> {
    pub fn parse(name: &'a str) -> Self {
        Self {
            name,
            family: family_for(name),
        }
    }

    pub fn name(self) -> &'a str {
        self.name
    }

    pub fn family(self) -> MethodFamily {
        self.family
    }

    pub fn is(self, name: &str) -> bool {
        self.name == name
    }

    pub fn is_family(self, family: MethodFamily) -> bool {
        self.family == family
    }

    pub fn is_known(self) -> bool {
        self.family != MethodFamily::Unknown || KNOWN_EXACT_METHODS.contains(&self.name)
    }

    pub fn known(self) -> Option<KnownMethod> {
        Some(match self.name {
            "player.Login" => KnownMethod::PlayerLogin,
            "player.GetUserList" => KnownMethod::PlayerGetUserList,
            "player.CreateUser" => KnownMethod::PlayerCreateUser,
            "user.GetUserInfo" => KnownMethod::UserGetUserInfo,
            "user.UserLogin" => KnownMethod::UserLogin,
            "tactic.GetHerosTactic" => KnownMethod::TacticGetHeros,
            "tactic.SetHerosTactic" => KnownMethod::TacticSetHeros,
            "bag.GetBagInfo" => KnownMethod::BagGetInfo,
            "presetfleet.PresetFleetsInfo" => KnownMethod::PresetFleetInfo,
            "presetfleet.SetPresetFleets" => KnownMethod::PresetFleetSet,
            _ => return None,
        })
    }
}

fn family_for(name: &str) -> MethodFamily {
    const PREFIXES: &[(&str, MethodFamily)] = &[
        ("activitybattlepass.", MethodFamily::ActivityBattlePass),
        ("activitybirthday.", MethodFamily::Activity),
        ("activitychristmasshop.", MethodFamily::Activity),
        ("activitycodeexchange.", MethodFamily::Activity),
        ("activityextractur.", MethodFamily::Activity),
        ("activityextract.", MethodFamily::Activity),
        ("activityfashion.", MethodFamily::Activity),
        ("activitypapercut.", MethodFamily::Activity),
        ("activitysecretcopy.", MethodFamily::Activity),
        ("activitySSRrolls.", MethodFamily::Activity),
        ("activitySSR.", MethodFamily::Activity),
        ("activityvalentineloveletter.", MethodFamily::Activity),
        ("activityVideo.", MethodFamily::Activity),
        ("activityTower.", MethodFamily::ActivityTower),
        ("activity.", MethodFamily::Activity),
        ("adventure.", MethodFamily::Adventure),
        ("bathroom.", MethodFamily::Bathroom),
        ("bag.", MethodFamily::Bag),
        ("battlepass.", MethodFamily::BattlePass),
        ("battle.", MethodFamily::Battle),
        ("bigactivity.", MethodFamily::BigActivity),
        ("boss.", MethodFamily::Boss),
        ("building.", MethodFamily::Building),
        ("buildnotes.", MethodFamily::BuildNotes),
        ("buildship.", MethodFamily::BuildShip),
        ("build.", MethodFamily::Build),
        ("chat.", MethodFamily::Chat),
        ("copy.", MethodFamily::Copy),
        ("copyinfo.", MethodFamily::CopyInfo),
        ("dailycopy.", MethodFamily::DailyCopy),
        ("discuss.", MethodFamily::Discuss),
        ("equipactivity.", MethodFamily::EquipActivity),
        ("equipnewtestcopy.", MethodFamily::EquipNewTestCopy),
        ("equiptestcopy.", MethodFamily::EquipTestCopy),
        ("equip.", MethodFamily::Equip),
        ("exchange.", MethodFamily::Exchange),
        ("foodCompose.", MethodFamily::FoodCompose),
        ("friend.", MethodFamily::Friend),
        ("fashion.", MethodFamily::Fashion),
        ("guide.", MethodFamily::Guide),
        ("guildbigactivityrank.", MethodFamily::GuildBigActivity),
        ("guildbigactivity.", MethodFamily::GuildBigActivity),
        ("guildofferrank.", MethodFamily::GuildOfferRank),
        ("guildOfferUser.", MethodFamily::GuildOffer),
        ("guildOffer.", MethodFamily::GuildOffer),
        ("guildtask.", MethodFamily::GuildTask),
        ("guildwar.", MethodFamily::GuildWar),
        ("guildbox.", MethodFamily::GuildBox),
        ("guild.", MethodFamily::Guild),
        ("heroawaken.", MethodFamily::HeroAwaken),
        ("hero.", MethodFamily::Hero),
        ("interactionitem.", MethodFamily::InteractionItem),
        ("invitescore.", MethodFamily::InviteScore),
        ("illustrate.", MethodFamily::Illustrate),
        ("jopen.", MethodFamily::Jopen),
        ("magazine.", MethodFamily::Magazine),
        ("mail.", MethodFamily::Mail),
        ("matchsvr_", MethodFamily::MatchServer),
        ("matchsvr.", MethodFamily::MatchServer),
        ("milestone.", MethodFamily::Milestone),
        ("mopUp.", MethodFamily::MopUp),
        ("outpost.", MethodFamily::Outpost),
        ("presetfleet.", MethodFamily::PresetFleet),
        ("recharge.", MethodFamily::Recharge),
        ("room.", MethodFamily::Room),
        ("shiptask.", MethodFamily::ShipTask),
        ("shop.", MethodFamily::Shop),
        ("sportsmeetrank.", MethodFamily::SportsMeetRank),
        ("sportsmeet.", MethodFamily::SportsMeet),
        ("strategy.", MethodFamily::Strategy),
        ("study.", MethodFamily::Study),
        ("supply.", MethodFamily::Supply),
        ("supportfleet.", MethodFamily::SupportFleet),
        ("talentTree.", MethodFamily::TalentTree),
        ("task.", MethodFamily::Task),
        ("teachingsvr.", MethodFamily::TeachingServer),
        ("tower.", MethodFamily::Tower),
        ("user.", MethodFamily::User),
        ("usersvr.", MethodFamily::UserServer),
        ("worldeventrank.", MethodFamily::WorldEvent),
        ("worldevent.", MethodFamily::WorldEvent),
    ];

    PREFIXES
        .iter()
        .find_map(|(prefix, family)| name.starts_with(prefix).then_some(*family))
        .unwrap_or(MethodFamily::Unknown)
}

const KNOWN_EXACT_METHODS: &[&str] = &[
    "GetSvrTime",
    "player.Login",
    "player.GetUserInfo",
    "player.GetUserList",
    "player.CreateUser",
    "cachedata.CacheData",
    "archiveCopy.IsLoad",
    "copyextra.AddCopyRewardCount",
    "copyextra.UpdateCopyExtraInfo",
    "prefs.SavePrefs",
    "statcount.GetStatCount",
    "sign.Sign",
    "miniGame.StartMiniGame",
    "alchemy.StartAlchemy",
    "repair.RepairHero",
];

#[cfg(test)]
mod tests {
    use super::{family_for, GameMethod, KnownMethod, MethodFamily};

    #[test]
    fn prefers_longer_prefixes_before_shorter_prefixes() {
        assert_eq!(family_for("guildtask.GetTasks"), MethodFamily::GuildTask);
        assert_eq!(
            family_for("sportsmeetrank.GetRank"),
            MethodFamily::SportsMeetRank
        );
        assert_eq!(family_for("matchsvr_1.Ready"), MethodFamily::MatchServer);
    }

    #[test]
    fn classifies_extended_protocol_names_at_one_boundary() {
        assert_eq!(family_for("activitybirthday.Get"), MethodFamily::Activity);
        assert_eq!(family_for("activitySSRrolls.Draw"), MethodFamily::Activity);
        assert_eq!(family_for("guildOfferUser.Get"), MethodFamily::GuildOffer);
        assert_eq!(family_for("worldeventrank.Rank"), MethodFamily::WorldEvent);
    }

    #[test]
    fn known_prefixes_share_same_family_registry() {
        for (name, family) in [
            ("bag.GetBagInfo", MethodFamily::Bag),
            ("copyinfo.GetCopyInfo", MethodFamily::CopyInfo),
            ("fashion.GetFashion", MethodFamily::Fashion),
            ("illustrate.GetInfo", MethodFamily::Illustrate),
            ("mail.GetMailList", MethodFamily::Mail),
        ] {
            let method = GameMethod::parse(name);
            assert_eq!(method.family(), family);
            assert!(method.is_known());
        }
    }

    #[test]
    fn exact_core_routes_use_one_registry() {
        assert_eq!(
            GameMethod::parse("user.GetUserInfo").known(),
            Some(KnownMethod::UserGetUserInfo)
        );
        assert_eq!(
            GameMethod::parse("presetfleet.SetPresetFleets").known(),
            Some(KnownMethod::PresetFleetSet)
        );
        assert_eq!(GameMethod::parse("user.NotARealMethod").known(), None);
    }
}
