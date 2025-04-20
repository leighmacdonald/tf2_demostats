use tf_demo_parser::demo::sendprop::SendPropIdentifier;

pub const MEDIGUN_CHARGE_LEVEL: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponMedigunDataNonLocal", "m_flChargeLevel");
pub const MEDIGUN_CHARGE_RELEASED: SendPropIdentifier =
    SendPropIdentifier::new("DT_WeaponMedigun", "m_bChargeRelease");
pub const SELF_HANDLE: SendPropIdentifier =
    SendPropIdentifier::new("DT_AttributeContainer", "m_hOuter");
pub const ITEM_DEFINITION: SendPropIdentifier =
    SendPropIdentifier::new("DT_ScriptCreatedItem", "m_iItemDefinitionIndex");

pub const MODEL: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_nModelIndex");
pub const TEAM: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_iTeamNum");
pub const SIM_TIME: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseEntity", "m_flSimulationTime");
pub const ORIGIN: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_vecOrigin");
pub const OWNER: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_hOwnerEntity");
pub const EFFECTS: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_fEffects");

pub const BUILDER: SendPropIdentifier = SendPropIdentifier::new("DT_BaseObject", "m_hBuilder");
pub const UPGRADE_LEVEL: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseObject", "m_iUpgradeLevel");

// This is the max upgrade the sentry ever reached (ie if it goes down via red tape
// recorder). *Not* the max possible level.
pub const _MAX_UPGRADE_LEVEL: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseObject", "m_iHighestUpgradeLevel");

pub const OBJECT_MAX_HEALTH: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseObject", "m_iMaxHealth");

pub const ORIGIN_XY: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin");
pub const ORIGIN_Z: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin[2]");

pub const FLAGS: SendPropIdentifier = SendPropIdentifier::new("DT_BasePlayer", "m_fFlags");
pub const HEALTH: SendPropIdentifier = SendPropIdentifier::new("DT_BasePlayer", "m_iHealth");
pub const CLASS: SendPropIdentifier = SendPropIdentifier::new("DT_TFPlayerClassShared", "m_iClass");

pub const EYE_X: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[0]");
pub const EYE_Y: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[1]");

pub const HANDLE: SendPropIdentifier = SendPropIdentifier::new("DT_AttributeManager", "m_hOuter");

pub const COND_0: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCond");
pub const COND_1: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx");
pub const COND_2: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx2");
pub const COND_3: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx3");

// Separate condition bits
// TODO: seems to only be used for Kritzkreig?
pub const COND_BITS: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerConditionListExclusive", "_condition_bits");

pub const COND_SOURCE: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerConditionSource", "m_pProvider");

pub const ACTIVE_WEAPON_HANDLE: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseCombatCharacter", "m_hActiveWeapon");

// These DT_TFPlayerScoringDataExclusive props are only present in PoV demos, not STV demos.
//
// Other fields: m_iDominations, m_iRevenge,
//  m_iHeadshots, m_iBackstabs, m_iHealPoints, m_iInvulns,
//  m_iTeleports, m_iDamageDone, m_iBonusPoints, m_iPoints,
//  m_iCaptures, m_iDefenses
pub const KILLS: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iKills");
pub const DEATHS: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iDeaths");
pub const KILL_ASSISTS: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iKillAssists");
// These are stuck at 0 in our corpus of STVs?
// pub const BUILDINGS_BUILT: SendPropIdentifier =
//     SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iBuildingsBuilt");
// pub const BUILDINGS_DESTROYED: SendPropIdentifier =
//     SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iBuildingsDestroyed");

pub const WEP_0: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "000");
pub const WEP_1: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "001");
pub const WEP_2: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "002");
pub const WEP_3: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "003");
pub const WEP_4: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "004");
pub const WEP_5: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "005");
pub const WEP_6: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "006");

pub const NUM_COSMETICS: SendPropIdentifier =
    SendPropIdentifier::new("_LPT_m_hMyWearables_8", "lengthprop8");

pub const COSMETIC_0: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "000");
pub const COSMETIC_1: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "001");
pub const COSMETIC_2: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "002");
pub const COSMETIC_3: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "003");
pub const COSMETIC_4: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "004");
pub const COSMETIC_5: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "005");
pub const COSMETIC_6: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "006");
pub const COSMETIC_7: SendPropIdentifier = SendPropIdentifier::new("_ST_m_hMyWearables_8", "007");

pub const RESET_PARITY: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponBase", "m_bResetParity");

// DT_TFBaseRocket is for most rockets, arrows, and maybe more?
pub const ROCKET_ORIGIN: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFBaseRocket", "m_vecOrigin");
pub const ROCKET_DEFLECTED: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFBaseRocket", "m_iDeflected");

pub const GRENADE_ORIGIN: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_vecOrigin");
pub const GRENADE_DEFLECTED: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_iDeflected");

pub const WEAPON_OWNER: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseCombatWeapon", "m_hOwner");

pub const INITIAL_SPEED: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFBaseRocket", "m_vInitialVelocity");
pub const ORIGINAL_LAUNCHER: SendPropIdentifier =
    SendPropIdentifier::new("DT_BaseProjectile", "m_hOriginalLauncher");
pub const PIPE_TYPE: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFProjectile_Pipebomb", "m_iType");
pub const ROCKET_ROTATION: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFBaseRocket", "m_angRotation");
pub const GRENADE_ROTATION: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_angRotation");
pub const DEFLECT_OWNER: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_hDeflectOwner");

pub const WAITING_FOR_PLAYERS: SendPropIdentifier =
    SendPropIdentifier::new("DT_TeamplayRoundBasedRules", "m_bInWaitingForPlayers");
pub const ROUND_STATE: SendPropIdentifier =
    SendPropIdentifier::new("DT_TeamplayRoundBasedRules", "m_iRoundState");

// Temp entities
pub const EFFECT_ENTITY: SendPropIdentifier = SendPropIdentifier::new("DT_EffectData", "entindex");
pub const ANIM_PLAYER: SendPropIdentifier =
    SendPropIdentifier::new("DT_TEPlayerAnimEvent", "m_hPlayer");
pub const ANIM_ID: SendPropIdentifier = SendPropIdentifier::new("DT_TEPlayerAnimEvent", "m_iEvent");

pub const FIRE_BULLETS_PLAYER: SendPropIdentifier =
    SendPropIdentifier::new("DT_TEFireBullets", "m_iPlayer");
