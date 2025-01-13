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
