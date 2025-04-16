use crate::{
    convert_vec,
    parser::{
        entity::{Entity, EntityClass, PROJECTILE_BOX},
        game::{Effects, GrenadeType, INVALID_HANDLE},
        props::*,
        summarizer::{Explosion, MatchAnalyzerView},
        weapon::projectile_explosion_radius,
    },
    schema::{Attribute, StringAttribute},
    Vec3,
};
use enumset::EnumSet;
use nalgebra::Vector3;
use parry3d::shape::SharedShape;
use rapier3d::prelude::{Aabb, Ball, BoundingVolume, Cuboid, QueryFilter};
use std::any::Any;
use tf_demo_parser::{
    demo::{
        message::packetentities::PacketEntity, packet::datatable::ClassId, parser::analyser::Team,
        sendprop::SendPropValue,
    },
    ParserState,
};
use tracing::error;

#[optfield::optfield(ProjectilePatch, merge_fn, attrs)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Projectile {
    pub original_launcher_handle: u32,
    pub launcher_schema_id: Option<u32>,
    pub is_sentry: bool,
    pub origin: Vec3,
    pub velocity: Vec3, // computed
    pub original_owner: u32,
    pub owner: u32,
    pub is_reflected: bool,
    pub original_team: Team,
    pub team: Team,
    pub class_name: String,
    pub grenade_type: Option<GrenadeType>,
    pub model_id: Option<u32>,
    pub kind: ProjectileType,
    pub effects: EnumSet<Effects>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ProjectileType {
    EnergyRing, // Pomson, Bison
    HealingBolt,
    Rocket,
    Sandman,
    ShortCircuit,
    SentryRocket,
    MadMilk,
    DragonsFuryFire,
    RescueRanger,
    HuntsmanArrow,
    Flare,
    DetonatorFlare,
    ManmelterFlare,
    ScorchShotFlare,
    Cleaver,
    Jarate,
    GasPasser,
    QuickieBomb,
    ScottishResistance,
    StickyBomb,
    StickyBombJumper,
    IronBomber,
    Pipe,
    LochNLoad,
    LooseCannon,
    WrapAssassin,
    CowMangler,
    #[default]
    Unknown,
}

pub fn is_sticky(kind: ProjectileType) -> bool {
    matches!(
        kind,
        ProjectileType::StickyBomb
            | ProjectileType::StickyBombJumper
            | ProjectileType::ScottishResistance
            | ProjectileType::QuickieBomb
    )
}

pub fn is_arrow(kind: ProjectileType) -> bool {
    matches!(
        kind,
        ProjectileType::HealingBolt | ProjectileType::HuntsmanArrow | ProjectileType::RescueRanger
    )
}

pub fn can_hurt_without_exploding(kind: ProjectileType) -> bool {
    matches!(
        kind,
        ProjectileType::HuntsmanArrow
            | ProjectileType::ShortCircuit
            | ProjectileType::EnergyRing
            | ProjectileType::ScorchShotFlare
    )
}

impl Projectile {
    fn parse(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &MatchAnalyzerView,
        patch: &mut ProjectilePatch,
    ) {
        let class_name = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
            .map(|s| s.name.to_string())
            .unwrap_or("UNKNOWN_PROJECTILE".to_string());

        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (ORIGIN | ROCKET_ORIGIN | GRENADE_ORIGIN, &SendPropValue::Vector(o)) => {
                    patch.origin = Some(convert_vec(o));
                }
                (ROCKET_DEFLECTED | GRENADE_DEFLECTED, &SendPropValue::Integer(b)) => {
                    patch.is_reflected = Some(b > 0)
                }
                (OWNER | DEFLECT_OWNER, &SendPropValue::Integer(h)) => {
                    let h = h as u32;
                    if h != INVALID_HANDLE {
                        patch.owner = Some(h);
                    }
                }

                (ORIGINAL_LAUNCHER, &SendPropValue::Integer(h)) => {
                    let launcher = h as u32;
                    if launcher != INVALID_HANDLE {
                        patch.original_launcher_handle = Some(launcher);
                    }

                    // set owner based on the launcher weapon, needed for some projectiles.
                    if patch.owner.is_none() {
                        let handle = game
                            .weapon_owners
                            .get(&launcher)
                            .and_then(|uid| game.player_summaries.get(uid))
                            .and_then(|p| game.get_player(&p.entity_id))
                            .and_then(|p| p.handle());

                        patch.owner = handle;
                    }
                }

                (TEAM, &SendPropValue::Integer(t)) => {
                    if let Ok(team_val) = Team::try_from(t as u8) {
                        patch.team = Some(team_val);
                    } else {
                        error!("Invalid team value {t}");
                    }
                }

                (PIPE_TYPE, &SendPropValue::Integer(t)) => {
                    if class_name != "CTFGrenadePipebombProjectile" {
                        // Jars have this field set for some reason -- but it doesn't differentiate
                        // anything.
                        continue;
                    }

                    let Ok(grenade_type) = GrenadeType::try_from(t as u16) else {
                        error!("Unknown grenade type {t} when parsing {packet:?}");
                        continue;
                    };
                    patch.grenade_type = Some(grenade_type);
                }
                (MODEL, &SendPropValue::Integer(t)) => {
                    patch.model_id = Some(t as u32);
                }

                (EFFECTS, &SendPropValue::Integer(f)) => {
                    patch.effects = Some(
                        EnumSet::<Effects>::try_from_repr(f as u16).unwrap_or_else(|| {
                            error!("Unknown entity effects on projectile: {}", f);
                            EnumSet::<_>::new()
                        }),
                    );
                }

                (INITIAL_SPEED, _) => {}
                (ROCKET_ROTATION | GRENADE_ROTATION, _) => {}

                _ => {}
            }
        }
    }

    pub fn check_hit(&self, volume: &Aabb) -> bool {
        if is_arrow(self.kind)
            || self.kind == ProjectileType::ShortCircuit
            || self.kind == ProjectileType::EnergyRing
        {
            let hitbox = if is_arrow(self.kind) {
                Cuboid::new(Vector3::new(2.0, 2.0, 2.0))
            } else {
                Cuboid::new(Vector3::new(100.0, 100.0, 100.0))
            };

            // TODO: do a proper ShapeCast if necessary...
            return hitbox.aabb(&self.origin.into()).intersects(volume)
                || hitbox
                    .aabb(&(self.origin + self.velocity.coords / 2.0).into())
                    .intersects(volume)
                || hitbox
                    .aabb(&(self.origin + self.velocity.coords).into())
                    .intersects(volume);
        }

        let explosion_ball = Ball::new(projectile_explosion_radius(&self.class_name));

        explosion_ball.aabb(&self.origin.into()).intersects(volume)
    }
}

impl Entity for Projectile {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Self {
        let class_name = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
            .map(|s| s.name.to_string())
            .unwrap_or("UNKNOWN_PROJECTILE".to_string());

        let mut p = ProjectilePatch::default();
        Projectile::parse(packet, parser_state, game, &mut p);

        let origin = p.origin.unwrap_or_else(|| {
            if class_name != "CTFProjectile_EnergyRing" {
                error!("No origin for Projectile! {:?} {packet:?}", class_name);
            }
            Vec3::default()
        });

        let is_sentry = if class_name == "CTFProjectile_SentryRocket" {
            game.world.intersections_with_point(
                game.rigid_body_set,
                game.collider_set,
                &origin,
                QueryFilter::new().predicate(&|_handle, c| {
                    if let Some(e) = &game.entities[c.user_data as usize] {
                        e.class() == EntityClass::Sentry
                    } else {
                        error!("Could not find entity from user_data: {}", c.user_data);
                        false
                    }
                }),
                |coll| {
                    // unwrap() safety: this is a lookup into the very set we are iterating over.
                    let collider = game.collider_set.get(coll).unwrap();

                    let eid = collider.user_data as usize;
                    let Some(entity) = &game.entities[eid] else {
                        error!(
                            "Collided with a broken entity id when checking sentry rocket {eid}"
                        );
                        return false;
                    };

                    p.owner = entity.owner();

                    false // Only ever look at the first match.
                },
            );
            true
        } else {
            false
        };

        let owner = p
            .owner
            .and_then(|x| if x == INVALID_HANDLE { None } else { Some(x) })
            .or(p
                .original_launcher_handle
                .and_then(|h| game.weapon_owners.get(&h))
                .and_then(|uid| game.player_summaries.get(uid))
                .and_then(|p| game.get_player(&p.entity_id))
                .and_then(|p| p.handle()))
            .unwrap_or_else(|| {
                error!("No owner for Projectile! {packet:?}");
                0
            });

        let mut original_owner = owner;

        let launcher = p
            .original_launcher_handle
            .and_then(|h| game.entity_handles.get(&h))
            .and_then(|eid| {
                game.entities
                    .get(usize::from(*eid))
                    .and_then(|b| b.as_ref())
            })
            .and_then(|e| e.weapon());

        if let Some(launcher) = launcher {
            if owner != launcher.owner {
                if p.is_reflected == Some(true) {
                    original_owner = launcher.owner;
                } else {
                    error!(
                        "original owner on non-reflected projectile does not match launcher owner"
                    );
                }
            }
        } else if !is_sentry {
            error!("No launcher found for projectile");
        }

        let launcher_schema = launcher.map(|w| w.schema_id);
        let item = launcher_schema.and_then(|id| game.schema.items.get(&id));
        let mut kind = match class_name.as_str() {
            "CTFProjectile_EnergyRing" => ProjectileType::EnergyRing,
            "CTFProjectile_HealingBolt" => ProjectileType::HealingBolt,
            "CTFProjectile_Rocket" => ProjectileType::Rocket,
            "CTFProjectile_SentryRocket" => ProjectileType::SentryRocket,
            "CTFProjectile_JarMilk" => ProjectileType::MadMilk,
            "CTFProjectile_BallOfFire" => ProjectileType::DragonsFuryFire,
            "CTFProjectile_Cleaver" => ProjectileType::Cleaver,
            "CTFBall_Ornament" => ProjectileType::WrapAssassin,
            "CTFProjectile_EnergyBall" => ProjectileType::CowMangler,
            "CTFStunBall" => ProjectileType::Sandman,
            "CTFProjectile_MechanicalArmOrb" => ProjectileType::ShortCircuit,
            _ => ProjectileType::Unknown,
        };

        if let Some(s) = item {
            if let Some(l) = &s.item_class {
                if l == "tf_weapon_shotgun_building_rescue" {
                    kind = ProjectileType::RescueRanger;
                } else if l == "tf_weapon_compound_bow" {
                    kind = ProjectileType::HuntsmanArrow;
                } else if l == "tf_weapon_jar" {
                    kind = ProjectileType::Jarate;
                } else if l == "tf_weapon_jar_gas" {
                    kind = ProjectileType::GasPasser;
                }
            }

            if let Some(l) = &s.item_name {
                if l == "#TF_Weapon_Sticky_Quickie" {
                    kind = ProjectileType::QuickieBomb;
                } else if l == "#TF_Unique_Achievement_StickyLauncher" {
                    kind = ProjectileType::ScottishResistance;
                } else if l == "#TF_Weapon_StickyBomb_Jump" {
                    kind = ProjectileType::StickyBombJumper;
                } else if l == "#TF_Weapon_Iron_bomber" {
                    kind = ProjectileType::IronBomber;
                } else if l == "#TF_LochNLoad" {
                    kind = ProjectileType::LochNLoad;
                } else if l == "#TF_Weapon_Cannon" {
                    kind = ProjectileType::LooseCannon;
                } else if l == "#TF_Weapon_PipebombLauncher"
                    || l.starts_with("#TF_Weapon_StickybombLauncher_")
                {
                    kind = ProjectileType::StickyBomb;
                } else if l.starts_with("#TF_Weapon_GrenadeLauncher") {
                    kind = ProjectileType::Pipe;
                }
            }

            let mode = s.attributes.iter().find_map(|(_k, v)| {
                if let Attribute::String(StringAttribute {
                    attribute_class,
                    value,
                }) = &v
                {
                    if attribute_class == "set_weapon_mode" {
                        return Some(value.as_str());
                    }
                }
                None
            });

            if class_name == "CTFProjectile_Flare" {
                kind = match mode {
                    Some("1") => ProjectileType::DetonatorFlare,
                    Some("2") => ProjectileType::ManmelterFlare,
                    Some("3") => ProjectileType::ScorchShotFlare,
                    _ => ProjectileType::Flare,
                }
            }
        }

        if matches!(kind, ProjectileType::Unknown) {
            error!("Unknown launcher type for {class_name} {item:?}");
        }

        if launcher_schema.is_none() && !is_sentry {
            error!("No projectile launcher class {class_name} {p:?}");
        }

        Self {
            launcher_schema_id: launcher_schema,
            is_sentry,
            original_launcher_handle: 0, // only for reading owner
            origin,
            velocity: Default::default(),
            original_owner,
            owner,
            is_reflected: p.is_reflected.unwrap_or(false),
            original_team: p.team.unwrap_or(Team::Spectator),
            team: p.team.unwrap_or(Team::Spectator),
            class_name,
            grenade_type: p.grenade_type,
            model_id: p.model_id,
            effects: p.effects.unwrap_or_default(),
            kind,
        }
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        let mut patch = Box::new(ProjectilePatch::default());
        Projectile::parse(packet, parser_state, game, &mut patch);

        let owner_changed = patch.owner.map(|r| r != self.owner).unwrap_or(false);
        let team_changed = patch.team.map(|r| r != self.team).unwrap_or(false);
        if team_changed && !owner_changed {
            error!("Projectile changed team without changing owner entity {patch:?}");
        }

        let disappeared = if let Some(new_effects) = patch.effects {
            new_effects.contains(Effects::NoDraw) && !self.effects.contains(Effects::NoDraw)
        } else {
            false
        };

        if can_hurt_without_exploding(self.kind)
            || (disappeared && self.kind == ProjectileType::ScottishResistance)
        {
            game.explosions.push(Explosion {
                origin: self.origin,
                projectile: Box::new(self.clone()),
            });
        }

        patch
    }

    fn delete(self: Box<Self>, game: &mut MatchAnalyzerView) {
        game.explosions.push(Explosion {
            origin: self.origin,
            projectile: self,
        });
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<ProjectilePatch>().unwrap();

        if let (p, Some(n)) = (self.origin, patch.origin) {
            self.velocity = (n - p).into();
        }

        self.merge_opt(*patch);
    }

    fn shape(&self) -> Option<SharedShape> {
        Some(PROJECTILE_BOX.clone())
    }

    fn origin(&self) -> Option<Vec3> {
        Some(self.origin)
    }

    fn owner(&self) -> Option<u32> {
        Some(self.owner)
    }

    fn class(&self) -> EntityClass {
        EntityClass::Projectile
    }

    fn projectile(&self) -> Option<&Projectile> {
        Some(self)
    }
}
