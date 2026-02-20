use crate::{
    parser::{
        entity::{self, Entity, ProjectileType},
        game::{
            Damage, DamageEffect, DamageType, Death, PlayerAnimation, RoundState, WeaponId,
            INVALID_HANDLE,
        },
        is_false,
        player::PlayerSummary,
        props::*,
        weapon::{self, projectile_log_name, sentry_name, taunt_log_name},
    },
    schema::{Item, Schema},
    Vec3,
};
use alga::linear::EuclideanSpace;
use enumset::EnumSet;
use num_enum::TryFromPrimitive;
use parry3d::math::Vector;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, Cuboid, IslandManager, QueryPipeline,
    RigidBodySet,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tf_demo_parser::{
    demo::{
        data::{DemoTick, MaybeUtf8String, UserInfo},
        gameevent_gen::{
            PlayerDeathEvent, PlayerHurtEvent, TeamPlayCaptureBlockedEvent,
            TeamPlayPointCapturedEvent,
        },
        gamevent::GameEvent,
        message::{
            gameevent::GameEventMessage,
            packetentities::{EntityId, PacketEntity, UpdateType},
            usermessage::{ChatMessageKind, UserMessage},
            Message, NetTickMessage,
        },
        packet::{
            datatable::{ClassId, ParseSendTable, ServerClass},
            stringtable::StringTableEntry,
        },
        parser::{
            gamestateanalyser::{Class, Team, UserId},
            MessageHandler,
        },
        sendprop::SendPropValue,
    },
    MessageType, ParserState, ReadResult, Stream,
};
use tracing::{debug, error, span::EnteredSpan, trace, warn};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DemoSummary {
    pub rounds: Vec<RoundSummary>,
    pub chat: Vec<ChatMessage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    tick: DemoTick,
    user: String, // steamid
    message: String,
    #[serde(skip_serializing_if = "is_false")]
    is_dead: bool,
    #[serde(skip_serializing_if = "is_false")]
    is_team: bool,
    #[serde(skip_serializing_if = "is_false")]
    is_spec: bool,
    #[serde(skip_serializing_if = "is_false")]
    is_name_change: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PlayerDeath {}

const ENTITY_COUNT: usize = 2048;

#[derive(Clone, Debug)]
pub struct Explosion {
    pub projectile: Box<entity::Projectile>,
    pub origin: Vec3,
}

#[derive(Clone, Debug)]
pub struct SentryShot {
    pub sentry: entity::Sentry,
}

#[derive(Debug)]
pub enum HurtSource {
    Explosion(Explosion),
    NonBlastProjectile(Explosion), // crossbow, huntsman
    SentryShot(SentryShot),
    Unknown,
}

#[derive(Debug)]
pub struct Hurt {
    pub victim: UserId,
    pub attacker: UserId,
    pub wep: u32,
    pub origin: Vec3,
    pub source: HurtSource,
}

pub struct MatchAnalyzer<'a> {
    chat: Vec<ChatMessage>,
    current_round: RoundSummary,
    rounds: Vec<RoundSummary>,
    player_summaries: HashMap<String, PlayerSummary>, // steamid -> PlayerSummary
    user_id_to_steam_id: HashMap<UserId, String>,     // user_id -> steamid
    user_entities: HashMap<EntityId, UserId>,         // entity_id -> user_id
    weapon_owners: HashMap<u32, UserId>,
    cosmetic_owners: HashMap<u32, UserId>,
    entity_handles: HashMap<u32, EntityId>,
    entities: Box<[Option<Box<dyn Entity>>; ENTITY_COUNT]>,
    colliders: Box<[Option<ColliderHandle>; ENTITY_COUNT]>,

    effects: HashMap<u32, String>,
    models: HashMap<u32, String>,
    waiting_for_players: bool,
    round_state: RoundState,
    span: Option<EnteredSpan>,
    tick: DemoTick,
    server_tick: u32,
    tick_events: Vec<Event>,
    schema: &'a Schema,

    // Events that happened this tick
    hurts: Vec<Hurt>,
    sentry_shots: Vec<SentryShot>,
    explosions: Vec<Explosion>, // aka projectiles that were deleted this frame
    deleted_entities: HashSet<EntityId>,

    airblasts: HashSet<u32>, // handles of players that airblasted this tick

    // Queryable geometry world. QVBH under the hood.
    world: QueryPipeline,
    island_manager: IslandManager,
    collider_set: ColliderSet,
    rigid_body_set: RigidBodySet, // unused, but needed for some APIs :\
    mutated_colliders: Vec<ColliderHandle>,
    removed_colliders: Vec<ColliderHandle>,

    weapon_class_ids: HashSet<ClassId>,
    projectile_class_ids: HashSet<ClassId>,
}

pub struct MatchAnalyzerView<'a> {
    pub user_entities: &'a HashMap<EntityId, UserId>,
    pub models: &'a HashMap<u32, String>,
    pub entities: &'a [Option<Box<dyn Entity>>; ENTITY_COUNT],
    pub entity_handles: &'a HashMap<u32, EntityId>,
    pub player_summaries: &'a mut HashMap<String, PlayerSummary>,
    pub user_id_to_steam_id: &'a HashMap<UserId, String>,
    pub weapon_owners: &'a mut HashMap<u32, UserId>,
    pub cosmetic_owners: &'a mut HashMap<u32, UserId>,
    pub explosions: &'a mut Vec<Explosion>,
    pub tick_events: &'a mut Vec<Event>,
    pub schema: &'a Schema,
    pub world: &'a QueryPipeline,
    pub collider_set: &'a ColliderSet,
    pub rigid_body_set: &'a RigidBodySet, // unused, but needed for some APIs :\
    pub tick: DemoTick,
}

impl MatchAnalyzerView<'_> {
    pub fn get_player(&self, id: &EntityId) -> Option<&entity::Player> {
        self.entities
            .get(usize::from(*id))
            .and_then(|b| b.as_ref())
            .and_then(|b| b.player())
    }

    pub fn handle_projectile_fired(&mut self, owner: &u32, item: &Item) {
        let Some(eid) = self.entity_handles.get(owner) else {
            error!("Could not find player entity for handle that fired projectile {owner:?}");
            return;
        };
        let Some(pe) = self.get_player(eid) else {
            error!("Could not find player entity that fired projectile {owner:?}");
            return;
        };
        let class = pe.class;
        let uid = pe.user_id;

        let Some(steamid) = self.user_id_to_steam_id.get(&uid).cloned() else {
            error!("Could not find steamid for user {uid} that fired projectile");
            return;
        };
        let Some(p) = self.player_summaries.get_mut(&steamid) else {
            error!("Could not find player summary for steamid {steamid} that fired projectile");
            return;
        };

        p.handle_fire_shot(weapon::weapon_name(item, class));
    }

    pub fn handle_object_built(&mut self, owner: &u32) {
        let Some(eid) = self.entity_handles.get(owner) else {
            error!("Could not find player entity for handle that built object {owner:?}");
            return;
        };
        let Some(pe) = self.get_player(eid) else {
            error!("Could not find player entity that built object {owner:?}");
            return;
        };

        let class = pe.class;
        let uid = pe.user_id;

        let Some(item) = self
            .entity_handles
            .get(&pe.last_active_weapon_handle)
            .and_then(|eid| {
                self.entities
                    .get(usize::from(*eid))
                    .and_then(|b| b.as_ref())
            })
            .and_then(|e| e.weapon())
            .and_then(|w| self.schema.items.get(&w.schema_id))
        else {
            error!("Could not find item used to create sentry");
            return;
        };

        let Some(steamid) = self.user_id_to_steam_id.get(&uid).cloned() else {
            error!("Could not find steamid for user {uid} that built object");
            return;
        };
        let Some(p) = self.player_summaries.get_mut(&steamid) else {
            error!("Could not find player summary for steamid {steamid} that built object");
            return;
        };

        p.handle_object_built(weapon::weapon_name(item, class));
    }
}

#[derive(Debug)]
pub enum Event {
    Death(Box<PlayerDeathEvent>),
    Hurt(PlayerHurtEvent),
    MedigunCharged(u32),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Killstreak {
    pub user_id: u32,
    pub class: Class,
    pub duration: u32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RoundSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<Team>,
    #[serde(skip_serializing_if = "is_false")]
    pub is_stalemate: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_sudden_death: bool,

    pub time: f32, // in seconds

    pub mvps: Vec<String>,           // steamids
    pub players: Vec<PlayerSummary>, // steamids

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub winners: Vec<String>, // steamids
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub losers: Vec<String>, // steamids
}

impl<'a> MatchAnalyzer<'a> {
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            chat: Default::default(),
            current_round: Default::default(),
            rounds: Default::default(),
            player_summaries: Default::default(),
            user_id_to_steam_id: Default::default(),
            user_entities: Default::default(),
            weapon_owners: Default::default(),
            cosmetic_owners: Default::default(),
            entity_handles: Default::default(),
            entities: Box::new([const { None }; ENTITY_COUNT]),
            colliders: Box::new([const { None }; ENTITY_COUNT]),
            effects: Default::default(),
            models: Default::default(),
            waiting_for_players: Default::default(),
            round_state: Default::default(),
            span: Default::default(),
            tick: Default::default(),
            server_tick: Default::default(),
            tick_events: Default::default(),
            hurts: Default::default(),
            sentry_shots: Default::default(),
            explosions: Default::default(),
            airblasts: Default::default(),
            deleted_entities: Default::default(),
            world: QueryPipeline::new(),
            island_manager: IslandManager::new(),
            collider_set: ColliderSet::with_capacity(ENTITY_COUNT),
            rigid_body_set: RigidBodySet::with_capacity(0),
            mutated_colliders: Vec::with_capacity(ENTITY_COUNT),
            removed_colliders: Vec::with_capacity(ENTITY_COUNT),
            projectile_class_ids: Default::default(),
            weapon_class_ids: Default::default(),
        }
    }

    fn parse_user_info(
        &mut self,
        index: usize,
        text: Option<&str>,
        data: Option<Stream>,
    ) -> ReadResult<()> {
        if let Some(user_info) = UserInfo::parse_from_string_table(index as u16, text, data)? {
            let entity_id = user_info.entity_id;
            let user_id = user_info.player_info.user_id;
            let steam_id = user_info.player_info.steam_id.clone();

            trace!(
                "user info {} user_id:{user_id} entity_id:{entity_id} steam_id:{steam_id} {user_info:?}",
                user_info.player_info.name,
            );

            self.player_summaries
                .entry(steam_id.clone())
                .and_modify(|summary| {
                    summary.connection_count += 1;
                    summary.entity_id = user_info.entity_id; // Update to the latest entity_id
                    summary.user_id = user_id.into(); // Update to the latest user_id
                    summary.name = user_info.player_info.name.clone(); // Name might change
                })
                .or_insert_with(|| PlayerSummary {
                    name: user_info.player_info.name,
                    steamid: steam_id.clone(),
                    entity_id: user_info.entity_id,
                    user_id: user_id.into(),
                    is_fake_player: user_info.player_info.is_fake_player > 0,
                    is_hl_tv: user_info.player_info.is_hl_tv > 0,
                    is_replay: user_info.player_info.is_replay > 0,
                    connection_count: 1, // First connection for this steamid
                    ..Default::default()
                });

            self.user_entities.insert(entity_id, user_id);
            self.user_id_to_steam_id.insert(user_id, steam_id);
        }

        Ok(())
    }

    // Calculate weapon name in a player damage situation
    //
    // Note that damage_bits will only be provided for deaths.
    pub fn weapon_name_from_damage(
        &self,
        damage_type: DamageType,
        damage_bits: EnumSet<Damage>,
        victim: &entity::Player,
        attacker: &entity::Player,
        hurt: Option<&Hurt>,
    ) -> &'static str {
        let mut my_name: &'static str = "UNKNOWN";

        let dmg_to_victim: Vec<_> = hurt.map(|h| vec![h]).unwrap_or_else(|| {
            self.hurts
                .iter()
                .filter(|h| h.victim == victim.user_id)
                .collect::<Vec<&Hurt>>()
        });

        let h = attacker.last_active_weapon_handle;
        if let Some(weapon) = self.get_weapon(&h) {
            if let Some(item) = self.schema.items.get(&weapon.schema_id) {
                my_name = weapon::weapon_name(item, attacker.class);
            } else {
                error!("Weapon id not in schema! {}", weapon.schema_id);
            }
        } else {
            error!("Player has unknown weapon handle: {h}");
        }

        if let Some(sentry_hurt) = dmg_to_victim
            .iter()
            .find(|h| matches!(h.source, HurtSource::SentryShot(_)))
        {
            let HurtSource::SentryShot(ref sentry_shot) = sentry_hurt.source else {
                error!("impossible match mi ss");
                return "UNKNOWN";
            };
            trace!("sentry shot {sentry_shot:?}");
            my_name = sentry_name(&sentry_shot.sentry);
        }

        if let Some(sentry_hurt) = dmg_to_victim
            .iter()
            .find(|h| matches!(h.source, HurtSource::NonBlastProjectile(_)))
        {
            let HurtSource::NonBlastProjectile(ref exp) = sentry_hurt.source else {
                panic!("impossible match miss");
            };
            let item = exp
                .projectile
                .launcher_schema_id
                .and_then(|id| self.schema.items.get(&id));
            my_name = projectile_log_name(&exp.projectile, &victim.team, item);
        } else if (damage_bits.contains(Damage::Blast)
            || damage_type == DamageType::BurningFlare
            || damage_type == DamageType::Plasma
            || damage_type == DamageType::PlasmaCharged
            || damage_type == DamageType::DefensiveSticky
            || damage_type == DamageType::AirStickyBurst
            || damage_type == DamageType::RocketDirecthit
            || damage_type == DamageType::StandardSticky
            || damage_type == DamageType::Normal)
            && damage_type != DamageType::Baseball
            && damage_type != DamageType::Headshot
            && damage_type != DamageType::HeadshotDecapitation
            && damage_type != DamageType::Suicide
            && damage_type != DamageType::CannonballPush
            && damage_type != DamageType::TauntGrenade
            && damage_type != DamageType::TauntEngineerArmKill
            && damage_type != DamageType::StickbombExplosion
        {
            let Some(attacker_handle) = attacker.handle() else {
                error!("No attacker handle for death");
                return "UNKNOWN";
            };

            let mut exps: Vec<_> = dmg_to_victim
                .iter()
                .filter_map(|h| {
                    if let HurtSource::Explosion(e) = &h.source {
                        if e.projectile.owner() == Some(attacker_handle)
                            || e.projectile.original_owner == attacker_handle
                            || self.airblasts.contains(&attacker_handle)
                        {
                            return Some(e);
                        }
                    }
                    None
                })
                .collect();

            if exps.len() > 1 {
                trace!("blast with many exps {:?}", exps);
                exps.drain(1..);
            }

            if let Some(ref exp) = exps.first() {
                trace!("blast with exp {:?}", exp);

                let item = exp
                    .projectile
                    .launcher_schema_id
                    .and_then(|id| self.schema.items.get(&id));

                my_name = projectile_log_name(&exp.projectile, &victim.team, item);
            } else if damage_bits.contains(Damage::Blast) && damage_type != DamageType::BurningFlare
            {
                let d = EuclideanSpace::distance(&attacker.origin, &victim.origin);
                if d > 100.0 {
                    // "Blast" damage can happen without a projectile in these cases:
                    //  - flare-caused burning
                    //  - if projectile impacts and explodes on the first tick, no
                    //    projectile entity is created.
                    error!(
												"Blast damage without a matching explosion type:{damage_type:?} (distance {d})"
										);
                }
            }
        }

        if let Some(taunt) = taunt_log_name(damage_type) {
            my_name = taunt;
        } else if damage_type == DamageType::DragonsFuryBonusBurning {
            my_name = "dragons_fury_bonus";
        } else if damage_type == DamageType::Burning {
            my_name = self
                .get_weapon(&attacker.weapon_handles[0])
                .and_then(|w| self.schema.items.get(&w.schema_id))
                .and_then(|i| i.item_logname.as_ref().map(|s| ustr::ustr(s).as_str()))
                .unwrap_or("flamethrower");
        } else if damage_type == DamageType::BurningArrow {
            my_name = self
                .get_weapon(&attacker.weapon_handles[0])
                .and_then(|w| self.schema.items.get(&w.schema_id))
                .and_then(|i| i.item_logname.as_ref().map(|s| ustr::ustr(s).as_str()))
                .unwrap_or("compound_bow");
        } else if damage_type == DamageType::BurningFlare {
            my_name = self
                .get_weapon(&attacker.weapon_handles[1])
                .and_then(|w| self.schema.items.get(&w.schema_id))
                .and_then(|i| i.item_logname.as_ref().map(|s| ustr::ustr(s).as_str()))
                .unwrap_or("flaregun");
        } else if damage_type == DamageType::ChargeImpact {
            if let Some(shield_logname) = attacker.cosmetic_handles.iter().find_map(|h| {
                self.entity_handles
                    .get(h)
                    .and_then(|eid| self.entities.get(usize::from(*eid)))
                    .and_then(|b| b.as_ref())
                    .and_then(|b| b.shield())
                    .and_then(|s| self.schema.items.get(&s.schema_id))
                    .and_then(|s| s.item_logname.as_ref().map(|s| ustr::ustr(s).as_str()))
            }) {
                my_name = shield_logname;
            } else {
                error!("Chart impact without a shield?!")
            }
        } else if damage_type == DamageType::PlayerSentry {
            my_name = "wrangler_kill";
        } else if damage_type == DamageType::Baseball {
            my_name = "ball";
        } else if damage_type == DamageType::ComboPunch {
            my_name = "robot_arm_combo_kill";
        } else if damage_type == DamageType::CannonballPush {
            my_name = "loose_cannon_impact";
        } else if damage_type == DamageType::BootsStomp {
            my_name = match attacker.class {
                Class::Soldier => "mantreads",
                Class::Pyro => "rocketpack_stomp",
                _ => {
                    error!("Unknown how class {:?} can stomp", attacker.class);
                    "mantreads"
                }
            };
        } else if damage_type == DamageType::Telefrag {
            my_name = "telefrag";
        } else if damage_type == DamageType::DefensiveSticky {
            my_name = "sticky_resistance";
        } else if damage_type == DamageType::StickbombExplosion {
            my_name = "ullapool_caber_explosion";
        } else if damage_type == DamageType::Bleeding {
            my_name = "bleed_kill";
        } else if dmg_to_victim.is_empty() || damage_type == DamageType::Suicide {
            if dmg_to_victim.is_empty() && damage_type != DamageType::Suicide {
                error!("No hurts for non-suicide???");
            }

            my_name = if damage_bits.contains(Damage::PreventPhysicsForce) {
                // Player suicided with a killbind, either kill or explode (Can
                // filter on Damage::Blast if we ever care about distinguishing
                // those.)
                "player"
            } else {
                "world"
            };
        }
        my_name
    }

    fn handle_packet_entity(&mut self, packet: &PacketEntity, parser_state: &ParserState) {
        let Some(class) = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
        else {
            error!("Unknown server class: {}", packet.server_class);
            return;
        };

        let eid = usize::from(packet.entity_index);

        let class_name = class.name.as_str();
        let is_projectile = self.projectile_class_ids.contains(&packet.server_class);
        let is_weapon = self.weapon_class_ids.contains(&packet.server_class);

        // Trace runs are really slow so skip at least some of the noise
        if class_name != "CBoneFollower"
            && class_name != "CBeam"
            && class_name != "CTFAmmoPack"
            && class_name != "CSniperDot"
            && class_name != "CTFDroppedWeapon"
            && class_name != "CBaseDoor"
            && !(class_name == "CTFPlayer"
                && packet.update_type == UpdateType::Preserve
                && packet.props.len() == 1
                && packet.props[0].identifier == SIM_TIME)
        {
            trace!("Packet {class_name} {:?} {packet:?}", packet.update_type);
        }

        if class_name == "CTFPlayerResource" {
            self.handle_player_resource(packet, parser_state);
            return;
        }
        if class_name == "CTFGameRulesProxy" {
            self.handle_game_rules(packet, parser_state);
            return;
        }

        match packet.update_type {
            UpdateType::Enter => {
                let mut ma = MatchAnalyzerView {
                    user_entities: &self.user_entities,
                    models: &self.models,
                    entities: &self.entities,
                    entity_handles: &self.entity_handles,
                    player_summaries: &mut self.player_summaries,
                    user_id_to_steam_id: &self.user_id_to_steam_id,
                    weapon_owners: &mut self.weapon_owners,
                    cosmetic_owners: &mut self.cosmetic_owners,
                    explosions: &mut self.explosions,
                    tick_events: &mut self.tick_events,
                    schema: self.schema,
                    world: &self.world,
                    collider_set: &self.collider_set,
                    rigid_body_set: &self.rigid_body_set,
                    tick: self.tick,
                };

                let e: Box<dyn Entity> = match class_name {
                    "CObjectSentrygun" => {
                        Box::new(entity::Sentry::new(packet, parser_state, &mut ma))
                    }
                    "CObjectTeleporter" => {
                        Box::new(entity::Teleporter::new(packet, parser_state, &mut ma))
                    }
                    "CObjectDispenser" => {
                        Box::new(entity::Dispenser::new(packet, parser_state, &mut ma))
                    }
                    "CTFPlayer" => Box::new(entity::Player::new(packet, parser_state, &mut ma)),
                    "CTFWearableDemoShield" => {
                        Box::new(entity::Shield::new(packet, parser_state, &mut ma))
                    }
                    _ if is_projectile => {
                        Box::new(entity::Projectile::new(packet, parser_state, &mut ma))
                    }
                    _ if is_weapon => Box::new(entity::Weapon::new(packet, parser_state, &mut ma)),
                    _ => Box::new(entity::Unknown::new(packet, parser_state, &mut ma)),
                };
                self.entities[eid] = Some(e);
            }
            UpdateType::Preserve => {
                let Some(ref e) = self.entities[eid] else {
                    error!(
                        "Preserve update for unknown entity {} in {:?}",
                        packet.entity_index, packet
                    );
                    return;
                };

                let mut ma = MatchAnalyzerView {
                    user_entities: &self.user_entities,
                    models: &self.models,
                    entities: &self.entities,
                    entity_handles: &self.entity_handles,
                    player_summaries: &mut self.player_summaries,
                    user_id_to_steam_id: &self.user_id_to_steam_id,
                    weapon_owners: &mut self.weapon_owners,
                    cosmetic_owners: &mut self.cosmetic_owners,
                    explosions: &mut self.explosions,
                    tick_events: &mut self.tick_events,
                    schema: self.schema,
                    world: &self.world,
                    collider_set: &self.collider_set,
                    rigid_body_set: &self.rigid_body_set,
                    tick: self.tick,
                };

                let update = e.parse_preserve(packet, parser_state, &mut ma);

                let e = self.entities[eid].as_mut().unwrap(); // safety: checked above

                e.apply_preserve(update);
            }
            UpdateType::Delete | UpdateType::Leave => {
                self.deleted_entities.insert(packet.entity_index.clone());

                if !packet.props.is_empty() {
                    error!(
                        "Unexpect props on {:?} update: {:?}",
                        packet.update_type, packet.props
                    );
                }

                let e = std::mem::take(&mut self.entities[eid]);
                let Some(e) = e else {
                    error!(
                        "{:?} for unknown entity {} from {:?}",
                        packet.update_type, packet.entity_index, packet
                    );
                    return;
                };

                let mut ma = MatchAnalyzerView {
                    user_entities: &self.user_entities,
                    models: &self.models,
                    entities: &self.entities,
                    entity_handles: &self.entity_handles,
                    player_summaries: &mut self.player_summaries,
                    user_id_to_steam_id: &self.user_id_to_steam_id,
                    weapon_owners: &mut self.weapon_owners,
                    cosmetic_owners: &mut self.cosmetic_owners,
                    explosions: &mut self.explosions,
                    tick_events: &mut self.tick_events,
                    schema: self.schema,
                    world: &self.world,
                    collider_set: &self.collider_set,
                    rigid_body_set: &self.rigid_body_set,
                    tick: self.tick,
                };

                if packet.update_type == UpdateType::Delete {
                    e.delete(&mut ma);
                } else {
                    e.leave(&mut ma);
                }

                let k = std::mem::take(&mut self.colliders[eid]);
                if let Some(k) = k {
                    self.collider_set.remove(
                        k,
                        &mut self.island_manager,
                        &mut self.rigid_body_set,
                        false,
                    );
                    self.removed_colliders.push(k);
                }

                self.entities[eid] = None;
                self.colliders[eid] = None;
                return;
            }
        }

        if let Some(e) = &self.entities[eid] {
            if let Some(h) = e.handle() {
                self.entity_handles.insert(h, EntityId::from(eid as u32));
            }

            if let (Some(shape), Some(origin)) = (e.shape(), e.origin()) {
                if let Some(collider) = self.colliders[eid] {
                    let Some(c) = self.collider_set.get_mut(collider) else {
                        error!("Colliders out of sync: missing collider for: {eid:?} {packet:?}");
                        return;
                    };

                    if c.user_data != (eid as u128) {
                        error!("Colliders out of sync: id mismatch: {eid:?} {packet:?}");
                    }

                    // These setters trigger dirty bits for extra processing, so it is worth
                    // the explicit change detection here.
                    //
                    // Due to https://github.com/dimforge/parry/issues/51 we use ptr_eq and
                    // rely on shapes being statics; revisit this for performance if an
                    // entity ever dynamically computes its shape on every tick.
                    if Arc::ptr_eq(&c.shared_shape().0, &shape.0) {
                        c.set_shape(shape);
                    }
                    if c.position().translation != origin.into() {
                        c.set_position(origin.into());
                    }
                    self.mutated_colliders.push(collider);
                } else {
                    let mut c = ColliderBuilder::new(shape).position(origin.into()).build();
                    c.user_data = eid as u128;
                    let k = self.collider_set.insert(c);
                    self.mutated_colliders.push(k);
                    self.colliders[eid] = Some(k);
                }
            }
        }
    }

    pub fn handle_player_resource(&mut self, entity: &PacketEntity, _parser_state: &ParserState) {
        for prop in &entity.props {
            let Some((table_name, prop_name)) = prop.identifier.names() else {
                error!("Unknown player resource prop: {:?}", prop);
                continue;
            };

            if let Ok(player_id) = prop_name.as_str().parse::<u32>() {
                let round_state = self.round_state;

                let entity_id = EntityId::from(player_id);
                let steamid = self
                    .user_entities
                    .get(&entity_id)
                    .and_then(|uid| self.user_id_to_steam_id.get(uid))
                    .cloned();

                if let Some(steamid) = steamid {
                    if let Some(player) = self.player_summaries.get_mut(&steamid) {
                        match table_name.as_str() {
                            "m_iTeam" => {}
                            "m_iHealing" => {
                                let hi = i64::try_from(&prop.value).unwrap_or_default();
                                if hi < 0 {
                                    error!("Negative healing of {hi} by {}", player.name);
                                    return;
                                }
                                let h = hi as u32;

                                // Skip the first real value; sometimes STV starts a little late and
                                // we can't distinguish the healing values.
                                if player.scoreboard_healing == 0 {
                                    player.scoreboard_healing = h;
                                    return;
                                }

                                // Add up deltas, as this tracker resets to 0 mid round.
                                let dh = h.saturating_sub(player.scoreboard_healing);
                                if dh > 300 {
                                    // Never saw a delta this large in our corpus; may be a sign of
                                    // a miscount
                                    warn!("Huge healing delta of {dh} by {}", player.name);
                                }

                                player.handle_healing(round_state, dh);

                                player.scoreboard_healing = h;
                            }
                            "m_iTotalScore" => {
                                player.points =
                                    Some(i64::try_from(&prop.value).unwrap_or_default() as u32)
                            }
                            "m_iDamage" => {
                                player.scoreboard_damage =
                                    Some(i64::try_from(&prop.value).unwrap_or_default() as u32)
                            }
                            "m_iDeaths" => {
                                player.scoreboard_deaths =
                                    Some(i64::try_from(&prop.value).unwrap_or_default() as u32)
                            }
                            "m_iScore" => {
                                // iScore is close to number of kills; but counts post-game kills and decrements on suicide.
                                player.scoreboard_kills =
                                    Some(i64::try_from(&prop.value).unwrap_or_default() as u32)
                            }
                            "m_iBonusPoints" => {
                                player.bonus_points =
                                    Some(i64::try_from(&prop.value).unwrap_or_default() as u32)
                            }
                            "m_iPlayerClass" => {}
                            "m_iPlayerLevel" => {}
                            "m_bAlive" => {}
                            "m_flNextRespawnTime" => {}
                            "m_iActiveDominations" => {}
                            "m_iDamageAssist" => {}
                            "m_iPing" => {}
                            "m_iChargeLevel" => {}
                            "m_iStreaks" => {}
                            "m_iHealth" => {}
                            "m_iMaxHealth" => {}
                            "m_iMaxBuffedHealth" => {}
                            "m_iPlayerClassWhenKilled" => {}
                            "m_bValid" => {}
                            "m_iUserID" => {}
                            "m_iConnectionState" => {}
                            "m_flConnectTime" => {}
                            "m_iDamageBoss" => {}
                            "m_bArenaSpectator" => {}
                            "m_iHealingAssist" => {}
                            "m_iBuybackCredits" => {}
                            "m_iUpgradeRefundCredits" => {}
                            "m_iCurrencyCollected" => {}
                            "m_iDamageBlocked" => {}
                            "m_iAccountID" => {}
                            "m_bConnected" => {}
                            x => {
                                error!("Unhandled player resource type: {x}");
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn handle_game_rules(&mut self, entity: &PacketEntity, _parser_state: &ParserState) {
        for prop in &entity.props {
            match (prop.identifier, &prop.value) {
                (WAITING_FOR_PLAYERS, SendPropValue::Integer(x)) => {
                    self.waiting_for_players = *x == 1;
                    trace!("Waiting for players: {}", self.waiting_for_players);
                }
                (ROUND_STATE, SendPropValue::Integer(x)) => match RoundState::try_from(*x as u16) {
                    Ok(x) => self.round_state = x,
                    Err(e) => error!("Could not parse RoundState: {e}"),
                },
                (id, value) => {
                    trace!("Unhandled game rule: {:?} {value:?}", id.names());
                }
            }
        }
    }

    pub fn get_entity_by_handle(&self, handle: &u32) -> Option<&dyn Entity> {
        self.entity_handles
            .get(handle)
            .and_then(|eid| self.entities.get(usize::from(*eid)).map(|b| b.as_ref()))
            .flatten()
            .map(|v| &**v)
    }

    pub fn get_entity(&self, eid: impl Into<usize>) -> Option<&dyn Entity> {
        self.entities
            .get(eid.into())
            .and_then(|b: &_| b.as_ref())
            .map(|v| &**v)
    }

    pub fn get_weapon(&self, handle: &u32) -> Option<&entity::Weapon> {
        self.get_entity_by_handle(handle).and_then(|e| {
            let z = e.weapon();
            if z.is_none() {
                error!("weapon handle {handle} in the map but entity is not a weapon {e:?}");
            }
            z
        })
    }

    pub fn get_player(&self, id: &EntityId) -> Option<&entity::Player> {
        self.entities
            .get(usize::from(*id))
            .and_then(|b| b.as_ref())
            .and_then(|b| b.player())
    }

    pub fn handle_player_death(&mut self, death: &PlayerDeathEvent) {
        debug!(
            "Player death {death:?} {} {:?}",
            self.waiting_for_players, self.round_state
        );
        if self.waiting_for_players {
            return;
        }

        if death.attacker == death.assister {
            error!("Self assist? {:?}", death);
        }

        let flags = EnumSet::<Death>::try_from_repr(death.death_flags).unwrap_or_else(|| {
            error!("Unknown death flags: {}", death.death_flags);
            EnumSet::<Death>::new()
        });

        let damage_type = DamageType::try_from(death.custom_kill).unwrap_or_else(|e| {
            error!(
                "Unknown kill damage type: {}, error: {e}",
                death.custom_kill
            );
            DamageType::Normal
        });

        let damage_bits =
            EnumSet::<Damage>::try_from_repr(death.damage_bits).unwrap_or_else(|| {
                error!("Unknown damage bits: {}", death.damage_bits);
                EnumSet::<Damage>::new()
            });

        let feigned = flags.contains(Death::Feign);

        let attacker_user_id = UserId::from(death.attacker as u32);
        let victim_user_id = UserId::from(death.user_id as u32);

        if victim_user_id == attacker_user_id {
            let steamid = self.user_id_to_steam_id.get(&attacker_user_id).cloned();
            if let Some(steamid) = steamid {
                if let Some(suicider) = self.player_summaries.get_mut(&steamid) {
                    if self.round_state != RoundState::TeamWin {
                        suicider.suicides += 1;
                    }
                } else {
                    error!("Unknown suicider steamid for user_id: {}", attacker_user_id);
                }
            } else {
                error!(
                    "Unknown suicider steamid mapping for user_id: {}",
                    attacker_user_id
                );
            }
            return;
        }

        let victim_steamid = self.user_id_to_steam_id.get(&victim_user_id).cloned();
        let Some(victim_steamid) = victim_steamid else {
            error!(
                "Unknown victim steamid mapping for user_id: {}",
                victim_user_id
            );
            return;
        };

        let victim_summary_for_eid_lookup = self.player_summaries.get(&victim_steamid);
        let Some(victim_summary_for_eid_lookup) = victim_summary_for_eid_lookup else {
            error!("Unknown victim summary for steamid: {}", victim_steamid);
            return;
        };
        let victim_eid = victim_summary_for_eid_lookup.entity_id;

        let Some(victim_e) = self.get_player(&victim_eid) else {
            error!("No victim entity for entity_id: {}", victim_eid);
            return;
        };
        let medigun_h = victim_e.weapon_handles[1];
        let charge_val = self.get_weapon(&medigun_h).map(|w| w.last_high_charge);

        let Some(victim) = self.player_summaries.get_mut(&victim_steamid) else {
            error!(
                "Failed to get mutable victim summary for steamid: {}",
                victim_steamid
            );
            return;
        };

        if victim.class == Class::Medic {
            if let Some(charge) = charge_val {
                victim.charge = charge;
            } else {
                error!("Med died without a secondary {medigun_h} {victim:?}");
            }
        }

        victim.handle_death(self.round_state, flags);

        let airshot = victim.in_air() && (self.tick - victim.started_flying > 16);

        let attacker_is_world = death.attacker == 0;
        let attacker_is_world_wep = death.weapon_def_index == 0xffff;
        if attacker_is_world || attacker_is_world_wep {
            return;
        }

        if feigned {
            return;
        }

        let attacker_steamid = self.user_id_to_steam_id.get(&attacker_user_id).cloned();
        let Some(attacker_steamid) = attacker_steamid else {
            error!(
                "Unknown attacker steamid mapping for user_id: {}",
                attacker_user_id
            );
            return;
        };

        let attacker_summary_for_eid_lookup = self.player_summaries.get(&attacker_steamid);
        let Some(attacker_summary_for_eid_lookup) = attacker_summary_for_eid_lookup else {
            error!("Unknown attacker summary for steamid: {}", attacker_steamid);
            return;
        };

        if self.round_state == RoundState::TeamWin {
            if let Some(attacker) = self.player_summaries.get_mut(&attacker_steamid) {
                attacker.stats.postround_kills += 1;
            } else {
                error!(
                    "Failed to get mutable attacker summary for steamid: {}",
                    attacker_steamid
                );
            }
        } else {
            if airshot {
                debug!("airshot by {}!", attacker_summary_for_eid_lookup.name);
            }
            let Some(attacker_e) = self.get_player(&attacker_summary_for_eid_lookup.entity_id)
            else {
                error!("Could not find entity for attacker {attacker_summary_for_eid_lookup:?}");
                return;
            };

            let Some(victim_e) = self.get_player(&victim_eid) else {
                error!("No victim entity for entity_id: {}", victim_eid);
                return;
            };
            let my_name =
                self.weapon_name_from_damage(damage_type, damage_bits, victim_e, attacker_e, None);

            if *my_name != format!("{}", death.weapon_log_class_name) {
                error!(
                    "log names disagree log:{} vs us:{}",
                    death.weapon_log_class_name, my_name
                );
            }

            trace!(
								"{}death with {} / {} damage_type:{damage_type:?} flags:{flags:?} bits:{damage_bits:?}   {death:?}",
								if damage_bits.contains(Damage::Blast) { "blast " } else { "" },
								death.weapon,
								death.weapon_log_class_name,
						);

            if let Some(attacker) = self.player_summaries.get_mut(&attacker_steamid) {
                attacker.handle_kill(self.round_state, my_name, flags, damage_type, airshot);
            } else {
                error!(
                    "Failed to get mutable attacker summary for steamid: {}",
                    attacker_steamid
                );
            }
        }

        if death.assister == 0xffff {
            return;
        }

        let assister_user_id = UserId::from(death.assister as u32);
        let assister_steamid = self.user_id_to_steam_id.get(&assister_user_id).cloned();
        if let Some(assister_steamid) = assister_steamid {
            if let Some(assister) = self.player_summaries.get_mut(&assister_steamid) {
                assister.handle_assist(self.round_state, flags);
            } else {
                error!("Unknown assister summary for steamid: {}", assister_steamid);
            }
        } else {
            error!(
                "Unknown assister steamid mapping for user_id: {}",
                assister_user_id
            );
        }
    }

    fn _get_player_summary(&self, eid: &EntityId) -> Option<&PlayerSummary> {
        self.user_entities
            .get(eid)
            .and_then(|uid| self.user_id_to_steam_id.get(uid))
            .and_then(|sid| self.player_summaries.get(sid))
    }

    fn get_player_summary_mut(&mut self, eid: &EntityId) -> Option<&mut PlayerSummary> {
        let steam_id = self
            .user_entities
            .get(eid)
            .and_then(|uid| self.user_id_to_steam_id.get(uid))?
            .clone();
        self.player_summaries.get_mut(&steam_id)
    }

    pub fn get_player_summary_mut_handle(&mut self, handle: &u32) -> Option<&mut PlayerSummary> {
        let eid = *self.entity_handles.get(handle)?;
        self.get_player_summary_mut(&eid)
    }

    pub fn handle_point_captured(&mut self, cap: &TeamPlayPointCapturedEvent) {
        trace!("Point captured {:?}", cap);

        for entity_id_val in cap.cappers.as_bytes() {
            let eid = EntityId::from(*entity_id_val as u32);
            if let Some(player) = self.get_player_summary_mut(&eid) {
                player.handle_capture();
            } else {
                error!("Could not lookup player with entity id {eid} in capture event");
            }
        }
    }

    pub fn handle_capture_blocked(&mut self, cap: &TeamPlayCaptureBlockedEvent) {
        trace!("Capture blocked {:?}", cap);

        let eid = EntityId::from(cap.blocker as u32);
        if let Some(player) = self.get_player_summary_mut(&eid) {
            player.handle_capture_blocked();
        } else {
            error!("Could not lookup player with entity id {eid} in capture blocked event");
        }
    }

    pub fn handle_player_hurt(&mut self, hurt: &PlayerHurtEvent) {
        trace!("Player hurt {:?}", hurt);

        let damage_type = DamageType::try_from(hurt.custom).unwrap_or_else(|e| {
            error!("Unknown hurt damage type: {}, error: {e}", hurt.custom);
            DamageType::Normal
        });

        let effect = DamageEffect::try_from(hurt.bonus_effect).unwrap_or_else(|e| {
            error!(
                "Unknown hurt damage effect: {}, error: {e}",
                hurt.bonus_effect
            );
            DamageEffect::Normal
        });

        // Note this doesn't map to actual schema weapons, and is wrong for any weapon
        // with a projectile where the user may swaps weapons before the projectile hits.
        //
        // https://github.com/ValveSoftware/source-sdk-2013/blob/a62efecf624923d3bacc67b8ee4b7f8a9855abfd/src/game/server/tf/tf_player.cpp#L10779
        let weapon_type = WeaponId::try_from(hurt.weapon_id).unwrap_or_else(|e| {
            error!("Unknown hurt weapon id {}, error: {e}", hurt.weapon_id);
            WeaponId::None
        });

        let fall_damage = hurt.attacker == 0
            && !hurt.crit
            && !hurt.mini_crit
            && hurt.weapon_id == 0
            && hurt.custom == 0
            && hurt.bonus_effect == 0;
        if hurt.attacker == hurt.user_id || fall_damage {
            // No need to track self damage or fall damage for now
            // TODO: maybe for rocket jumping or uber building?
            return;
        }

        let attacker_user_id = UserId::from(hurt.attacker);
        let victim_user_id = UserId::from(hurt.user_id);

        let attacker_steamid = self.user_id_to_steam_id.get(&attacker_user_id).cloned();
        let Some(attacker_steamid) = attacker_steamid else {
            error!("Unknown attacker steamid mapping for user_id {attacker_user_id} in player hurt event");
            return;
        };
        let Some(attacker_summary_for_lookup) = self.player_summaries.get(&attacker_steamid) else {
            error!("Unknown attacker summary for steamid {attacker_steamid} in player hurt event");
            return;
        };
        let attacker_eid = attacker_summary_for_lookup.entity_id;
        let attacker_entity = self.get_player(&attacker_eid);
        let attacker_team = attacker_entity.map(|e| e.team).unwrap_or_default();
        let attacker_handle = attacker_entity.and_then(|p| p.handle()).unwrap_or_else(|| {
            error!("Player missing a handle??");
            INVALID_HANDLE
        });
        let attacker_class = attacker_summary_for_lookup.class;

        let victim_steamid = self.user_id_to_steam_id.get(&victim_user_id).cloned();
        let Some(victim_steamid) = victim_steamid else {
            error!(
                "Unknown victim steamid mapping for user_id {victim_user_id} in player hurt event"
            );
            return;
        };
        let Some(victim_summary_for_lookup) = self.player_summaries.get(&victim_steamid) else {
            error!("Unknown victim summary for steamid {victim_steamid} in player hurt event");
            return;
        };
        let victim_origin = victim_summary_for_lookup.origin;

        let mut source = HurtSource::Unknown;

        if attacker_class == Class::Engineer && damage_type == DamageType::Normal {
            let remove_idx = if let Some((idx, s)) = self
                .sentry_shots
                .iter()
                .enumerate()
                .find(|s| s.1.sentry.owner_entity == attacker_eid)
            {
                source = HurtSource::SentryShot((*s).clone());
                Some(idx)
            } else {
                None
            };

            if let Some(idx) = remove_idx {
                self.sentry_shots.swap_remove(idx);
            }
        }

        if matches!(source, HurtSource::Unknown) && damage_type != DamageType::Burning {
            trace!(
                "Check exps {attacker_handle} {} {:?}",
                self.airblasts.contains(&attacker_handle),
                self.explosions
            );
            let mut exps = self
                .explosions
                .iter()
                .filter(|e| {
                    e.projectile.owner == attacker_handle
												|| e.projectile.original_owner == attacker_handle
										// If the pyro reflects a projectile and it immediately hits a target in the
										// same tick, it gets destroyed without ever changing owner.
												|| self.airblasts.contains(&attacker_handle)
                })
                .map(|e| (e, EuclideanSpace::distance(&e.origin, &victim_origin)))
                .collect::<Vec<_>>();
            if !exps.is_empty() {
                trace!("look at explosions {:?}", exps);
                exps.sort_by(|a, b| a.1.total_cmp(&b.1));
                let playerbox =
                    Cuboid::new(Vector::new(49.0, 49.0, 83.0)).aabb(&victim_origin.into());

                let hit_exps = exps
                    .into_iter()
                    .filter(|(exp, _dist)| exp.projectile.check_hit(&playerbox))
                    .collect::<Vec<_>>();

                if let Some((e, _dist)) = hit_exps.first() {
                    trace!("Hit by explosion! {:?} damage_type:{damage_type:?} effect:{effect:?}  weapon_type:{weapon_type:?}     {hit_exps:?}", format!("{:?}-{:?}-{:?}", e.projectile.class_name, e.projectile.grenade_type, e.projectile.model_id.as_ref().and_then(|id| self.models.get(id))));

                    let mut e = (*hit_exps.first().unwrap().0).clone();
                    if self.airblasts.contains(&attacker_handle) {
                        e.projectile.is_reflected = true;
                        e.projectile.owner = attacker_handle;
                        e.projectile.team = attacker_team;
                    }

                    if entity::is_arrow(e.projectile.kind)
                        || e.projectile.kind == entity::ProjectileType::ScorchShotFlare
                        || e.projectile.kind == entity::ProjectileType::Cleaver
                        || e.projectile.kind == entity::ProjectileType::EnergyRing
                    {
                        source = HurtSource::NonBlastProjectile(e);
                    } else {
                        source = HurtSource::Explosion(e);
                    }
                }
            }
        }

        if hurt.attacker == 0 {
            if self.round_state == RoundState::TeamWin && hurt.damage_amount == 5000 {
                // Explosion at the end of some maps
                return;
            }

            // Huge fall damage amounts >=500 are typically kill zones like falling out of a map.
            if hurt.damage_amount <= 500
                && (damage_type != DamageType::Normal
                    || effect != DamageEffect::Crit
                    || weapon_type != WeaponId::None)
            {
                error!(
                    "Weird fall damage {} {damage_type:?} {effect:?} {weapon_type:?} {:?}",
                    hurt.damage_amount, self.round_state
                );
            }

            return;
        }

        let Some(attacker_summary_for_entity_lookup) = self.player_summaries.get(&attacker_steamid)
        else {
            error!("Unknown attacker summary for steamid {attacker_steamid} in player hurt event");
            return;
        };
        let Some(attacker_e) = self.get_player(&attacker_summary_for_entity_lookup.entity_id)
        else {
            error!("Unknown entity for attacker steamid {attacker_steamid}");
            return;
        };
        // attacker_class is already derived and available
        let attacker_wep = attacker_e.last_active_weapon_handle;

        let Some(victim_summary_for_entity_lookup) = self.player_summaries.get(&victim_steamid)
        else {
            error!("Unknown victim summary for steamid {victim_steamid} in player hurt event");
            return;
        };
        let Some(victim_e) = self.get_player(&victim_summary_for_entity_lookup.entity_id) else {
            error!("Unknown entity for victim steamid {}", victim_steamid);
            return;
        };

        let hurt_event = Hurt {
            victim: victim_user_id,
            attacker: attacker_user_id,
            wep: attacker_wep,
            origin: victim_origin,
            source,
        };
        let weapon_name = self.weapon_name_from_damage(
            damage_type,
            Default::default(),
            victim_e,
            attacker_e,
            Some(&hurt_event),
        );

        let Some(victim) = self.player_summaries.get_mut(&victim_steamid) else {
            error!(
                "Unknown victim summary (mut) for steamid {victim_steamid} in player hurt event"
            );
            return;
        };
        victim.handle_damage_taken(weapon_name, hurt, damage_type);

        if let Some(wep) = self.get_weapon(&attacker_wep) {
            let Some(wi) = self.schema.items.get(&wep.schema_id) else {
                error!("Weapon id {} not in schema", wep.schema_id);
                return;
            };
            let amount = hurt.damage_amount;
            debug!(
								"{victim_user_id} hurt by {attacker_user_id} {attacker_class:?} as {amount} x {damage_type:?} ({effect:?}) with {weapon_type:?} vs entity: {} / {:?}   explosions:{:?}   {hurt:?}",
								wep.class_name,
								wi.item_type_name,
								self.explosions
						);
        } else {
            error!(
                "hurt with {} but unknown player weapon handle: {attacker_wep} {hurt:?}",
                hurt.weapon_id
            );
        }
        let Some(attacker) = self.player_summaries.get_mut(&attacker_steamid) else {
            error!("Unknown attacker summary (mut) for steamid {attacker_steamid} in player hurt event");
            return;
        };

        attacker.handle_damage_dealt(weapon_name, hurt, damage_type);

        // TODO: Handle initial flamethrower hits; ignore
        if damage_type != DamageType::Burning
            && damage_type != DamageType::BurningFlare
            && !weapon::is_sentry(weapon_name)
        {
            attacker.handle_shot_hit(weapon_name);
        }

        if hurt.health == 0 {
            self.hurts.push(hurt_event);
        }
    }

    pub fn handle_tick(&mut self, tick: &DemoTick, server_tick: Option<&NetTickMessage>) {
        if *tick != self.tick {
            self.on_tick();
        }

        self.hurts.drain(..);
        self.sentry_shots.drain(..);
        self.airblasts.drain();
        self.deleted_entities.drain();

        self.tick = *tick;

        let server_tick = server_tick.map(|x| u32::from(x.tick)).unwrap_or(0);

        self.server_tick = server_tick;

        // Must explicitly drop the old span to avoid creating
        // a cycle where the new span points to the old span.
        self.span = None;

        self.span = Some(
            tracing::error_span!("Tick", tick = u32::from(*tick)) //, server_tick = server_tick,)
                .entered(),
        );
    }

    // Do processing at the end of a tick, once all entities have been
    // processed. This is important when referring to entities that
    // may have been both created and referenced in the same packet.
    fn on_tick(&mut self) {
        for v in self.player_summaries.values() {
            let Some(e) = self.get_player(&v.entity_id) else {
                continue;
            };
            if e.active_weapon_handle != 0 && e.active_weapon_handle != INVALID_HANDLE {
                let Some(_) = self.get_weapon(&e.active_weapon_handle) else {
                    error!("could not find weapon handle {:?}", e.active_weapon_handle);
                    continue;
                };
            }
        }

        let t: Vec<_> = self.tick_events.drain(..).collect();
        for e in t {
            match e {
                Event::Death(death) => {
                    self.handle_player_death(&death);
                }
                Event::Hurt(hurt) => {
                    self.handle_player_hurt(&hurt);
                }
                Event::MedigunCharged(handle) => {
                    let Some(owner_uid) = self.weapon_owners.get(&handle) else {
                        error!("No owner for medigun {handle} when it was charged");
                        continue;
                    };
                    let Some(steamid) = self.user_id_to_steam_id.get(owner_uid).cloned() else {
                        error!("No steamid for owner uid {owner_uid} of medigun {handle}");
                        continue;
                    };
                    let Some(medigun) = self.get_weapon(&handle) else {
                        error!("Med charged without a secondary {handle}");
                        continue;
                    };
                    let Some(item) = self.schema.items.get(&medigun.schema_id) else {
                        error!(
                            "Med charged with an unknown medigun defindex: {}",
                            medigun.schema_id
                        );
                        continue;
                    };
                    let Some(player) = self.player_summaries.get_mut(&steamid) else {
                        error!("Invalid owner steamid {steamid} for medigun {handle} when it was charged");
                        continue;
                    };
                    player.handle_charged(item);
                }
            }
        }

        self.explosions.clear();
    }

    fn handle_user_message(&mut self, msg: &UserMessage) {
        match msg {
            UserMessage::SayText2(msg) => {
                self.chat.push(ChatMessage {
                    tick: self.tick,
                    user: self
                        .user_entities
                        .get(&msg.client)
                        .and_then(|uid| self.user_id_to_steam_id.get(uid))
                        .map(|s| s.clone())
                        .unwrap_or("".to_string()),
                    message: msg.text.to_string(),
                    is_dead: matches!(
                        msg.kind,
                        ChatMessageKind::ChatAllDead | ChatMessageKind::ChatTeamDead
                    ),
                    is_team: matches!(
                        msg.kind,
                        ChatMessageKind::ChatTeam | ChatMessageKind::ChatTeamDead
                    ),
                    is_spec: matches!(msg.kind, ChatMessageKind::ChatAllSpec),
                    is_name_change: matches!(msg.kind, ChatMessageKind::NameChange),
                });
            }
            e => {
                trace!("Unhandled user message type {e:?}")
            }
        }
    }
}

impl MessageHandler for MatchAnalyzer<'_> {
    type Output = DemoSummary;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(
            message_type,
            MessageType::PacketEntities
                | MessageType::GameEvent
                | MessageType::NetTick
                | MessageType::TempEntities
                | MessageType::UserMessage
        )
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) {
        if tick != self.tick {
            self.handle_tick(&tick, None);
            self.tick = tick;
        }
        match message {
            Message::NetTick(t) => self.handle_tick(&tick, Some(t)),
            Message::PacketEntities(message) => {
                self.mutated_colliders.drain(..);
                self.removed_colliders.drain(..);

                for entity in message.entities.iter() {
                    self.handle_packet_entity(entity, parser_state);
                }
                if !self.mutated_colliders.is_empty() || !self.removed_colliders.is_empty() {
                    self.world.update_incremental(
                        &self.collider_set,
                        &self.mutated_colliders,
                        &self.removed_colliders,
                        true,
                    );
                }
            }
            Message::UserMessage(ue) => self.handle_user_message(ue),
            Message::TempEntities(te) => {
                for e in &te.events {
                    let Some(class) = parser_state
                        .server_classes
                        .get(<ClassId as Into<usize>>::into(e.class_id))
                    else {
                        error!("Unknown temp entity class: {}", e.class_id);
                        continue;
                    };

                    if class.name == "CTEPlayerAnimEvent" {
                        let mut event: Option<u32> = None;
                        let mut player: Option<u32> = None;
                        for p in &e.props {
                            match (p.identifier, &p.value) {
                                (ANIM_ID, &SendPropValue::Integer(x)) => event = Some(x as u32),
                                (ANIM_PLAYER, &SendPropValue::Integer(x)) => {
                                    player = Some(x as u32);
                                }
                                _ => {}
                            }
                        }
                        if let (Some(event), Some(player)) = (event, player) {
                            let Ok(event) = PlayerAnimation::try_from_primitive(event) else {
                                error!("Invalid animation type in {e:?}");
                                continue;
                            };
                            if event == PlayerAnimation::AttackSecondary {
                                let Some(p) = self
                                    .entity_handles
                                    .get(&player)
                                    .and_then(|eid| self.get_player(eid))
                                else {
                                    error!("Invalid player handle {player} in anim event");
                                    continue;
                                };
                                if p.class == Class::Pyro
                                    && p.active_weapon_handle == p.weapon_handles[0]
                                {
                                    self.airblasts.insert(player);
                                }
                            } else {
                                trace!("Unhandled animation type {event:?}: {te:?}");
                            }
                        }
                    } else if class.name == "CTEEffectDispatch" {
                        let mut entity = None;
                        let mut name_id = None;
                        let mut raw_dmg_type = 0;
                        let mut origin = Vec3::default();
                        let mut start = Vec3::default();
                        for p in &e.props {
                            match (p.identifier, &p.value) {
                                (EFFECT_ENTITY, &SendPropValue::Integer(x)) => {
                                    entity = Some((x as u32) + 1);
                                }
                                (EFFECT_NAME, &SendPropValue::Integer(x)) => {
                                    name_id = Some(x as u32);
                                }
                                (EFFECT_DAMAGE_TYPE, &SendPropValue::Integer(x)) => {
                                    raw_dmg_type = x as u32;
                                }
                                (EFFECT_ORIGIN_X, &SendPropValue::Float(x)) => {
                                    origin.x = x as f32;
                                }
                                (EFFECT_ORIGIN_Y, &SendPropValue::Float(y)) => {
                                    origin.y = y as f32;
                                }
                                (EFFECT_ORIGIN_Z, &SendPropValue::Float(z)) => {
                                    origin.z = z as f32;
                                }
                                (EFFECT_START_X, &SendPropValue::Float(x)) => {
                                    start.x = x as f32;
                                }
                                (EFFECT_START_Y, &SendPropValue::Float(y)) => {
                                    start.y = y as f32;
                                }
                                (EFFECT_START_Z, &SendPropValue::Float(z)) => {
                                    start.z = z as f32;
                                }
                                _ => {}
                            }
                        }

                        let _damage_bits = EnumSet::<Damage>::try_from_repr(raw_dmg_type)
                            .unwrap_or_else(|| {
                                error!("Unknown damage bits: {}", raw_dmg_type);
                                EnumSet::<Damage>::new()
                            });

                        let name =
                            name_id.map(|id| self.effects.get(&id).map(|e: &String| e.as_str()));

                        let (Some(entity), Some(name)) = (entity, name) else {
                            trace!("Effect does not have both name:{name:?} and an entity:{entity:?} from {e:?}");
                            return;
                        };
                        let Some(ent) = self.entities.get(entity as usize).and_then(|e| e.as_ref())
                        else {
                            // This is expected to rarely happen when a sentry is destroyed on the
                            // same tick that it shoot; otherwise it is an issue.
                            if !self.deleted_entities.contains(&EntityId::from(entity)) {
                                error!("Unknown entity from effect dispatch: {entity}");
                            }
                            return;
                        };
                        let Some(name) = name else {
                            error!("Unknown effect name for id {name_id:?}");
                            return;
                        };

                        trace!("effect dispatch to ent {name:?} {e:?} {ent:?}");

                        if let Some(sentry) = ent.sentry() {
                            self.sentry_shots.push(SentryShot {
                                sentry: sentry.clone(),
                            });
                        }

                        if name == "Impact" {
                            let explosions = self
                                .explosions
                                .iter()
                                .map(|e| (e, EuclideanSpace::distance(&e.origin, &origin)))
                                .collect::<Vec<_>>();

                            if let Some((explosion, _)) =
                                explosions.iter().max_by(|x, y| x.1.total_cmp(&y.1))
                            {
                                if explosion.projectile.kind == ProjectileType::HealingBolt
                                    && !explosion.projectile.is_reflected
                                {
                                    let o = explosion.projectile.owner;
                                    let Some(attacker) = self.get_player_summary_mut_handle(&o)
                                    else {
                                        error!("Could not find player that fired healing bolt");
                                        continue;
                                    };
                                    attacker.handle_shot_hit("crusaders_crossbow");
                                }
                            }
                        }
                    } else if class.name == "CTEFireBullets" {
                        let mut player = None;
                        for p in &e.props {
                            if let (FIRE_BULLETS_PLAYER, &SendPropValue::Integer(x)) =
                                (p.identifier, &p.value)
                            {
                                // Player ids here are offset by 1
                                // https://github.com/ValveSoftware/source-sdk-2013/blob/0565403b153dfcde602f6f58d8f4d13483696a13/src/game/server/tf/tf_fx.cpp#L80
                                player = Some(EntityId::from((x + 1) as u32));
                            }
                        }

                        if player.is_none() {
                            // This seems to just rarely be missing off events
                            debug!("No player entity for firebullets {e:?}");
                            continue;
                        }

                        let Some(pe) = player.and_then(|id| self.get_player(&id)) else {
                            error!(
                                "Could not find player entity for firebullets player {player:?}"
                            );
                            continue;
                        };

                        let Some(weapon) = self.get_weapon(&pe.last_active_weapon_handle) else {
                            error!(
                                "Could not find active weapon ({}) for player that fired bullets {player:?}",
																pe.last_active_weapon_handle
                            );
                            continue;
                        };
                        let Some(item) = self.schema.items.get(&weapon.schema_id) else {
                            error!(
                                "Could not find item schema for weapon ({}) for fired bullets {player:?}",
																weapon.schema_id
                            );
                            continue;
                        };

                        let name = weapon::weapon_name(item, pe.class);

                        let uid = pe.user_id;
                        let Some(sid) = self.user_id_to_steam_id.get(&uid) else {
                            error!("Could not find steamid for player for firebullets {uid:?}");
                            continue;
                        };
                        let Some(p) = self.player_summaries.get_mut(sid) else {
                            error!("Could not find player for firebullets {player:?}");
                            continue;
                        };

                        p.handle_fire_shot(name);
                    } else {
                        debug!("Unknown temp entity {}: {:?}", class.name, e);
                    }
                }
            }
            Message::GameEvent(GameEventMessage { event, .. }) => match event {
                GameEvent::PlayerDeath(death) => {
                    self.tick_events.push(Event::Death(death.clone()));
                }
                GameEvent::PlayerHurt(hurt) => {
                    self.tick_events.push(Event::Hurt(hurt.clone()));
                }

                GameEvent::TeamPlayPointCaptured(cap) => self.handle_point_captured(cap),
                GameEvent::TeamPlayCaptureBlocked(block) => self.handle_capture_blocked(block),

                GameEvent::TeamPlayWinPanel(e) => {
                    for entity_id_val in [e.player_1, e.player_2, e.player_3] {
                        let eid = EntityId::from(entity_id_val as u32);
                        let steamid = self
                            .user_entities
                            .get(&eid)
                            .and_then(|uid| self.user_id_to_steam_id.get(uid));
                        if let Some(steamid) = steamid {
                            if let Some(p) = self.player_summaries.get(steamid) {
                                self.current_round.mvps.push(p.steamid.clone());
                            }
                        }
                    }
                }

                GameEvent::TeamPlayRoundWin(e) => {
                    let winner = Team::try_from(e.team).unwrap_or_else(|_| {
                        error!("Unknown team id won round: {}", e.team);
                        Team::Spectator // Weird, but "Team::Other" is used for stalemates!
                    });

                    self.current_round.time = e.round_time;
                    self.current_round.is_sudden_death = e.was_sudden_death != 0;

                    if winner == Team::Red || winner == Team::Blue {
                        self.current_round.winner = Some(winner);

                        let loser = if winner == Team::Red {
                            Team::Blue
                        } else {
                            Team::Red
                        };

                        for p in self
                            .player_summaries
                            .values()
                            // ignore players that have left
                            .filter(|p| p.tick_end.is_none())
                        {
                            let Some(pe) = self.get_player(&p.entity_id) else {
                                error!("Missing player at round end {:?}", p.entity_id);
                                continue;
                            };
                            if pe.team == winner {
                                self.current_round.winners.push(p.steamid.clone());
                            } else if pe.team == loser {
                                self.current_round.losers.push(p.steamid.clone());
                            } // else: spec, or never joined a team
                        }
                    } else if winner == Team::Other {
                        self.current_round.is_stalemate = true;

                        let mut losers = vec![];
                        for (p, _pe) in self
                            .player_summaries
                            .values()
                            .filter(|p| p.tick_end.is_none())
                            .filter_map(|p| self.get_player(&p.entity_id).map(|pe| (p, pe)))
                            .filter(|(_p, pe)| pe.team == Team::Red || pe.team == Team::Blue)
                        {
                            losers.push(p.steamid.clone());
                        }
                        self.current_round.losers = losers;
                    }

                    // Populate players for the round that just ended
                    for player_summary in self.player_summaries.values() {
                        // Optionally filter for players active in this round if needed,
                        // for now, we take a snapshot of all known players.
                        // Players who left mid-round will have their stats up to that point.
                        self.current_round.players.push(player_summary.clone());
                    }
                    self.current_round
                        .players
                        .sort_by_cached_key(|p| p.steamid.clone());

                    self.rounds.push(std::mem::take(&mut self.current_round));

                    // Reset stats for all players for the new round
                    for player_summary in self.player_summaries.values_mut() {
                        player_summary.reset_stats();
                    }
                }

                // Some STVs demos don't have these events; they are
                // present in PoV demos and some STV demos (possibly
                // based on server side plugins?)
                GameEvent::PlayerDisconnect(d) => debug!("PlayerDisconnect {d:?}"),
                GameEvent::PlayerHealed(heal) => debug!("PlayerHealed {heal:?}"),
                GameEvent::PlayerInvulned(invuln) => debug!("PlayerDisconnect {invuln:?}"),
                GameEvent::PlayerChargeDeployed(c) => debug!("PlayerChargeDeployed {c:?}"),
                // GameEvent::TeamPlayRoundStalemate

                // Uninteresting
                GameEvent::HLTVStatus(_) => {}
                GameEvent::TeamPlayBroadcastAudio(_) => {}
                GameEvent::TeamPlayGameOver(_) => {}

                GameEvent::ObjectDestroyed(e) => {
                    if self.round_state != RoundState::Running {
                        return;
                    }

                    let attacker_uid = UserId::from(e.attacker);

                    let mut weapon: &'static str = ustr::ustr(e.weapon.as_ref()).as_str();
                    if matches!(e.weapon, MaybeUtf8String::Invalid(_))
                        || weapon == "building_carried_destroyed"
                    {
                        let steamid = self.user_id_to_steam_id.get(&attacker_uid).cloned();
                        if let Some(steamid) = steamid {
                            if let Some(player_summary) = self.player_summaries.get(&steamid) {
                                if let Some(player_ent) = self.get_player(&player_summary.entity_id)
                                {
                                    if let Some(item) = self
                                        .get_weapon(&player_ent.last_active_weapon_handle)
                                        .and_then(|w| self.schema.items.get(&w.schema_id))
                                    {
                                        weapon = weapon::weapon_name(item, player_ent.class);
                                    } else {
                                        // Could not get weapon item, proceed with original weapon name if any
                                    }
                                } else {
                                    error!("Could not find player entity {} for object destroyed event", player_summary.entity_id);
                                }
                            } else {
                                error!("Could not find player summary for steamid of attacker_uid {attacker_uid} for object destroyed event");
                            }
                        } else {
                            error!("Could not find steamid for attacker_uid {attacker_uid} for object destroyed event");
                        }
                    }

                    let steamid = self.user_id_to_steam_id.get(&attacker_uid).cloned();
                    if let Some(steamid) = steamid {
                        if let Some(attacker) = self.player_summaries.get_mut(&steamid) {
                            attacker.handle_object_destroyed(weapon);
                        } else {
                            error!("Could not find attacker summary for steamid {steamid} that destroyed building {e:?}");
                        }
                    } else {
                        error!("Could not find steamid for attacker_uid {attacker_uid} that destroyed building {e:?}");
                    }
                }

                _ => {
                    trace!("Unhandled game event: {event:?}");
                }
            },
            _ => {
                trace!("Unhandled message: {message:?}");
            }
        }
    }

    fn handle_string_entry(
        &mut self,
        table: &str,
        index: usize,
        entry: &StringTableEntry,
        _parser_state: &ParserState,
    ) {
        if table == "userinfo" {
            let _ = self.parse_user_info(
                index,
                entry.text.as_ref().map(|s| s.as_ref()),
                entry.extra_data.as_ref().map(|data| data.data.clone()),
            );
        } else if table == "modelprecache" {
            self.models.insert(
                index as u32,
                entry
                    .text
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or("".to_string()),
            );
        } else if table == "EffectDispatch" {
            self.effects.insert(
                index as u32,
                entry
                    .text
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or("".to_string()),
            );
        }
    }

    fn handle_data_tables(
        &mut self,
        parse_tables: &[ParseSendTable],
        server_classes: &[ServerClass],
        _parser_state: &ParserState,
    ) {
        fn dfs<'a>(
            graph: &'a HashMap<&'a str, Vec<&'a str>>,
            start_node: &'a str,
        ) -> HashSet<&'a str> {
            let mut visited = HashSet::new();
            let mut stack = Vec::new();

            stack.push(start_node);

            while let Some(node) = stack.pop() {
                if !visited.contains(node) {
                    visited.insert(node);

                    if let Some(neighbors) = graph.get(node) {
                        stack.extend(neighbors);
                    }
                }
            }

            visited
        }

        let mut classes = HashMap::<&str, ClassId>::new();
        for table in server_classes {
            classes.insert(table.data_table.as_str(), table.id);
        }

        let mut edges = HashMap::<&str, Vec<&str>>::new();
        for table in parse_tables {
            let name = table.name.as_str();
            if let Some(baseclass) = table.props.iter().find(|p| p.name == "baseclass") {
                if let Some(basename) = &baseclass.table_name {
                    edges.entry(basename).or_default().push(name);
                }
            }
        }

        for weapon_name in dfs(&edges, "DT_BaseCombatWeapon") {
            if let Some(id) = classes.get(weapon_name) {
                self.weapon_class_ids.insert(*id);
            } else {
                error!("No class id for weapon {weapon_name}");
            }
        }

        for projectile_name in dfs(&edges, "DT_BaseProjectile") {
            if let Some(id) = classes.get(projectile_name) {
                self.projectile_class_ids.insert(*id);
            } else {
                error!("No class id for projectile {projectile_name}");
            }
        }
    }

    fn into_output(mut self, _parser_state: &ParserState) -> <Self as MessageHandler>::Output {
        // If the demo ends mid-round, capture the state of the current_round
        // We can check if current_round has any meaningful data, e.g., time > 0 or specific events occurred.
        // A simple check could be if any players have stats, or if round_state indicates it started.
        // For now, we'll assume if self.current_round.time > 0 or if it's not default, it's a partial round.
        // A more robust check might be needed depending on how RoundSummary is populated.
        // Let's assume if there are any players, or if round time is set, it's a round.
        if self.current_round.time > 0.0
            || !self.player_summaries.is_empty() && self.rounds.is_empty()
            || (self.round_state != RoundState::default()
                && self.round_state != RoundState::Pregame)
        {
            for player_summary in self.player_summaries.values() {
                self.current_round.players.push(player_summary.clone());
            }
            self.current_round
                .players
                .sort_by_cached_key(|p| p.steamid.clone());
            self.rounds.push(std::mem::take(&mut self.current_round));
        }

        // Update tick_end for all players in self.player_summaries who are still "connected"
        // This ensures their global connection span is correctly recorded.
        // The PlayerSummary objects within each round are snapshots and won't be affected here.
        for summary in self.player_summaries.values_mut() {
            if summary.tick_start.is_none() {
                // Player might have info but never fully entered an entity processing loop
                summary.tick_start = Some(self.tick);
            }
            if summary.tick_end.is_none() {
                summary.tick_end = Some(self.tick);
            }
        }

        DemoSummary {
            rounds: self.rounds,
            chat: self.chat,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_demo_parser::{
        demo::{
            data::userinfo::PlayerInfo,
            message::{packetentities::BaselineIndex, Message},
            packet::{datatable::ServerClass, stringtable::StringTableEntry},
            parser::gamestateanalyser::UserId,
        },
        ParserState,
    };

    // Returns ServerClass with 'static lifetime for strings
    fn create_mock_server_class(name: &str, id: ClassId) -> ServerClass {
        ServerClass {
            id,
            name: name.to_string().into(),
            data_table: name.to_string().into(),
        }
    }

    // Returns a StringTableEntry suitable for handle_string_entry
    fn create_mock_user_info<'a>(
        name: &'a str,
        steam_id: &'a str,
        user_id_val: u16,
        entity_id_val: u32,
    ) -> StringTableEntry<'a> {
        let player_info = PlayerInfo {
            name: name.into(),
            steam_id: steam_id.into(),
            user_id: UserId::from(user_id_val),
            ..Default::default()
        };
        let user_info = UserInfo {
            player_info,
            entity_id: EntityId::from(entity_id_val),
            ..Default::default()
        };
        user_info.encode_to_string_table().unwrap()
    }

    fn create_mock_player_entity_enter_message(
        entity_id_val: u32,
        class_id: ClassId,
    ) -> Message<'static> {
        Message::PacketEntities(
            tf_demo_parser::demo::message::packetentities::PacketEntitiesMessage {
                entities: vec![PacketEntity {
                    entity_index: EntityId::from(entity_id_val),
                    server_class: class_id,
                    props: vec![],
                    update_type: UpdateType::Enter,
                    serial_number: 0,
                    baseline_index: BaselineIndex::First,
                    delta: None,
                    in_pvs: true,
                    delay: None,
                }],
                removed_entities: vec![],
                max_entries: ENTITY_COUNT as u16,
                delta: None,
                updated_base_line: false,
                base_line: BaselineIndex::First,
            },
        )
    }

    const EXAMPLE_STEAMID: &'static str = "STEAM_0:1:67890";

    #[test]
    fn test_single_player_summary() {
        let example_entity_id = 123;
        let player_class_id = ClassId::from(5);

        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let mut parser_state = ParserState::new(0, |_| true, false);

        parser_state.server_classes = vec![create_mock_server_class("CTFPlayer", player_class_id)];
        analyzer.weapon_class_ids.insert(ClassId::from(0));
        analyzer.projectile_class_ids.insert(ClassId::from(0));

        // Simulate UserInfo update by directly calling handle_string_entry
        let user_info_s_entry =
            create_mock_user_info("Player1", EXAMPLE_STEAMID, 2, example_entity_id);
        analyzer.handle_string_entry("userinfo", 0, &user_info_s_entry, &parser_state);

        // Simulate Player entity creation
        let player_entity_msg = create_mock_player_entity_enter_message(1, player_class_id);
        analyzer.handle_message(&player_entity_msg, DemoTick::from(1), &parser_state);

        analyzer.on_tick();

        let summary = analyzer.into_output(&parser_state);

        assert_eq!(summary.rounds.len(), 1);
        assert_eq!(summary.rounds[0].players.len(), 1);
        assert_eq!(summary.rounds[0].players[0].name, "Player1");
        assert_eq!(summary.rounds[0].players[0].steamid, EXAMPLE_STEAMID);
        assert_eq!(summary.rounds[0].players[0].connection_count, 1);

        assert_eq!(summary.rounds[0].players[0].user_id, 2);

        // tf_demo_parser internally increments the entity id by one to account for TF2's encoding
        assert_eq!(
            summary.rounds[0].players[0].entity_id,
            EntityId::from(example_entity_id + 1)
        );
    }

    #[test]
    fn test_reconnecting_player_consolidated() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let mut parser_state = ParserState::new(0, |_| true, false);

        let player_class_id = ClassId::from(5);
        parser_state.server_classes = vec![create_mock_server_class("CTFPlayer", player_class_id)];
        analyzer.weapon_class_ids.insert(ClassId::from(0));
        analyzer.projectile_class_ids.insert(ClassId::from(0));

        // First connection
        let user_info_s_entry1 = create_mock_user_info("PlayerA", EXAMPLE_STEAMID, 3, 2);
        analyzer.handle_string_entry("userinfo", 0, &user_info_s_entry1, &parser_state);
        let player_entity_msg1 = create_mock_player_entity_enter_message(2, player_class_id);
        analyzer.handle_message(&player_entity_msg1, DemoTick::from(10), &parser_state);
        analyzer.on_tick();

        // TODO: Disconnect via entity destroyed

        // Second connection
        let user_info_s_entry2 = create_mock_user_info("PlayerA_NewName", EXAMPLE_STEAMID, 4, 56);
        analyzer.handle_string_entry("userinfo", 1, &user_info_s_entry2, &parser_state);
        let player_entity_msg2 = create_mock_player_entity_enter_message(3, player_class_id);
        analyzer.handle_message(&player_entity_msg2, DemoTick::from(100), &parser_state);
        analyzer.on_tick();

        let summary = analyzer.into_output(&parser_state);

        assert_eq!(summary.rounds.len(), 1);
        assert_eq!(summary.rounds[0].players.len(), 1);
        assert_eq!(summary.rounds[0].players[0].name, "PlayerA_NewName");
        assert_eq!(summary.rounds[0].players[0].steamid, EXAMPLE_STEAMID);
        assert_eq!(summary.rounds[0].players[0].connection_count, 2);

        assert_eq!(summary.rounds[0].players[0].user_id, 4);
        assert_eq!(
            summary.rounds[0].players[0].entity_id,
            EntityId::from(57u32)
        );
    }
}
