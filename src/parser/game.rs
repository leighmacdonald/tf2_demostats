use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, IntoPrimitive, TryFromPrimitive, PartialEq, Debug, Default)]
#[repr(u16)]
pub enum RoundState {
    #[default]
    Init = 0,
    Pregame = 1,
    StartGame = 2,
    PreRound = 3,
    RoundRunning = 4,
    TeamWin = 5,
    Restart = 6,
    Stalemate = 7,
    GameOver = 8,
    Bonus = 9,
    BetweenRounds = 10,
}

pub const DEATH_FEIGNED: u16 = 0x0020;

pub const ENTITY_ON_GROUND: u16 = 1;
pub const ENTITY_IN_WATER: u16 = 1 << 9;

#[derive(Deserialize, Serialize, IntoPrimitive, TryFromPrimitive, PartialEq, Debug)]
#[repr(u16)]
pub enum DamageType {
    Normal = 0,
    Headshot = 1,
    Backstab = 2,
    Burning = 3,
    WrenchFix = 4,
    Minigun = 5,
    Suicide = 6,
    TauntHadouken = 7,
    BurningFlare = 8,
    TauntHighNoon = 9,
    TauntGrandSlam = 10,
    PenetrateMyTeam = 11,
    PenetrateAllPlayers = 12,
    TauntFencing = 13,
    PenetrateNonburningTeammate = 14,
    TauntArrowStab = 15,
    Telefrag = 16,
    BurningArrow = 17,
    Flyingburn = 18,
    PumpkinBomb = 19,
    Decapitation = 20,
    TauntGrenade = 21,
    Baseball = 22,
    ChargeImpact = 23,
    TauntBarbarianSwing = 24,
    AirStickyBurst = 25,
    DefensiveSticky = 26,
    Pickaxe = 27,
    RocketDirecthit = 28,
    TauntUberslice = 29,
    PlayerSentry = 30,
    StandardSticky = 31,
    ShotgunRevengeCrit = 32,
    TauntEngineerGuitarSmash = 33,
    Bleeding = 34,
    GoldWrench = 35,
    CarriedBuilding = 36,
    ComboPunch = 37,
    TauntEngineerArmKill = 38,
    FishKill = 39,
    TriggerHurt = 40,
    DecapitationBoss = 41,
    StickbombExplosion = 42,
    AegisRound = 43,
    FlareExplosion = 44,
    BootsStomp = 45,
    Plasma = 46,
    PlasmaCharged = 47,
    PlasmaGib = 48,
    PracticeSticky = 49,
    EyeballRocket = 50,
    HeadshotDecapitation = 51,
    TauntArmageddon = 52,
    FlarePellet = 53,
    Cleaver = 54,
    CleaverCrit = 55,
    SapperRecorderDeath = 56,
    MerasmusPlayerBomb = 57,
    MerasmusGrenade = 58,
    MerasmusZap = 59,
    MerasmusDecapitation = 60,
    CannonballPush = 61,
    TauntAllclassGuitarRiff = 62,
    Throwable = 63,
    ThrowableKill = 64,
    SpellTeleport = 65,
    SpellSkeleton = 66,
    SpellMirv = 67,
    SpellMeteor = 68,
    SpellLightning = 69,
    SpellFireball = 70,
    SpellMonoculus = 71,
    SpellBlastjump = 72,
    SpellBats = 73,
    SpellTiny = 74,
    Kart = 75,
    GiantHammer = 76,
    RuneReflect = 77,
    DragonsFuryIgnite = 78,
    DragonsFuryBonusBurning = 79,
    SlapKill = 80,
    Croc = 81,
    TauntGasBlast = 82,
    AxtinguisherBoosted = 83,
}

#[derive(Deserialize, Serialize, IntoPrimitive, TryFromPrimitive, PartialEq, Debug)]
#[repr(u16)]
pub enum WeaponId {
    WeaponNone = 0,
    WeaponBat = 1,
    WeaponBatWood = 2,
    WeaponBottle = 3,
    WeaponFireaxe = 4,
    WeaponClub = 5,
    WeaponCrowbar = 6,
    WeaponKnife = 7,
    WeaponFists = 8,
    WeaponShovel = 9,
    WeaponWrench = 10,
    WeaponBonesaw = 11,
    WeaponShotgunPrimary = 12,
    WeaponShotgunSoldier = 13,
    WeaponShotgunHwg = 14,
    WeaponShotgunPyro = 15,
    WeaponScattergun = 16,
    WeaponSniperrifle = 17,
    WeaponMinigun = 18,
    WeaponSmg = 19,
    WeaponSyringegunMedic = 20,
    WeaponTranq = 21,
    WeaponRocketlauncher = 22,
    WeaponGrenadelauncher = 23,
    WeaponPipebomblauncher = 24,
    WeaponFlamethrower = 25,
    WeaponGrenadeNormal = 26,
    WeaponGrenadeConcussion = 27,
    WeaponGrenadeNail = 28,
    WeaponGrenadeMirv = 29,
    WeaponGrenadeMirvDemoman = 30,
    WeaponGrenadeNapalm = 31,
    WeaponGrenadeGas = 32,
    WeaponGrenadeEmp = 33,
    WeaponGrenadeCaltrop = 34,
    WeaponGrenadePipebomb = 35,
    WeaponGrenadeSmokeBomb = 36,
    WeaponGrenadeHeal = 37,
    WeaponGrenadeStunball = 38,
    WeaponGrenadeJar = 39,
    WeaponGrenadeJarMilk = 40,
    WeaponPistol = 41,
    WeaponPistolScout = 42,
    WeaponRevolver = 43,
    WeaponNailgun = 44,
    WeaponPda = 45,
    WeaponPdaEngineerBuild = 46,
    WeaponPdaEngineerDestroy = 47,
    WeaponPdaSpy = 48,
    WeaponBuilder = 49,
    WeaponMedigun = 50,
    WeaponGrenadeMirvbomb = 51,
    WeaponFlamethrowerRocket = 52,
    WeaponGrenadeDemoman = 53,
    WeaponSentryBullet = 54,
    WeaponSentryRocket = 55,
    WeaponDispenser = 56,
    WeaponInvis = 57,
    WeaponFlaregun = 58,
    WeaponLunchbox = 59,
    WeaponJar = 60,
    WeaponCompoundBow = 61,
    WeaponBuffItem = 62,
    WeaponPumpkinBomb = 63,
    WeaponSword = 64,
    WeaponRocketlauncherDirecthit = 65,
    WeaponLifeline = 66,
    WeaponLaserPointer = 67,
    WeaponDispenserGun = 68,
    WeaponSentryRevenge = 69,
    WeaponJarMilk = 70,
    WeaponHandgunScoutPrimary = 71,
    WeaponBatFish = 72,
    WeaponCrossbow = 73,
    WeaponStickbomb = 74,
    WeaponHandgunScoutSecondary = 75,
    WeaponSodaPopper = 76,
    WeaponSniperrifleDecap = 77,
    WeaponRaygun = 78,
    WeaponParticleCannon = 79,
    WeaponMechanicalArm = 80,
    WeaponDrgPomson = 81,
    WeaponBatGiftwrap = 82,
    WeaponGrenadeOrnamentBall = 83,
    WeaponFlaregunRevenge = 84,
    WeaponPepBrawlerBlaster = 85,
    WeaponCleaver = 86,
    WeaponGrenadeCleaver = 87,
    WeaponStickyBallLauncher = 88,
    WeaponGrenadeStickyBall = 89,
    WeaponShotgunBuildingRescue = 90,
    WeaponCannon = 91,
    WeaponThrowable = 92,
    WeaponGrenadeThrowable = 93,
    WeaponPdaSpyBuild = 94,
    WeaponGrenadeWaterballoon = 95,
    WeaponHarvesterSaw = 96,
    WeaponSpellbook = 97,
    WeaponSpellbookProjectile = 98,
    WeaponSniperrifleClassic = 99,
    WeaponParachute = 100,
    WeaponGrapplinghook = 101,
    WeaponPasstimeGun = 102,
    WeaponSniperrifleRevolver = 103,
    WeaponChargedSmg = 104,
}

#[repr(u8)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TryFromPrimitive)]
pub enum PlayerAnim {
    AttackPrimary,
    AttackSecondary,
    AttackGrenade,
    Reload,
    ReloadLoop,
    ReloadEnd,
    Jump,
    Swim,
    Die,
    FlinchChest,
    FlinchHead,
    FlinchLeftarm,
    FlinchRightarm,
    FlinchLeftleg,
    FlinchRightleg,
    Doublejump,
    Cancel,
    Spawn,
    SnapYaw,
    Custom, // Used to play specific activities
    CustomGesture,
    CustomSequence, // Used to play specific sequences
    CustomGestureSequence,
    AttackPre,
    AttackPost,
    Grenade1Draw,
    Grenade2Draw,
    Grenade1Throw,
    Grenade2Throw,
    VoiceCommandGesture,
    DoublejumpCrouch,
    StunBegin,
    StunMiddle,
    StunEnd,
    PasstimeThrowBegin,
    PasstimeThrowMiddle,
    PasstimeThrowEnd,
    PasstimeThrowCancel,
    AttackPrimarySuper,
}
