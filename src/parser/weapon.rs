use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd, Default)]
pub enum Weapon {
    #[default]
    Unknown,
    RocketLauncher,
    Pistol,
    ScatterGun,
    Minigun,
    SniperRifle,
    Knife,
    Flamethrower,
    GrenadeLauncher,
    MediGun,
    ShotgunEngy,
    ShitgunHWG,
    ShotgunPyro,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct WeaponDetail {
    pub shots: u32,
    pub hits: u32,
    pub damage: u32,
    pub kills: u32,
}
