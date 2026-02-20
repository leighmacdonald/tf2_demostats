use crate::{
    parser::{
        entity::{self, ProjectileType},
        game::{DamageType, GrenadeType},
    },
    schema,
};
use serde::{Deserialize, Serialize};
use tf_demo_parser::demo::parser::analyser::{Class, Team};
use tracing::{error, trace};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd, Default)]
#[allow(dead_code)]
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

pub fn strip_prefix(killer_weapon_name: &str) -> &str {
    let prefixes = ["tf_weapon_grenade_", "tf_weapon_", "NPC_", "func_"];

    for prefix in prefixes {
        if let Some(weapon) = killer_weapon_name.strip_prefix(prefix) {
            return weapon;
        }
    }

    killer_weapon_name
}

pub fn sentry_name(sentry: &entity::Sentry) -> &'static str {
    if sentry.is_mini {
        return "obj_minisentry";
    }
    match sentry.level {
        1 => "obj_sentrygun",
        2 => "obj_sentrygun2",
        3 => "obj_sentrygun3",
        _ => {
            error!("Unexpected sentry gun type {sentry:?}");
            "obj_sentrygun"
        }
    }
}

pub fn is_sentry(name: &'static str) -> bool {
    matches!(
        name,
        "obj_minisentry" | "obj_sentrygun" | "obj_sentrygun2" | "obj_sentrygun3"
    )
}

pub fn weapon_name(weapon: &schema::Item, class: Class) -> &'static str {
    weapon
        .item_logname
        .as_ref()
        .map(|s| ustr::ustr(s).as_str())
        .or(weapon
            .item_class
            .as_deref()
            .map(strip_prefix)
            .map(|s| ustr::ustr(s).as_str()))
        .map(|s| log_name(s, class))
        .unwrap_or("UNKNOWN")
}

pub fn log_name(weapon_name: &str, class: Class) -> &str {
    match weapon_name {
        "rocketlauncher" => "tf_projectile_rocket",
        "grenadelauncher" => "tf_projectile_pipe",
        "pipebomblauncher" => "tf_projectile_pipe_remote",
        "compound_bow" => "tf_projectile_arrow",
        "pistol" => match class {
            Class::Scout => "pistol_scout",
            _ => "pistol",
        },
        "shotgun" => match class {
            Class::Scout => "scattergun",
            Class::Soldier => "shotgun_soldier",
            Class::Heavy => "shotgun_hwg",
            Class::Pyro => "shotgun_pyro",
            Class::Engineer => "shotgun_primary",
            _ => {
                error!("Shotgun on unexpected class! {class:?}");
                "shotgun"
            }
        },
        _ => weapon_name,
    }

    //   weapon_name
}

pub fn projectile_log_name(
    p: &entity::Projectile,
    target_team: &Team,
    item: Option<&schema::Item>,
) -> &'static str {
    if p.is_sentry {
        return if p.is_reflected {
            "deflect_rocket"
        } else {
            "obj_sentrygun3"
        };
    }
    trace!("proj log name target:{:?} {p:?}", target_team);
    if p.is_reflected
        && (p.original_team == *target_team
            || (p.original_team != *target_team
                && p.owner != p.original_owner
								// reflected sticky kills on their own team get attributed to the original
								// demo who triggers the det.
                && !entity::is_sticky(p.kind)))
    {
        if entity::is_arrow(p.kind) {
            return "deflect_arrow";
        }
        if p.kind == ProjectileType::CowMangler {
            return "deflect_rocket";
        }
        if p.kind == ProjectileType::ShortCircuit {
            // TF2 Doesn't emit this! TF2 seems to just emit "tf_projectile_energy_ball" but it's
            // better to distinguish this case.
            return "deflect_tf_projectile_energy_ball";
        }
        if p.kind == ProjectileType::DetonatorFlare {
            // TF2 Doesn't emit this! TF2 seems to just emit "tf_projectile_energy_ball" but it's
            // better to distinguish this case.
            return "deflect_flare_detonator";
        }
        if let Some(t) = p.grenade_type {
            return match t {
                GrenadeType::Pipe => "deflect_promode",
                GrenadeType::Sticky => "deflect_sticky",
                GrenadeType::StickyJumper => {
                    error!("Reflect kill with a sticky jumper?!");
                    "deflect_sticky"
                }
                GrenadeType::Cannonball => "loose_cannon_reflect",
            };
        }
        return match p.class_name.as_ref() {
            "CTFProjectile_Rocket" => "deflect_rocket",
            "CTFProjectile_Flare" => "deflect_flare",
            _ => {
                error!(
                    "Unknown reflected projectile class: {} {:?}",
                    p.class_name, p.kind
                );
                "deflect_UNKNOWN"
            }
        };
    }
    let class_name = p.class_name.as_str();
    if class_name == "CTFProjectile_MechanicalArmOrb" {
        return "tf_projectile_mechanicalarmorb";
    }
    if let Some(ref item) = item {
        trace!("projectile has schema {item:?}");
        if let Some(ref ln) = item.item_logname {
            return ustr::ustr(ln).as_str();
        }
        if let Some(ref class) = item.item_class {
            if class == "tf_weapon_rocketlauncher_directhit" {
                return "rocketlauncher_directhit";
            }
        }
    }
    if let Some(t) = p.grenade_type {
        return match t {
            GrenadeType::Pipe => "tf_projectile_pipe",
            GrenadeType::Sticky => "tf_projectile_pipe_remote",
            GrenadeType::StickyJumper => {
                error!("Kill with a sticky jumper?!");
                "tf_projectile_pipe_remote"
            }
            GrenadeType::Cannonball => "loose_cannon",
        };
    }
    match p.kind {
        ProjectileType::HealingBolt => return "crusaders_crossbow",
        ProjectileType::HuntsmanArrow => return "tf_projectile_arrow",
        ProjectileType::ScorchShotFlare => return "scorch_shot",
        _ => {}
    }
    match class_name {
        "CTFProjectile_Rocket" => "tf_projectile_rocket",
        "CTFProjectile_Flare" => "flaregun",
        _ => {
            error!("Unhandled projectile type: {}", p.class_name);
            "UNKNWON"
        }
    }
}

pub fn taunt_log_name(damage_type: DamageType) -> Option<&'static str> {
    match damage_type {
        DamageType::TauntHadouken => Some("taunt_pyro"),
        DamageType::TauntHighNoon => Some("taunt_heavy"),
        DamageType::TauntGrandSlam => Some("taunt_scout"),
        DamageType::TauntFencing => Some("taunt_spy"),
        DamageType::TauntArrowStab => Some("taunt_sniper"),
        DamageType::TauntGrenade => {
            // this could be taunt_soldier_lumbricus if the
            // worms grenade is equipped....
            Some("taunt_soldier")
        }
        DamageType::TauntBarbarianSwing => Some("taunt_demoman"),
        DamageType::TauntUberslice => Some("taunt_medic"),
        DamageType::TauntEngineerGuitarSmash => Some("taunt_guitar_kill"),
        DamageType::TauntEngineerArmKill => Some("robot_arm_blender_kill"),
        DamageType::TauntArmageddon => Some("armageddon"),
        DamageType::TauntAllclassGuitarRiff => Some("taunt_guitar_riff_kill"),
        DamageType::TauntGasBlast => Some("gas_blast"),
        DamageType::TauntTrickShot => Some("taunt_trickshot"),
        _ => None,
    }
}

pub fn projectile_explosion_radius(projectile_class_name: &str) -> f32 {
    match projectile_class_name {
        "CTFProjectile_SentryRocket" | "CTFProjectile_Rocket" | "CTFProjectile_EnergyBall" => 146.0,
        // airstrike is 0.90 -- but maybe we just store the mult_explosion_radius ?
        // lochnload is 0.75
        // grenadelauncher is 0.85
        // directhit is 0.3
        // beggars is 0.8
        "CTFGrenadePipebombProjectile" => 146.0 * 0.85,

        // flares are really complicated!!! lots of variations and rules
        "CTFProjectile_Flare" => 110.0,
        "CTFBall_Ornament" => 50.0,

        "CTFProjectile_JarMilk" => 200.0,

        "CTFStunBall" | "CTFProjectile_Cleaver" => 0.1, // its an OnHit check in game

        "CTFProjectile_EnergyRing" | "CTFProjectile_Arrow" | "CTFProjectile_HealingBolt" => 2.0, // traces a line, no explosion

        _ => {
            error!(
                "Unknown projectile needs explosion radius: {}",
                projectile_class_name
            );
            146.0
        }
    }
}
