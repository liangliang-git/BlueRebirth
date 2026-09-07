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
    DailyCopy,
    Discuss,
    Equip,
    EquipActivity,
    EquipNewTestCopy,
    EquipTestCopy,
    Exchange,
    FoodCompose,
    Friend,
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
    Jopen,
    Magazine,
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
    Unknown,
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
        self.family != MethodFamily::Unknown
            || KNOWN_EXACT_METHODS.contains(&self.name)
            || KNOWN_PREFIXES
                .iter()
                .any(|prefix| self.name.starts_with(prefix))
    }
}

fn family_for(name: &str) -> MethodFamily {
    const PREFIXES: &[(&str, MethodFamily)] = &[
        ("activitybattlepass.", MethodFamily::ActivityBattlePass),
        ("activityTower.", MethodFamily::ActivityTower),
        ("activity.", MethodFamily::Activity),
        ("adventure.", MethodFamily::Adventure),
        ("bathroom.", MethodFamily::Bathroom),
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
        ("dailycopy.", MethodFamily::DailyCopy),
        ("discuss.", MethodFamily::Discuss),
        ("equipactivity.", MethodFamily::EquipActivity),
        ("equipnewtestcopy.", MethodFamily::EquipNewTestCopy),
        ("equiptestcopy.", MethodFamily::EquipTestCopy),
        ("equip.", MethodFamily::Equip),
        ("exchange.", MethodFamily::Exchange),
        ("foodCompose.", MethodFamily::FoodCompose),
        ("friend.", MethodFamily::Friend),
        ("guide.", MethodFamily::Guide),
        ("guildbigactivityrank.", MethodFamily::GuildBigActivity),
        ("guildbigactivity.", MethodFamily::GuildBigActivity),
        ("guildofferrank.", MethodFamily::GuildOfferRank),
        ("guildOffer.", MethodFamily::GuildOffer),
        ("guildtask.", MethodFamily::GuildTask),
        ("guildwar.", MethodFamily::GuildWar),
        ("guildbox.", MethodFamily::GuildBox),
        ("guild.", MethodFamily::Guild),
        ("heroawaken.", MethodFamily::HeroAwaken),
        ("hero.", MethodFamily::Hero),
        ("interactionitem.", MethodFamily::InteractionItem),
        ("invitescore.", MethodFamily::InviteScore),
        ("jopen.", MethodFamily::Jopen),
        ("magazine.", MethodFamily::Magazine),
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

const KNOWN_PREFIXES: &[&str] = &[
    "illustrate.",
    "bag.",
    "fashion.",
    "mail.",
    "copyinfo.",
    "recharge.",
    "activitybirthday.",
    "activitychristmasshop.",
    "activitycodeexchange.",
    "activityextract.",
    "activityextractur.",
    "activityfashion.",
    "activitypapercut.",
    "activitysecretcopy.",
    "activitySSR.",
    "activitySSRrolls.",
    "activityvalentineloveletter.",
    "activityVideo.",
    "worldevent.",
    "worldeventrank.",
];

#[cfg(test)]
mod tests {
    use super::{family_for, MethodFamily};

    #[test]
    fn prefers_longer_prefixes_before_shorter_prefixes() {
        assert_eq!(family_for("guildtask.GetTasks"), MethodFamily::GuildTask);
        assert_eq!(
            family_for("sportsmeetrank.GetRank"),
            MethodFamily::SportsMeetRank
        );
        assert_eq!(family_for("matchsvr_1.Ready"), MethodFamily::MatchServer);
    }
}
