use crate::{
    Vec3,
    parser::{
        entity::{self, Entity, ProjectileType},
        game::{
            Damage, DamageEffect, DamageType, Death, INVALID_HANDLE, PlayerAnimation, RoundState,
            WeaponId,
        },
        is_false,
        player::PlayerSummary,
        props::*,
        weapon::{self, projectile_log_name, sentry_name, taunt_log_name},
    },
    schema::{Item, Schema},
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
    MessageType, ParserState, ReadResult, Stream,
    demo::{
        data::{DemoTick, MaybeUtf8String, UserInfo},
        gameevent_gen::{
            BuildingHealedEvent, CapperKilledEvent, CrossbowHealEvent,
            EnvironmentalDeathEvent, KilledCappingPlayerEvent, MedicDeathEvent,
            ObjectDeflectedEvent, ObjectDetonatedEvent, ObjectRemovedEvent,
            PayloadPushedEvent, PlayerBuiltObjectEvent, PlayerCarryObjectEvent,
            PlayerDeathEvent, PlayerDropObjectEvent, PlayerExtinguishedEvent,
            PlayerHealOnHitEvent, PlayerHealedEvent, PlayerHurtEvent,
            PlayerTeleportedEvent, PlayerUpgradedObjectEvent,
            ProjectileDirectHitEvent, TeamPlayCaptureBlockedEvent,
            TeamPlayPointCapturedEvent, TeamPlayPointStartCaptureEvent,
            VoteCastEvent, VoteChangedEvent, VoteFailedEvent, VoteOptionsEvent,
            VotePassedEvent, VoteStartedEvent,
        },
        gamevent::GameEvent,
        message::{
            Message, NetTickMessage,
            gameevent::GameEventMessage,
            packetentities::{EntityId, PacketEntity, UpdateType},
            usermessage::{ChatMessageKind, UserMessage},
        },
        packet::{
            datatable::{ClassId, ParseSendTable, ServerClass},
            stringtable::StringTableEntry,
        },
        parser::{
            MessageHandler,
            gamestateanalyser::{Class, Team, UserId},
        },
        sendprop::SendPropValue,
    },
};
use tracing::{debug, error, span::EnteredSpan, trace, warn};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DemoSummary {
    pub rounds: Vec<RoundSummary>,
    pub chat: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub votes: Vec<VoteSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sourcemod_votes: Vec<SourceModVote>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub point_captures: Vec<PointCaptureStart>,
}

/// A `teamplay_point_startcapture` event: a capture attempt began.
/// `cappers` holds the steamids of players on the point (best effort).
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PointCaptureStart {
    pub tick: DemoTick,
    pub cp: u8,
    pub cp_name: String,
    pub team: u8,
    pub cap_team: u8,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cappers: Vec<String>, // steamids
    pub cap_time: f32,
}

/// One ballot cast in a native TF2 vote (`vote_cast` game event).
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct VoteBallot {
    pub tick: DemoTick,
    pub voter_entity: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voter: Option<String>, // steamid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voter_name: Option<String>,
    /// 0-based option index (`vote_cast.vote_option`).
    pub option: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_name: Option<String>,
}

/// A native TF2 vote, correlated by `voteidx` across
/// `vote_started` / `vote_options` / `vote_cast` / `vote_changed` /
/// `vote_passed` / `vote_failed` / `vote_ended` game events.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct VoteSummary {
    pub voteidx: u32,
    pub tick_start: DemoTick,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_end: Option<DemoTick>,
    pub issue: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub param1: String,
    pub team: u8,
    /// Raw initiator client/entity index (`vote_started.initiator`).
    /// `99` means the server; then `initiator`/`initiator_name` are `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initiator_entity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initiator: Option<String>, // steamid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initiator_name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ballots: Vec<VoteBallot>,
    /// Last-seen per-option tallies from `vote_changed`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub counts: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub potential_votes: Option<u32>,
    /// `Some(true)` = passed, `Some(false)` = failed, `None` = no
    /// `vote_passed`/`vote_failed` event seen (ongoing or truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_param1: Option<String>,
}

/// One `"<name> wants to scramble teams"` / `"wants to rock the vote"`
/// trigger that feeds a SourceMod vote.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SmVoteInitiator {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steamid: Option<String>,
    pub tick: DemoTick,
    pub current: u32,
    pub required: u32,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SmNomination {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steamid: Option<String>,
    pub map: String,
    pub tick: DemoTick,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SmVoteOption {
    pub name: String,
    pub votes: u32,
}

/// A SourceMod vote reconstructed from `Text` user messages
/// (`PrintTalk` triggers/results, `PrintCenter` progress).
/// Individual ballots are not broadcast, so only aggregate `options`
/// tallies are available (unlike native votes, which list every voter).
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SourceModVote {
    /// `"scramble"` or `"map"`.
    pub kind: String,
    pub tick_start: DemoTick,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_end: Option<DemoTick>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub initiators: Vec<SmVoteInitiator>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub nominations: Vec<SmNomination>,
    pub total_votes: u32,
    pub potential_votes: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub options: Vec<SmVoteOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub tick: DemoTick,
    pub user: String, // steamid
    pub message: String,
    #[serde(skip_serializing_if = "is_false")]
    pub is_dead: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_team: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_spec: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_name_change: bool,
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

    vote_sessions: HashMap<u32, VoteSummary>, // voteidx -> in-progress native vote
    finished_votes: Vec<VoteSummary>, // closed native votes (voteidx reuse across maps)

    point_captures: Vec<PointCaptureStart>,

    sm_votes: Vec<SourceModVote>,
    sm_current: Option<SourceModVote>,
    sm_pending_scramble: Vec<SmVoteInitiator>,
    sm_pending_rtv: Vec<SmVoteInitiator>,
    sm_pending_nominations: Vec<SmNomination>,
    sm_map_announced_tick: Option<DemoTick>,
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

/// Resolve a vote client/entity index to `(steamid, name)`.
/// `99` (and `0`) mean the server, which has no player identity.
fn resolve_vote_entity(
    entity: u32,
    user_entities: &HashMap<EntityId, UserId>,
    user_id_to_steam_id: &HashMap<UserId, String>,
    player_summaries: &HashMap<String, PlayerSummary>,
) -> (Option<String>, Option<String>) {
    if entity == 99 || entity == 0 {
        return (None, None);
    }
    let eid = EntityId::from(entity);
    let steamid = user_entities
        .get(&eid)
        .and_then(|uid| user_id_to_steam_id.get(uid))
        .cloned();
    let name = steamid
        .as_ref()
        .and_then(|sid| player_summaries.get(sid))
        .map(|p| p.name.clone());
    (steamid, name)
}

/// `"FreaK wants to scramble teams. [1/4 votes required]"`
/// -> `("FreaK", 1, 4)`.
fn parse_scramble_trigger(text: &str) -> Option<(String, u32, u32)> {
    let (name, rest) = text.split_once(" wants to scramble teams. [")?;
    let rest = rest.strip_suffix(" votes required]")?;
    let (cur, req) = rest.split_once('/')?;
    Some((name.to_string(), cur.parse().ok()?, req.parse().ok()?))
}

/// `"[SM] moriya wants to rock the vote. (1 votes, 11 required)"`
/// -> `("moriya", 1, 11)`.
fn parse_rtv_trigger(text: &str) -> Option<(String, u32, u32)> {
    let text = text.strip_prefix("[SM] ")?.strip_suffix(')')?;
    let (name, rest) = text.split_once(" wants to rock the vote. (")?;
    let (cur, req) = rest.split_once(" votes, ")?;
    let req = req.strip_suffix(" required")?;
    Some((name.to_string(), cur.parse().ok()?, req.parse().ok()?))
}

/// `"[SM] SchwanzusLongus has nominated cp_process_final."`
/// -> `("SchwanzusLongus", "cp_process_final")`.
fn parse_nomination(text: &str) -> Option<(String, String)> {
    let text = text.strip_prefix("[SM] ")?.strip_suffix('.')?;
    let (name, map) = text.split_once(" has nominated ")?;
    Some((name.to_string(), map.to_string()))
}

/// `"Votes: 12/21, 9s left\n1. cp_process_final: (9)\n2. ..."`
/// -> `(12, 21, 9, options)`.
fn parse_vote_progress(text: &str) -> Option<(u32, u32, u32, Vec<SmVoteOption>)> {
    let mut lines = text.lines();
    let head = lines.next()?;
    let head = head.strip_prefix("Votes: ")?;
    let (counts, secs) = head.split_once(", ")?;
    let secs = secs.strip_suffix("s left")?;
    let (total, potential) = counts.split_once('/')?;
    let mut options = Vec::new();
    for line in lines {
        // `"1. cp_process_final: (9)"`
        let (_, rest) = line.split_once(". ")?;
        let (name, count) = rest.rsplit_once(": (")?;
        let count = count.strip_suffix(')')?;
        options.push(SmVoteOption {
            name: name.to_string(),
            votes: count.parse().ok()?,
        });
    }
    Some((
        total.parse().ok()?,
        potential.parse().ok()?,
        secs.parse().ok()?,
        options,
    ))
}

/// `"[SM] Map voting has finished. The next map will be koth_harvest_final.
/// (Received 61% of 13 votes)"` -> `("koth_harvest_final", 61, 13)`.
fn parse_map_finished(text: &str) -> Option<(String, u32, u32)> {
    let text = text.strip_prefix("[SM] Map voting has finished. The next map will be ")?;
    let (map, rest) = text.split_once(". (Received ")?;
    let rest = rest.strip_suffix(')')?;
    let (pct, votes) = rest.split_once("% of ")?;
    let votes = votes.strip_suffix(" votes")?;
    Some((map.to_string(), pct.parse().ok()?, votes.parse().ok()?))
}

/// Raw entity indices packed into `teamplay_point_startcapture.cappers`.
/// Observed on the wire as 1-3 raw bytes (e.g. `[7, 15, 16]` = three
/// cappers on entity slots 7/15/16), *not* a name list. Only byte values
/// below 64 are accepted: anything else means the server sent some other
/// (name-based?) format we don't decode, and resolving its bytes as entity
/// slots would misattribute strangers' stats.
fn parse_capper_entities(cappers: &MaybeUtf8String) -> Vec<u32> {
    let bytes: &[u8] = match cappers {
        MaybeUtf8String::Valid(s) => s.as_bytes(),
        MaybeUtf8String::Invalid(b) => b,
    };
    if bytes.is_empty() || bytes.iter().any(|b| *b >= 64) {
        return Vec::new();
    }
    bytes.iter().map(|b| *b as u32).collect()
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
            vote_sessions: Default::default(),
            finished_votes: Default::default(),
            point_captures: Default::default(),
            sm_votes: Default::default(),
            sm_current: Default::default(),
            sm_pending_scramble: Default::default(),
            sm_pending_rtv: Default::default(),
            sm_pending_nominations: Default::default(),
            sm_map_announced_tick: Default::default(),
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
                    if let HurtSource::Explosion(e) = &h.source
                        && (e.projectile.owner() == Some(attacker_handle)
                            || e.projectile.original_owner == attacker_handle
                            || self.airblasts.contains(&attacker_handle))
                    {
                        return Some(e);
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
                self.deleted_entities.insert(packet.entity_index);

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

                if let Some(steamid) = steamid
                    && let Some(player) = self.player_summaries.get_mut(&steamid)
                {
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
                if damage_bits.contains(Damage::Blast) {
                    "blast "
                } else {
                    ""
                },
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

    fn player_by_user_id_mut(&mut self, user_id: u16) -> Option<&mut PlayerSummary> {
        let steamid = self
            .user_id_to_steam_id
            .get(&UserId::from(user_id))
            .cloned()?;
        self.player_summaries.get_mut(&steamid)
    }

    fn player_by_entity_mut(&mut self, entity: u32) -> Option<&mut PlayerSummary> {
        self.get_player_summary_mut(&EntityId::from(entity))
    }

    /// Resolve an ambiguous byte-sized player ref. Field conventions split
    /// by event family: kill-attribution and heal/building fields verified
    /// against real demos carry entity indices here
    /// (`BuildingHealed.healer`, `PlayerHealOnHit.ent_index`), while
    /// `KilledCappingPlayer`/`CapperKilled` ids match entity slots too
    /// (a userid-first lookup demonstrably misattributed capping kills to
    /// SourceTV, whose low userid collides with live entity slots).
    /// Entity-first therefore wins; userid is the fallback for refs that
    /// are really userids (`CrossbowHeal`-style ids are handled userid-only
    /// at their call sites). 0 (world/none) resolves to nobody.
    fn player_by_ambiguous_id_mut(&mut self, id: u8) -> Option<&mut PlayerSummary> {
        if id == 0 {
            return None;
        }
        // Resolve to an owned steamid under immutable borrows first, then
        // take a single mutable borrow.
        let eid = EntityId::from(u32::from(id));
        let steamid = self
            .user_entities
            .get(&eid)
            .and_then(|uid| self.user_id_to_steam_id.get(uid))
            .filter(|sid| self.player_summaries.contains_key(*sid))
            .cloned()
            .or_else(|| {
                self.user_id_to_steam_id
                    .get(&UserId::from(u32::from(id)))
                    .filter(|sid| self.player_summaries.contains_key(*sid))
                    .cloned()
            })?;
        self.player_summaries.get_mut(&steamid)
    }

    pub fn handle_player_healed(&mut self, e: &PlayerHealedEvent) {
        trace!("Player healed {e:?}");
        if e.healer == 0 {
            return; // health kits etc. have no healer to credit
        }
        if let Some(healer) = self.player_by_user_id_mut(e.healer) {
            healer.handle_heal_given(u32::from(e.amount));
        } else {
            error!("Could not lookup healer with user id {} in player_healed", e.healer);
        }
    }

    pub fn handle_crossbow_heal(&mut self, e: &CrossbowHealEvent) {
        trace!("Crossbow heal {e:?}");
        if e.healer == 0 {
            return;
        }
        if let Some(healer) = self.player_by_user_id_mut(u16::from(e.healer)) {
            healer.handle_crossbow_heal(u32::from(e.amount));
        } else {
            error!("Could not lookup healer {} in crossbow_heal", e.healer);
        }
    }

    pub fn handle_player_heal_on_hit(&mut self, e: &PlayerHealOnHitEvent) {
        trace!("Player heal on hit {e:?}");
        if let Some(player) = self.player_by_entity_mut(u32::from(e.ent_index)) {
            player.handle_heal_on_hit(u32::from(e.amount));
        } else {
            error!(
                "Could not lookup player with entity id {} in player_heal_on_hit",
                e.ent_index
            );
        }
    }

    pub fn handle_player_extinguished(&mut self, e: &PlayerExtinguishedEvent) {
        trace!("Player extinguished {e:?}");
        if e.healer == 0 {
            return;
        }
        if let Some(healer) = self.player_by_ambiguous_id_mut(e.healer) {
            healer.handle_extinguish();
        } else {
            error!("Could not lookup healer {} in player_extinguished", e.healer);
        }
    }

    pub fn handle_building_healed(&mut self, e: &BuildingHealedEvent) {
        trace!("Building healed {e:?}");
        // `healer` is an entity index here (verified: sentry owner resolves
        // via entity slot, not userid).
        if let Some(healer) = self.player_by_entity_mut(u32::from(e.healer)) {
            healer.handle_building_heal(u32::from(e.amount));
        } else {
            error!(
                "Could not lookup healer with entity id {} in building_healed",
                e.healer
            );
        }
    }

    pub fn handle_medic_death(&mut self, e: &MedicDeathEvent) {
        trace!("Medic death {e:?}");
        if !e.charged {
            return;
        }
        if let Some(medic) = self.player_by_user_id_mut(e.user_id) {
            medic.handle_dropped_uber();
        } else {
            error!("Could not lookup medic with user id {} in medic_death", e.user_id);
        }
    }

    pub fn handle_object_deflected(&mut self, e: &ObjectDeflectedEvent) {
        trace!("Object deflected {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_reflect();
        } else {
            error!("Could not lookup player with user id {} in object_deflected", e.user_id);
        }
    }

    pub fn handle_killed_capping_player(&mut self, e: &KilledCappingPlayerEvent) {
        trace!("Killed capping player {e:?}");
        // Killer/victim verified as entity indices (they match the u16
        // CapperKilled ids for the same kills, and all values fall in live
        // entity slots).
        if let Some(killer) = self.player_by_entity_mut(u32::from(e.killer)) {
            killer.handle_defense();
        } else {
            error!(
                "Could not lookup killer {} in killed_capping_player",
                e.killer
            );
        }
    }

    pub fn handle_capper_killed(&mut self, e: &CapperKilledEvent) {
        trace!("Capper killed {e:?}");
        // Entity index (see above); the co-fired KilledCappingPlayer event
        // credits the same play again, so one stopped capper yields two
        // `defenses` on koth.
        if let Some(blocker) = self.player_by_entity_mut(u32::from(e.blocker)) {
            blocker.handle_defense();
        } else {
            error!("Could not lookup blocker {} in capper_killed", e.blocker);
        }
    }

    pub fn handle_projectile_direct_hit(&mut self, e: &ProjectileDirectHitEvent) {
        trace!("Projectile direct hit {e:?}");
        if e.attacker == 0 {
            return;
        }
        if let Some(attacker) = self.player_by_ambiguous_id_mut(e.attacker) {
            attacker.handle_direct_hit();
        } else {
            error!(
                "Could not lookup attacker {} in projectile_direct_hit",
                e.attacker
            );
        }
    }

    pub fn handle_player_teleported(&mut self, e: &PlayerTeleportedEvent) {
        trace!("Player teleported {e:?}");
        if let Some(builder) = self.player_by_user_id_mut(e.builder_id) {
            builder.handle_teleport();
        } else {
            error!(
                "Could not lookup builder {} in player_teleported",
                e.builder_id
            );
        }
    }

    pub fn handle_point_start_capture(&mut self, e: &TeamPlayPointStartCaptureEvent) {
        trace!("Point start capture {e:?}");
        let mut cappers = Vec::new();
        for entity in parse_capper_entities(&e.cappers) {
            if let Some(eid) = self
                .user_entities
                .get(&EntityId::from(entity))
                .and_then(|uid| self.user_id_to_steam_id.get(uid))
                .cloned()
            {
                cappers.push(eid);
            }
        }
        self.point_captures.push(PointCaptureStart {
            tick: self.tick,
            cp: e.cp,
            cp_name: e.cp_name.to_string(),
            team: e.team,
            cap_team: e.cap_team,
            cappers,
            cap_time: e.cap_time,
        });
    }

    pub fn handle_payload_pushed(&mut self, e: &PayloadPushedEvent) {
        trace!("Payload pushed {e:?}");
        if e.pusher == 0 {
            return;
        }
        if let Some(pusher) = self.player_by_ambiguous_id_mut(e.pusher) {
            pusher.handle_push(u32::from(e.distance));
        } else {
            error!("Could not lookup pusher {} in payload_pushed", e.pusher);
        }
    }

    pub fn handle_environmental_death(&mut self, e: &EnvironmentalDeathEvent) {
        trace!("Environmental death {e:?}");
        if let Some(victim) = self.player_by_ambiguous_id_mut(e.victim) {
            victim.handle_environmental_death();
        } else {
            error!("Could not lookup victim {} in environmental_death", e.victim);
        }
        if e.killer != 0 && e.killer != e.victim
            && let Some(killer) = self.player_by_ambiguous_id_mut(e.killer)
        {
            killer.handle_environmental_kill();
        }
    }

    pub fn handle_player_built_object(&mut self, e: &PlayerBuiltObjectEvent) {
        trace!("Player built object {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_placed();
        } else {
            error!(
                "Could not lookup player with user id {} in player_builtobject",
                e.user_id
            );
        }
    }

    pub fn handle_player_upgraded_object(&mut self, e: &PlayerUpgradedObjectEvent) {
        trace!("Player upgraded object {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_upgraded();
        } else {
            error!(
                "Could not lookup player with user id {} in player_upgradedobject",
                e.user_id
            );
        }
    }

    pub fn handle_player_carry_object(&mut self, e: &PlayerCarryObjectEvent) {
        trace!("Player carry object {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_carried();
        } else {
            error!(
                "Could not lookup player with user id {} in player_carryobject",
                e.user_id
            );
        }
    }

    pub fn handle_player_drop_object(&mut self, e: &PlayerDropObjectEvent) {
        trace!("Player drop object {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_dropped();
        } else {
            error!(
                "Could not lookup player with user id {} in player_dropobject",
                e.user_id
            );
        }
    }

    pub fn handle_object_removed(&mut self, e: &ObjectRemovedEvent) {
        trace!("Object removed {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_removed();
        } else {
            error!("Could not lookup player with user id {} in object_removed", e.user_id);
        }
    }

    pub fn handle_object_detonated(&mut self, e: &ObjectDetonatedEvent) {
        trace!("Object detonated {e:?}");
        if let Some(player) = self.player_by_user_id_mut(e.user_id) {
            player.handle_object_detonated();
        } else {
            error!(
                "Could not lookup player with user id {} in object_detonated",
                e.user_id
            );
        }
    }

    /// Resolve a vote client/entity index to `(steamid, name)`.
    /// `99` (and `0`) mean the server, which has no player identity.
    fn resolve_vote_entity(&self, entity: u32) -> (Option<String>, Option<String>) {
        resolve_vote_entity(
            entity,
            &self.user_entities,
            &self.user_id_to_steam_id,
            &self.player_summaries,
        )
    }

    fn steamid_for_name(&self, name: &str) -> Option<String> {
        self.player_summaries
            .values()
            .find(|p| p.name == name)
            .map(|p| p.steamid.clone())
    }

    fn vote_session_mut(&mut self, voteidx: u32) -> &mut VoteSummary {
        let tick = self.tick;
        self.vote_sessions
            .entry(voteidx)
            .or_insert_with(|| VoteSummary {
                voteidx,
                tick_start: tick,
                ..Default::default()
            })
    }

    pub fn handle_vote_started(&mut self, e: &VoteStartedEvent) {
        trace!("Vote started {e:?}");
        let tick = self.tick;
        // `voteidx` is a per-server counter that restarts across map changes
        // within one STV demo; a fresh `vote_started` for an already-closed
        // session starts a new vote rather than merging into the old one.
        if self
            .vote_sessions
            .get(&e.voteidx)
            .is_some_and(|s| s.passed.is_some() || s.tick_end.is_some())
            && let Some(old) = self.vote_sessions.remove(&e.voteidx)
        {
            self.finished_votes.push(old);
        }
        let (steamid, name) = self.resolve_vote_entity(e.initiator);
        let session = self.vote_sessions.entry(e.voteidx).or_insert_with(|| VoteSummary {
            voteidx: e.voteidx,
            tick_start: tick,
            ..Default::default()
        });
        // A fresh `vote_started` (re)initializes the session; keep any
        // already-seen options/ballots only if they belong to the same tick.
        // In practice events arrive in order, so overwrite the header fields.
        session.tick_start = tick;
        session.issue = e.issue.to_string();
        session.param1 = e.param_1.to_string();
        session.team = e.team;
        session.initiator_entity = (e.initiator != 99 && e.initiator != 0).then_some(e.initiator);
        session.initiator = steamid;
        session.initiator_name = name;
    }

    pub fn handle_vote_cast(&mut self, e: &VoteCastEvent) {
        trace!("Vote cast {e:?}");
        let tick = self.tick;
        let (steamid, name) = self.resolve_vote_entity(e.entity_id);
        let option_name = self
            .vote_sessions
            .get(&e.voteidx)
            .and_then(|s| s.options.get(e.vote_option as usize).cloned());
        let ballot = VoteBallot {
            tick,
            voter_entity: e.entity_id,
            voter: steamid,
            voter_name: name,
            option: e.vote_option,
            option_name,
        };
        self.vote_session_mut(e.voteidx).ballots.push(ballot);
    }

    pub fn handle_vote_options(&mut self, e: &VoteOptionsEvent) {
        trace!("Vote options {e:?}");
        let options = [
            e.option_1.to_string(),
            e.option_2.to_string(),
            e.option_3.to_string(),
            e.option_4.to_string(),
            e.option_5.to_string(),
        ];
        let count = usize::min(e.count as usize, options.len());
        let session = self.vote_session_mut(e.voteidx);
        session.options = options.into_iter().take(count).collect();
        // Backfill option names on ballots seen before the options event.
        for ballot in &mut session.ballots {
            ballot.option_name = session
                .options
                .get(ballot.option as usize)
                .cloned();
        }
    }

    pub fn handle_vote_changed(&mut self, e: &VoteChangedEvent) {
        trace!("Vote changed {e:?}");
        let session = self.vote_session_mut(e.voteidx);
        session.counts = vec![
            u32::from(e.vote_option_1),
            u32::from(e.vote_option_2),
            u32::from(e.vote_option_3),
            u32::from(e.vote_option_4),
            u32::from(e.vote_option_5),
        ];
        session.potential_votes = Some(u32::from(e.potential_votes));
    }

    pub fn handle_vote_passed(&mut self, e: &VotePassedEvent) {
        trace!("Vote passed {e:?}");
        let tick = self.tick;
        let session = self.vote_session_mut(e.voteidx);
        session.passed = Some(true);
        session.result_details = Some(e.details.to_string());
        session.result_param1 = Some(e.param_1.to_string());
        session.tick_end = Some(tick);
    }

    pub fn handle_vote_failed(&mut self, e: &VoteFailedEvent) {
        trace!("Vote failed {e:?}");
        let tick = self.tick;
        let session = self.vote_session_mut(e.voteidx);
        session.passed = Some(false);
        session.tick_end = Some(tick);
    }

    pub fn handle_vote_ended(&mut self) {
        trace!("Vote ended");
        let tick = self.tick;
        for session in self.vote_sessions.values_mut() {
            if session.tick_end.is_none() {
                session.tick_end = Some(tick);
            }
        }
    }

    fn finish_sm_current(&mut self) {
        if let Some(vote) = self.sm_current.take() {
            self.sm_votes.push(vote);
        }
        self.sm_map_announced_tick = None;
    }

    /// Classify a SourceMod progress update: Yes/No options mean a
    /// scramble vote, anything else means a map vote.
    fn sm_kind_for_options(options: &[SmVoteOption]) -> &str {
        if options.is_empty() {
            return "unknown";
        }
        let is_yes_no = options.len() <= 2
            && options
                .iter()
                .all(|o| o.name.eq_ignore_ascii_case("yes") || o.name.eq_ignore_ascii_case("no"));
        if is_yes_no { "scramble" } else { "map" }
    }

    fn handle_sm_progress(&mut self, total: u32, potential: u32, options: Vec<SmVoteOption>) {
        let tick = self.tick;
        let announced_is_map = self.sm_map_announced_tick.is_some();

        // A `0/N` update starts a new vote only if the current vote already
        // accumulated votes/options (a reset). Repeated `0/N` countdown
        // messages (`15s left`, `14s left`, ...) belong to the same vote.
        let starts_new = match &self.sm_current {
            None => true,
            Some(cur) => {
                total == 0 && (cur.total_votes > 0 || !cur.options.is_empty())
            }
        };
        if starts_new {
            self.finish_sm_current();
            let kind = if announced_is_map {
                "map".to_string()
            } else {
                Self::sm_kind_for_options(&options).to_string()
            };
            // Triggers are only relevant if they happened shortly before
            // the vote started; nominations persist for the whole map.
            // (~150s at 66 ticks/s; observed gaps are <40s.)
            let tick_u32 = u32::from(tick);
            let recent = |i: &SmVoteInitiator| {
                tick_u32.saturating_sub(u32::from(i.tick)) < 10_000
            };
            let (initiators, nominations) = if kind == "map" {
                (
                    std::mem::take(&mut self.sm_pending_rtv)
                        .into_iter()
                        .filter(recent)
                        .collect(),
                    std::mem::take(&mut self.sm_pending_nominations),
                )
            } else if kind == "scramble" {
                (
                    std::mem::take(&mut self.sm_pending_scramble)
                        .into_iter()
                        .filter(recent)
                        .collect(),
                    Vec::new(),
                )
            } else {
                // Kind still unknown (bare `0/N` countdown): attach nothing
                // yet; pending triggers are claimed once options classify
                // the vote in the update path below.
                (Vec::new(), Vec::new())
            };
            self.sm_current = Some(SourceModVote {
                kind,
                tick_start: tick,
                tick_end: None,
                initiators,
                nominations,
                total_votes: total,
                potential_votes: potential,
                options,
                result: None,
                passed: None,
            });
            self.sm_map_announced_tick = None;
            return;
        }

        if let Some(cur) = self.sm_current.as_mut() {
            cur.total_votes = total;
            cur.potential_votes = potential;
            cur.options = options;
            if cur.kind == "unknown" {
                cur.kind = Self::sm_kind_for_options(&cur.options).to_string();
                let tick_u32 = u32::from(cur.tick_start);
                if cur.kind == "map" {
                    cur.initiators.extend(
                        std::mem::take(&mut self.sm_pending_rtv)
                            .into_iter()
                            .filter(|i| tick_u32.saturating_sub(u32::from(i.tick)) < 10_000),
                    );
                    cur.nominations
                        .append(&mut self.sm_pending_nominations);
                } else if cur.kind == "scramble" {
                    cur.initiators.extend(
                        std::mem::take(&mut self.sm_pending_scramble)
                            .into_iter()
                            .filter(|i| tick_u32.saturating_sub(u32::from(i.tick)) < 10_000),
                    );
                }
            }
        }
    }

    fn handle_sm_text(&mut self, text: &str) {
        let tick = self.tick;
        if let Some((name, current, required)) = parse_scramble_trigger(text) {
            let steamid = self.steamid_for_name(&name);
            self.sm_pending_scramble.push(SmVoteInitiator {
                name,
                steamid,
                tick,
                current,
                required,
            });
            return;
        }
        if let Some((name, current, required)) = parse_rtv_trigger(text) {
            let steamid = self.steamid_for_name(&name);
            self.sm_pending_rtv.push(SmVoteInitiator {
                name,
                steamid,
                tick,
                current,
                required,
            });
            return;
        }
        if let Some((name, map)) = parse_nomination(text) {
            let steamid = self.steamid_for_name(&name);
            self.sm_pending_nominations.push(SmNomination {
                name,
                steamid,
                map,
                tick,
            });
            return;
        }
        if text == "[SM] Voting for next map has started." {
            // The announcement usually shares a tick with the first
            // `Votes: 0/N` progress message; message order within the tick
            // is not guaranteed, so reclassify an in-progress vote started
            // on the same tick.
            if let Some(cur) = self.sm_current.as_mut()
                && cur.kind == "unknown"
                && cur.tick_start == tick
            {
                cur.kind = "map".to_string();
                let start = u32::from(cur.tick_start);
                cur.initiators.extend(
                    std::mem::take(&mut self.sm_pending_rtv)
                        .into_iter()
                        .filter(|i| start.saturating_sub(u32::from(i.tick)) < 10_000),
                );
                cur.nominations
                    .append(&mut self.sm_pending_nominations);
                return;
            }
            self.sm_map_announced_tick = Some(tick);
            return;
        }
        if text == "Scrambling the teams due to vote." {
            if let Some(cur) = self.sm_current.as_mut() {
                cur.tick_end = Some(tick);
                cur.result = Some(text.to_string());
                cur.passed = Some(true);
                if cur.kind == "unknown" {
                    cur.kind = "scramble".to_string();
                }
            } else if let Some(last) = self.sm_votes.last_mut()
                && last.tick_end.is_none()
            {
                last.tick_end = Some(tick);
                last.result = Some(text.to_string());
                last.passed = Some(true);
            }
            self.finish_sm_current();
            return;
        }
        if let Some((map, _pct, _votes)) = parse_map_finished(text) {
            if let Some(cur) = self.sm_current.as_mut() {
                cur.tick_end = Some(tick);
                cur.result = Some(map);
                cur.passed = Some(true);
                if cur.kind == "unknown" {
                    cur.kind = "map".to_string();
                }
            } else if let Some(last) = self.sm_votes.last_mut()
                && last.tick_end.is_none()
            {
                last.tick_end = Some(tick);
                last.result = Some(map);
                last.passed = Some(true);
            }
            self.finish_sm_current();
            return;
        }
        if let Some((total, potential, _secs, options)) = parse_vote_progress(text) {
            self.handle_sm_progress(total, potential, options);
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
            error!(
                "Unknown attacker steamid mapping for user_id {attacker_user_id} in player hurt event"
            );
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
                    trace!(
                        "Hit by explosion! {:?} damage_type:{damage_type:?} effect:{effect:?}  weapon_type:{weapon_type:?}     {hit_exps:?}",
                        format!(
                            "{:?}-{:?}-{:?}",
                            e.projectile.class_name,
                            e.projectile.grenade_type,
                            e.projectile
                                .model_id
                                .as_ref()
                                .and_then(|id| self.models.get(id))
                        )
                    );

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
                wep.class_name, wi.item_type_name, self.explosions
            );
        } else {
            error!(
                "hurt with {} but unknown player weapon handle: {attacker_wep} {hurt:?}",
                hurt.weapon_id
            );
        }
        let Some(attacker) = self.player_summaries.get_mut(&attacker_steamid) else {
            error!(
                "Unknown attacker summary (mut) for steamid {attacker_steamid} in player hurt event"
            );
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
                        error!(
                            "Invalid owner steamid {steamid} for medigun {handle} when it was charged"
                        );
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
                        .and_then(|uid| self.user_id_to_steam_id.get(uid).cloned())
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
            UserMessage::Text(msg) => {
                self.handle_sm_text(msg.text.as_ref());
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
                                    origin.x = x;
                                }
                                (EFFECT_ORIGIN_Y, &SendPropValue::Float(y)) => {
                                    origin.y = y;
                                }
                                (EFFECT_ORIGIN_Z, &SendPropValue::Float(z)) => {
                                    origin.z = z;
                                }
                                (EFFECT_START_X, &SendPropValue::Float(x)) => {
                                    start.x = x;
                                }
                                (EFFECT_START_Y, &SendPropValue::Float(y)) => {
                                    start.y = y;
                                }
                                (EFFECT_START_Z, &SendPropValue::Float(z)) => {
                                    start.z = z;
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
                            trace!(
                                "Effect does not have both name:{name:?} and an entity:{entity:?} from {e:?}"
                            );
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
                                && explosion.projectile.kind == ProjectileType::HealingBolt
                                && !explosion.projectile.is_reflected
                            {
                                let o = explosion.projectile.owner;
                                let Some(attacker) = self.get_player_summary_mut_handle(&o) else {
                                    error!("Could not find player that fired healing bolt");
                                    continue;
                                };
                                attacker.handle_shot_hit("crusaders_crossbow");
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

                GameEvent::VoteStarted(e) => self.handle_vote_started(e),
                GameEvent::VoteCast(e) => self.handle_vote_cast(e),
                GameEvent::VoteOptions(e) => self.handle_vote_options(e),
                GameEvent::VoteChanged(e) => self.handle_vote_changed(e),
                GameEvent::VotePassed(e) => self.handle_vote_passed(e),
                GameEvent::VoteFailed(e) => self.handle_vote_failed(e),
                GameEvent::VoteEnded(_) => self.handle_vote_ended(),

                GameEvent::PlayerHealed(e) => self.handle_player_healed(e),
                GameEvent::CrossbowHeal(e) => self.handle_crossbow_heal(e),
                GameEvent::PlayerHealOnHit(e) => self.handle_player_heal_on_hit(e),
                GameEvent::PlayerExtinguished(e) => self.handle_player_extinguished(e),
                GameEvent::BuildingHealed(e) => self.handle_building_healed(e),
                GameEvent::MedicDeath(e) => self.handle_medic_death(e),
                GameEvent::ObjectDeflected(e) => self.handle_object_deflected(e),
                GameEvent::KilledCappingPlayer(e) => self.handle_killed_capping_player(e),
                GameEvent::CapperKilled(e) => self.handle_capper_killed(e),
                GameEvent::ProjectileDirectHit(e) => self.handle_projectile_direct_hit(e),
                GameEvent::PlayerTeleported(e) => self.handle_player_teleported(e),
                GameEvent::TeamPlayPointStartCapture(e) => self.handle_point_start_capture(e),
                GameEvent::PayloadPushed(e) => self.handle_payload_pushed(e),
                GameEvent::EnvironmentalDeath(e) => self.handle_environmental_death(e),
                GameEvent::PlayerBuiltObject(e) => self.handle_player_built_object(e),
                GameEvent::PlayerUpgradedObject(e) => self.handle_player_upgraded_object(e),
                GameEvent::PlayerCarryObject(e) => self.handle_player_carry_object(e),
                GameEvent::PlayerDropObject(e) => self.handle_player_drop_object(e),
                GameEvent::ObjectRemoved(e) => self.handle_object_removed(e),
                GameEvent::ObjectDetonated(e) => self.handle_object_detonated(e),

                GameEvent::TeamPlayWinPanel(e) => {
                    for entity_id_val in [e.player_1, e.player_2, e.player_3] {
                        let eid = EntityId::from(entity_id_val as u32);
                        let steamid = self
                            .user_entities
                            .get(&eid)
                            .and_then(|uid| self.user_id_to_steam_id.get(uid));
                        if let Some(steamid) = steamid
                            && let Some(p) = self.player_summaries.get(steamid)
                        {
                            self.current_round.mvps.push(p.steamid.clone());
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
                                    error!(
                                        "Could not find player entity {} for object destroyed event",
                                        player_summary.entity_id
                                    );
                                }
                            } else {
                                error!(
                                    "Could not find player summary for steamid of attacker_uid {attacker_uid} for object destroyed event"
                                );
                            }
                        } else {
                            error!(
                                "Could not find steamid for attacker_uid {attacker_uid} for object destroyed event"
                            );
                        }
                    }

                    let steamid = self.user_id_to_steam_id.get(&attacker_uid).cloned();
                    if let Some(steamid) = steamid {
                        if let Some(attacker) = self.player_summaries.get_mut(&steamid) {
                            attacker.handle_object_destroyed(weapon);
                        } else {
                            error!(
                                "Could not find attacker summary for steamid {steamid} that destroyed building {e:?}"
                            );
                        }
                    } else {
                        error!(
                            "Could not find steamid for attacker_uid {attacker_uid} that destroyed building {e:?}"
                        );
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
            if let Some(baseclass) = table.props.iter().find(|p| p.name == "baseclass")
                && let Some(basename) = &baseclass.table_name
            {
                edges.entry(basename).or_default().push(name);
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

        if let Some(vote) = self.sm_current.take() {
            self.sm_votes.push(vote);
        }
        // Backfill initiator/voter identities: userinfo string-table updates
        // can arrive after the vote events that reference them, so anything
        // unresolved at event time gets one more chance against the final
        // entity -> user -> steamid mappings.
        {
            let user_entities = &self.user_entities;
            let uid_to_sid = &self.user_id_to_steam_id;
            let summaries = &self.player_summaries;
            for session in self.vote_sessions.values_mut() {
                if session.initiator.is_none()
                    && let Some(entity) = session.initiator_entity
                {
                    let (steamid, name) =
                        resolve_vote_entity(entity, user_entities, uid_to_sid, summaries);
                    session.initiator = steamid;
                    session.initiator_name = name;
                }
                for ballot in &mut session.ballots {
                    if ballot.voter.is_none() {
                        let (steamid, name) = resolve_vote_entity(
                            ballot.voter_entity,
                            user_entities,
                            uid_to_sid,
                            summaries,
                        );
                        ballot.voter = steamid;
                        ballot.voter_name = name;
                    }
                    if ballot.option_name.is_none() {
                        ballot.option_name =
                            session.options.get(ballot.option as usize).cloned();
                    }
                }
            }
        }
        let mut votes: Vec<VoteSummary> = self
            .finished_votes
            .into_iter()
            .chain(self.vote_sessions.into_values())
            .collect();
        votes.sort_by_key(|v| (u32::from(v.tick_start), v.voteidx));

        DemoSummary {
            rounds: self.rounds,
            chat: self.chat,
            votes,
            sourcemod_votes: self.sm_votes,
            point_captures: self.point_captures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_demo_parser::{
        ParserState,
        demo::{
            data::userinfo::PlayerInfo,
            message::{Message, packetentities::BaselineIndex},
            packet::{datatable::ServerClass, stringtable::StringTableEntry},
            parser::gamestateanalyser::UserId,
        },
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

    #[test]
    fn test_parse_sm_triggers() {
        let (name, cur, req) =
            parse_scramble_trigger("FreaK wants to scramble teams. [1/4 votes required]")
                .unwrap();
        assert_eq!((name.as_str(), cur, req), ("FreaK", 1, 4));

        let (name, cur, req) =
            parse_rtv_trigger("[SM] moriya wants to rock the vote. (1 votes, 11 required)")
                .unwrap();
        assert_eq!((name.as_str(), cur, req), ("moriya", 1, 11));

        let (name, map) =
            parse_nomination("[SM] SchwanzusLongus has nominated cp_process_final.").unwrap();
        assert_eq!(name, "SchwanzusLongus");
        assert_eq!(map, "cp_process_final");

        assert!(parse_scramble_trigger("random chat").is_none());
        assert!(parse_rtv_trigger("random chat").is_none());
        assert!(parse_nomination("random chat").is_none());
    }

    #[test]
    fn test_parse_sm_progress_and_result() {
        let (total, potential, secs, options) =
            parse_vote_progress("Votes: 12/21, 9s left\n1. cp_process_final: (9)\n2. pl_phoenix: (2)\n3. cp_snakewater_final1: (1)").unwrap();
        assert_eq!((total, potential, secs), (12, 21, 9));
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].name, "cp_process_final");
        assert_eq!(options[0].votes, 9);

        let (total, potential, secs, options) =
            parse_vote_progress("Votes: 0/18, 20s left").unwrap();
        assert_eq!((total, potential, secs), (0, 18, 20));
        assert!(options.is_empty());

        let (map, pct, votes) = parse_map_finished(
            "[SM] Map voting has finished. The next map will be koth_harvest_final. (Received 61% of 13 votes)",
        )
        .unwrap();
        assert_eq!(map, "koth_harvest_final");
        assert_eq!((pct, votes), (61, 13));
    }

    #[test]
    fn test_native_vote_session() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);

        analyzer.tick = DemoTick::from(100);
        analyzer.handle_vote_started(&VoteStartedEvent {
            issue: "Kick".into(),
            param_1: "76561198000000000".into(),
            team: 0,
            initiator: 1,
            voteidx: 0,
        });
        analyzer.handle_vote_options(&VoteOptionsEvent {
            count: 2,
            option_1: "Yes".into(),
            option_2: "No".into(),
            option_3: "".into(),
            option_4: "".into(),
            option_5: "".into(),
            voteidx: 0,
        });
        analyzer.tick = DemoTick::from(110);
        analyzer.handle_vote_cast(&VoteCastEvent {
            vote_option: 0,
            team: 0,
            entity_id: 1,
            voteidx: 0,
        });
        analyzer.handle_vote_changed(&VoteChangedEvent {
            vote_option_1: 1,
            vote_option_2: 0,
            vote_option_3: 0,
            vote_option_4: 0,
            vote_option_5: 0,
            potential_votes: 12,
            voteidx: 0,
        });
        analyzer.tick = DemoTick::from(200);
        analyzer.handle_vote_passed(&VotePassedEvent {
            details: "#TF_vote_passed_kick".into(),
            param_1: "76561198000000000".into(),
            team: 0,
            voteidx: 0,
        });

        let session = analyzer.vote_sessions.get(&0).unwrap();
        assert_eq!(session.issue, "Kick");
        assert_eq!(u32::from(session.tick_start), 100);
        assert_eq!(session.tick_end, Some(DemoTick::from(200)));
        assert_eq!(session.options, vec!["Yes".to_string(), "No".to_string()]);
        assert_eq!(session.ballots.len(), 1);
        assert_eq!(session.ballots[0].option, 0);
        assert_eq!(session.ballots[0].option_name.as_deref(), Some("Yes"));
        assert_eq!(session.counts, vec![1, 0, 0, 0, 0]);
        assert_eq!(session.potential_votes, Some(12));
        assert_eq!(session.passed, Some(true));

        // Server-initiated votes have no player identity.
        analyzer.tick = DemoTick::from(300);
        analyzer.handle_vote_started(&VoteStartedEvent {
            issue: "ChangeMap".into(),
            param_1: "cp_badlands".into(),
            team: 0,
            initiator: 99,
            voteidx: 1,
        });
        let session = analyzer.vote_sessions.get(&1).unwrap();
        assert_eq!(session.initiator, None);
        assert_eq!(session.initiator_entity, None);
    }

    #[test]
    fn test_native_voteidx_reuse_across_maps() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);

        // First vote with voteidx 0, closed by a pass.
        analyzer.tick = DemoTick::from(100);
        analyzer.handle_vote_started(&VoteStartedEvent {
            issue: "Kick".into(),
            param_1: "A".into(),
            team: 0,
            initiator: 99,
            voteidx: 0,
        });
        analyzer.tick = DemoTick::from(200);
        analyzer.handle_vote_passed(&VotePassedEvent {
            details: "x".into(),
            param_1: "A".into(),
            team: 0,
            voteidx: 0,
        });

        // Counter restarts on the next map: same voteidx must not merge.
        analyzer.tick = DemoTick::from(50_000);
        analyzer.handle_vote_started(&VoteStartedEvent {
            issue: "Scramble".into(),
            param_1: "".into(),
            team: 0,
            initiator: 99,
            voteidx: 0,
        });

        let parser_state = ParserState::new(0, |_| true, false);
        let summary = analyzer.into_output(&parser_state);
        assert_eq!(summary.votes.len(), 2);
        assert_eq!(summary.votes[0].issue, "Kick");
        assert_eq!(summary.votes[0].passed, Some(true));
        assert_eq!(summary.votes[1].issue, "Scramble");
        assert_eq!(u32::from(summary.votes[1].tick_start), 50_000);
    }

    #[test]
    fn test_native_vote_failed_and_ended() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);

        // Failed vote: ballots for both options, then vote_failed.
        analyzer.tick = DemoTick::from(1000);
        analyzer.handle_vote_started(&VoteStartedEvent {
            issue: "Kick".into(),
            param_1: "B".into(),
            team: 0,
            initiator: 99,
            voteidx: 7,
        });
        analyzer.handle_vote_options(&VoteOptionsEvent {
            count: 2,
            option_1: "Yes".into(),
            option_2: "No".into(),
            option_3: "".into(),
            option_4: "".into(),
            option_5: "".into(),
            voteidx: 7,
        });
        analyzer.tick = DemoTick::from(1010);
        for (entity, option) in [(3u32, 0u8), (5u32, 1u8), (9u32, 1u8)] {
            analyzer.handle_vote_cast(&VoteCastEvent {
                vote_option: option,
                team: 0,
                entity_id: entity,
                voteidx: 7,
            });
        }
        analyzer.handle_vote_changed(&VoteChangedEvent {
            vote_option_1: 1,
            vote_option_2: 2,
            vote_option_3: 0,
            vote_option_4: 0,
            vote_option_5: 0,
            potential_votes: 10,
            voteidx: 7,
        });
        analyzer.tick = DemoTick::from(1200);
        analyzer.handle_vote_failed(&VoteFailedEvent {
            team: 0,
            voteidx: 7,
        });
        analyzer.handle_vote_ended();

        let parser_state = ParserState::new(0, |_| true, false);
        let summary = analyzer.into_output(&parser_state);
        assert_eq!(summary.votes.len(), 1);
        let v = &summary.votes[0];
        assert_eq!(v.voteidx, 7);
        assert_eq!(u32::from(v.tick_start), 1000);
        assert_eq!(v.tick_end, Some(DemoTick::from(1200)));
        assert_eq!(v.passed, Some(false));
        assert_eq!(v.ballots.len(), 3);
        assert_eq!(v.ballots[1].voter_entity, 5);
        assert_eq!(v.ballots[1].option, 1);
        assert_eq!(v.ballots[1].option_name.as_deref(), Some("No"));
        assert_eq!(v.counts, vec![1, 2, 0, 0, 0]);
        assert_eq!(v.potential_votes, Some(10));

        // JSON round-trip: every field must serialize.
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"issue\":\"Kick\""));
        assert!(json.contains("\"passed\":false"));
        let back: DemoSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.votes.len(), 1);
        assert_eq!(back.votes[0].ballots.len(), 3);
    }

    #[test]
    fn test_parse_capper_entities() {
        use tf_demo_parser::demo::data::MaybeUtf8String;

        // Packed entity indices, as observed on the wire.
        let cappers: MaybeUtf8String = "\u{7}\u{f}\u{10}".into();
        assert_eq!(parse_capper_entities(&cappers), vec![7, 15, 16]);

        let single: MaybeUtf8String = "\u{2}".into();
        assert_eq!(parse_capper_entities(&single), vec![2]);

        let empty: MaybeUtf8String = "".into();
        assert!(parse_capper_entities(&empty).is_empty());

        // Name-like content (any byte >= 64) is rejected rather than
        // misresolved as entity slots.
        let names: MaybeUtf8String = "Alice, Bob".into();
        assert!(parse_capper_entities(&names).is_empty());
    }

    #[test]
    fn test_heal_event_handlers() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        // Medic userid 42 on entity 16, patient userid 37 on entity 15,
        // pyro userid 45 on entity 7 (mirrors ashville observations).
        for (idx, (name, steam, uid, eid)) in [
            ("Scourage", "STEAM_0:1:100", 42u16, 15u32),
            ("Crispy", "STEAM_0:1:101", 37, 14),
            ("Funguz", "STEAM_0:1:102", 45, 6),
        ]
        .iter()
        .enumerate()
        {
            let entry = create_mock_user_info(name, steam, *uid, *eid);
            analyzer.handle_string_entry("userinfo", idx, &entry, &parser_state);
        }
        analyzer.tick = DemoTick::from(1001);

        analyzer.handle_player_healed(&PlayerHealedEvent {
            patient: 37,
            healer: 42,
            amount: 127,
        });
        // Healer 0 (kits) credits nobody.
        analyzer.handle_player_healed(&PlayerHealedEvent {
            patient: 45,
            healer: 0,
            amount: 88,
        });
        analyzer.handle_crossbow_heal(&CrossbowHealEvent {
            healer: 42,
            target: 37,
            amount: 127,
        });
        analyzer.handle_player_heal_on_hit(&PlayerHealOnHitEvent {
            amount: 127,
            ent_index: 15,
            weapon_def_index: 207,
        });
        analyzer.handle_player_extinguished(&PlayerExtinguishedEvent {
            victim: 37,
            healer: 45,
            item_definition_index: 0,
        });

        let medic = analyzer.player_summaries.get("STEAM_0:1:100").unwrap();
        assert_eq!(medic.stats.heals, 1);
        assert_eq!(medic.stats.healed, 127);
        assert_eq!(medic.stats.crossbow_heals, 1);
        assert_eq!(medic.stats.crossbow_healing, 127);

        let patient = analyzer.player_summaries.get("STEAM_0:1:101").unwrap();
        assert_eq!(patient.stats.heal_on_hit, 127);
        assert_eq!(patient.stats.heals, 0);

        let pyro = analyzer.player_summaries.get("STEAM_0:1:102").unwrap();
        assert_eq!(pyro.stats.extinguishes, 1);
    }

    #[test]
    fn test_defense_direct_teleport_push_handlers() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        for (idx, (name, steam, uid, eid)) in [
            // Killer on entity slot 11 (seed is stored +1 by the mock
            // userinfo encoding): capping-kill ids are entity indices.
            ("Killer", "STEAM_0:1:200", 21u16, 10u32),
            ("Engie", "STEAM_0:1:201", 30, 8),
        ]
        .iter()
        .enumerate()
        {
            let entry = create_mock_user_info(name, steam, *uid, *eid);
            analyzer.handle_string_entry("userinfo", idx, &entry, &parser_state);
        }

        analyzer.handle_killed_capping_player(&KilledCappingPlayerEvent {
            cp: 0,
            killer: 11,
            victim: 13,
            assister: 16,
        });
        analyzer.handle_capper_killed(&CapperKilledEvent {
            blocker: 11,
            victim: 13,
        });
        analyzer.handle_projectile_direct_hit(&ProjectileDirectHitEvent {
            attacker: 11,
            victim: 13,
            weapon_def_index: 0,
        });
        analyzer.handle_player_teleported(&PlayerTeleportedEvent {
            user_id: 12,
            builder_id: 30,
            dist: 1500.0,
        });
        analyzer.handle_payload_pushed(&PayloadPushedEvent {
            pusher: 11,
            distance: 42,
        });

        let killer = analyzer.player_summaries.get("STEAM_0:1:200").unwrap();
        assert_eq!(killer.stats.defenses, 2);
        assert_eq!(killer.stats.direct_hits, 1);
        assert_eq!(killer.stats.push_distance, 42);

        let engie = analyzer.player_summaries.get("STEAM_0:1:201").unwrap();
        assert_eq!(engie.stats.teleports, 1);
    }

    #[test]
    fn test_environmental_death_handler() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        for (idx, (name, steam, uid, eid)) in [
            ("Victim", "STEAM_0:1:300", 13u16, 9u32),
            ("Killer", "STEAM_0:1:301", 11, 5),
        ]
        .iter()
        .enumerate()
        {
            let entry = create_mock_user_info(name, steam, *uid, *eid);
            analyzer.handle_string_entry("userinfo", idx, &entry, &parser_state);
        }

        // World kill: only the victim tag.
        analyzer.handle_environmental_death(&EnvironmentalDeathEvent {
            killer: 0,
            victim: 13,
        });
        // Player-attributed: both sides.
        analyzer.handle_environmental_death(&EnvironmentalDeathEvent {
            killer: 11,
            victim: 13,
        });

        let victim = analyzer.player_summaries.get("STEAM_0:1:300").unwrap();
        assert_eq!(victim.stats.environmental_deaths, 2);
        assert_eq!(victim.stats.environmental_kills, 0);
        let killer = analyzer.player_summaries.get("STEAM_0:1:301").unwrap();
        assert_eq!(killer.stats.environmental_kills, 1);
    }

    #[test]
    fn test_object_lifecycle_handlers() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        let entry = create_mock_user_info("Engie", "STEAM_0:1:400", 30, 8);
        analyzer.handle_string_entry("userinfo", 0, &entry, &parser_state);

        analyzer.handle_player_built_object(&PlayerBuiltObjectEvent {
            user_id: 30,
            object: 2,
            index: 475,
        });
        analyzer.handle_player_upgraded_object(&PlayerUpgradedObjectEvent {
            user_id: 30,
            object: 2,
            index: 475,
            is_builder: true,
        });
        analyzer.handle_player_carry_object(&PlayerCarryObjectEvent {
            user_id: 30,
            object: 2,
            index: 475,
        });
        analyzer.handle_player_drop_object(&PlayerDropObjectEvent {
            user_id: 30,
            object: 2,
            index: 475,
        });
        analyzer.handle_object_removed(&ObjectRemovedEvent {
            user_id: 30,
            object_type: 2,
            index: 475,
        });
        analyzer.handle_object_detonated(&ObjectDetonatedEvent {
            user_id: 30,
            object_type: 2,
            index: 475,
        });

        let engie = analyzer.player_summaries.get("STEAM_0:1:400").unwrap();
        assert_eq!(engie.stats.object_placed, 1);
        assert_eq!(engie.stats.object_upgraded, 1);
        assert_eq!(engie.stats.object_carried, 1);
        assert_eq!(engie.stats.object_dropped, 1);
        assert_eq!(engie.stats.object_removed, 1);
        assert_eq!(engie.stats.object_detonated, 1);
        // Entity-derived completion counter untouched by the event path.
        assert_eq!(engie.stats.object_built, 0);
    }

    #[test]
    fn test_medic_death_reflect_building_heal_handlers() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        for (idx, (name, steam, uid, eid)) in [
            ("Medic", "STEAM_0:1:500", 42u16, 15u32),
            ("Pyro", "STEAM_0:1:501", 45, 6),
            ("Engie", "STEAM_0:1:502", 30, 7),
        ]
        .iter()
        .enumerate()
        {
            let entry = create_mock_user_info(name, steam, *uid, *eid);
            analyzer.handle_string_entry("userinfo", idx, &entry, &parser_state);
        }

        analyzer.handle_medic_death(&MedicDeathEvent {
            user_id: 42,
            attacker: 35,
            healing: 147,
            charged: true,
        });
        analyzer.handle_medic_death(&MedicDeathEvent {
            user_id: 42,
            attacker: 35,
            healing: 200,
            charged: false,
        });
        analyzer.handle_object_deflected(&ObjectDeflectedEvent {
            user_id: 45,
            owner_id: 35,
            weapon_id: 35,
            object_ent_index: 327,
        });
        analyzer.handle_building_healed(&BuildingHealedEvent {
            building: 475,
            healer: 8,
            amount: 81,
        });

        let medic = analyzer.player_summaries.get("STEAM_0:1:500").unwrap();
        assert_eq!(medic.stats.dropped_ubers, 1);
        let pyro = analyzer.player_summaries.get("STEAM_0:1:501").unwrap();
        assert_eq!(pyro.stats.reflects, 1);
        let engie = analyzer.player_summaries.get("STEAM_0:1:502").unwrap();
        assert_eq!(engie.stats.building_healing, 81);
    }

    #[test]
    fn test_point_start_capture_handler() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        let parser_state = ParserState::new(0, |_| true, false);
        for (idx, (name, steam, uid, eid)) in [
            ("Red1", "STEAM_0:1:600", 20u16, 7u32),
            ("Red2", "STEAM_0:1:601", 21, 15),
        ]
        .iter()
        .enumerate()
        {
            let entry = create_mock_user_info(name, steam, *uid, *eid);
            analyzer.handle_string_entry("userinfo", idx, &entry, &parser_state);
        }

        analyzer.tick = DemoTick::from(2384);
        analyzer.handle_point_start_capture(&TeamPlayPointStartCaptureEvent {
            cp: 0,
            cp_name: "#koth_viaduct_cap".into(),
            team: 0,
            cap_team: 2,
            // Packed entity indices 8 and 15 (seeds are stored +1 by the
            // mock userinfo encoding, mirroring real demos).
            cappers: "\u{8}\u{10}".into(),
            cap_time: 34.45,
        });

        assert_eq!(analyzer.point_captures.len(), 1);
        let cap = &analyzer.point_captures[0];
        assert_eq!(u32::from(cap.tick), 2384);
        assert_eq!(cap.cp_name, "#koth_viaduct_cap");
        assert_eq!(cap.cap_team, 2);
        assert_eq!(cap.cappers, vec!["STEAM_0:1:600".to_string(), "STEAM_0:1:601".to_string()]);
        assert!((cap.cap_time - 34.45).abs() < 0.01);

        let parser_state = ParserState::new(0, |_| true, false);
        let summary = analyzer.into_output(&parser_state);
        assert_eq!(summary.point_captures.len(), 1);
    }

    #[test]
    fn test_sm_scramble_vote_flow() {
        let schema = Schema::default();
        let mut analyzer = MatchAnalyzer::new(&schema);
        // Seed a player so the initiator name resolves.
        let parser_state = ParserState::new(0, |_| true, false);
        let entry = create_mock_user_info("FreaK", EXAMPLE_STEAMID, 2, 6);
        analyzer.handle_string_entry("userinfo", 0, &entry, &parser_state);

        analyzer.tick = DemoTick::from(9968);
        analyzer.handle_sm_text("FreaK wants to scramble teams. [1/1 votes required]");
        analyzer.tick = DemoTick::from(11203);
        analyzer.handle_sm_text("Votes: 0/18, 20s left");
        analyzer.handle_sm_text("Votes: 1/18, 19s left\n1. Yes: (1)");
        analyzer.tick = DemoTick::from(12553);
        analyzer.handle_sm_text("Scrambling the teams due to vote.");

        assert!(analyzer.sm_current.is_none());
        assert_eq!(analyzer.sm_votes.len(), 1);
        let v = &analyzer.sm_votes[0];
        assert_eq!(v.kind, "scramble");
        assert_eq!(u32::from(v.tick_start), 11203);
        assert_eq!(v.tick_end, Some(DemoTick::from(12553)));
        assert_eq!(v.initiators.len(), 1);
        assert_eq!(v.initiators[0].steamid.as_deref(), Some(EXAMPLE_STEAMID));
        assert_eq!(v.total_votes, 1);
        assert_eq!(v.passed, Some(true));
    }
}
