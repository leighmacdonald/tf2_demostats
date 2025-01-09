use crate::parser::{
    game::{
        DamageType, Death, PlayerCondition, RoundState, ENTITY_IN_WATER, ENTITY_ON_GROUND,
        INVALID_HANDLE,
    },
    weapon::Weapon,
};
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tf_demo_parser::{
    demo::{
        data::{DemoTick, UserInfo},
        gameevent_gen::{
            ObjectDestroyedEvent, PlayerDeathEvent, PlayerHurtEvent, TeamPlayCaptureBlockedEvent,
            TeamPlayPointCapturedEvent,
        },
        gamevent::GameEvent,
        message::{
            gameevent::GameEventMessage,
            packetentities::{EntityId, PacketEntity},
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
        sendprop::{SendPropIdentifier, SendPropValue},
        vector::{Vector, VectorXY},
    },
    MessageType, ParserState, ReadResult, Stream,
};
use tracing::{debug, error, error_span, info, span::EnteredSpan, trace, warn};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerMeta {
    pub name: String,
    entity_id: u32,
    pub user_id: u32,
    pub steam_id: String,
    //extra: u32, // all my sources say these 4 bytes don't exist
    //friends_id: u32,
    //friends_name_bytes: [u8; 32], // seem to all be 0 now
    pub is_fake_player: bool,
    pub is_hl_tv: bool,
    pub is_replay: bool,
    // pub custom_file: [u32; 4],
    // pub files_downloaded: u32,
    // pub more_extra: u8,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DemoSummary {
    pub player_summaries: HashMap<UserId, PlayerSummary>,
    pub deaths: Vec<PlayerDeath>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PlayerDeath {}

#[derive(Debug, Default)]
pub struct WeaponState {
    pub class: String,
    pub charge: f32,
    pub id: u32,
}

#[derive(Debug, Default)]
pub struct MatchAnalyzer {
    state: DemoSummary,
    user_entities: HashMap<EntityId, UserId>,
    user_handles: HashMap<u32, UserId>,
    weapon_handles: HashMap<u32, WeaponState>,
    entity_handles: HashMap<EntityId, u32>, // Entity -> Handle lookup
    users: BTreeMap<UserId, PlayerMeta>,
    waiting_for_players: bool,
    round_state: RoundState,
    span: Option<EnteredSpan>,
    tick: DemoTick,
    server_tick: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Killstreak {
    pub user_id: u32,
    pub class: Class,
    pub duration: u32,
}

// Med specific stats
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct HealersSummary {
    pub preround_healing: u32,
    pub healing: u32,
    pub postround_healing: u32,

    pub drops: u32,
    pub near_full_charge_death: u32,
    // TODO:
    // pub charges_uber: u32,
    // pub charges_kritz: u32,
    // pub charges_vacc: u32,
    // pub charges_quickfix: u32,
    // pub avg_uber_length: u32,
    // pub major_adv_lost: u32,
    // pub biggest_adv_lost: u32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Stats {
    pub kills: u32,
    pub assists: u32,
    pub deaths: u32,
    pub postround_kills: u32,
    pub postround_assists: u32,
    pub postround_deaths: u32,

    // Dupes with HealersSummary to cover non-med healing
    pub preround_healing: u32,
    pub healing: u32,
    pub postround_healing: u32,

    pub damage: u32, // Added up PlayerHurt events
    pub damage_taken: u32,

    pub dominations: u32, // This player dominated another player
    pub dominated: u32,   // Another player dominated this player
    pub revenges: u32,    // This player got revenge on another player
    pub revenged: u32,    // Another player got revenge on this player

    // Kills where the victim was in the air for a decent amount of time.
    // TOOD: clarify this definition
    pub airshots: u32,

    pub headshot_kills: u32,
    pub backstab_kills: u32,

    pub headshots: u32,
    pub backstabs: u32,

    pub was_headshot: u32,
    pub was_backstabbed: u32,
    // TODO
    // pub shots: u32,
    // pub hits: u32,
}

impl Stats {
    pub fn handle_damage_dealt(&mut self, hurt: &PlayerHurtEvent, damage_type: DamageType) {
        self.damage += hurt.damage_amount as u32;

        if damage_type == DamageType::Backstab {
            self.backstabs += 1;
        } else if damage_type == DamageType::Headshot {
            self.headshots += 1;
        }
    }

    pub fn handle_damage_taken(&mut self, hurt: &PlayerHurtEvent, damage_type: DamageType) {
        self.damage_taken += hurt.damage_amount as u32;

        if damage_type == DamageType::Backstab {
            self.was_backstabbed += 1;
        } else if damage_type == DamageType::Headshot {
            self.was_headshot += 1;
        }
    }

    pub fn handle_death(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        if flags.contains(Death::Domination) {
            self.dominated += 1;
        }
        if flags.contains(Death::AssisterDomination) {
            self.dominated += 1;
        }
        if flags.contains(Death::Revenge) {
            self.revenged += 1;
        }
        if flags.contains(Death::AssisterRevenge) {
            self.revenged += 1;
        }

        if flags.contains(Death::Feign) {
            return;
        }

        if round_state == RoundState::TeamWin {
            self.postround_deaths += 1;
        } else {
            self.deaths += 1;
        }
    }

    pub fn handle_assist(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        if flags.contains(Death::AssisterDomination) {
            self.dominations += 1;
        }
        if flags.contains(Death::AssisterRevenge) {
            self.revenges += 1;
        }

        if flags.contains(Death::Feign) {
            return;
        }

        if round_state == RoundState::TeamWin {
            self.postround_assists += 1;
        } else {
            self.assists += 1;
        }
    }

    pub fn handle_kill(
        &mut self,
        round_state: RoundState,
        flags: EnumSet<Death>,
        damage_type: DamageType,
        airshot: bool,
    ) {
        if flags.contains(Death::Domination) {
            self.dominations += 1;
        }
        if flags.contains(Death::Revenge) {
            self.revenges += 1;
        }

        if flags.contains(Death::Feign) {
            return;
        }

        if round_state == RoundState::TeamWin {
            self.postround_kills += 1;
            return;
        }

        self.kills += 1;

        if airshot {
            self.airshots += 1;
        }

        if damage_type == DamageType::Backstab {
            self.backstab_kills += 1;
        } else if damage_type == DamageType::Headshot {
            self.headshot_kills += 1;
        }
    }

    pub fn handle_healing(&mut self, round_state: RoundState, amount: u32) {
        if round_state == RoundState::TeamWin {
            return;
        }

        if round_state == RoundState::PreRound {
            self.preround_healing += amount;
        } else if round_state == RoundState::TeamWin {
            self.postround_healing += amount;
        } else {
            self.healing += amount;
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PlayerSummary {
    pub name: String,
    pub steamid: String,

    pub team: Team,
    pub time_start: u32, // ticks instead?
    pub time_end: u32,
    pub points: Option<u32>,
    pub connection_count: u32,
    pub bonus_points: Option<u32>,

    #[serde(flatten)]
    pub stats: Stats,

    pub classes: HashMap<Class, Stats>,
    pub weapons: HashMap<Weapon, Stats>,

    pub scoreboard_kills: Option<u32>,
    pub postround_kills: u32,

    pub scoreboard_assists: Option<u32>, // Only present in PoV demos
    pub postround_assists: u32,

    pub suicides: u32,

    pub scoreboard_deaths: Option<u32>,
    pub postround_deaths: u32,

    pub captures: u32,
    pub captures_blocked: u32,

    pub scoreboard_damage: Option<u32>,

    pub healing: HealersSummary,

    // TODO
    //pub healing_taken: u32,
    //pub health_packs: u32,
    //pub healing_packs: u32, // total healing from packs
    //pub extinguishes: u32,
    //pub building_built: u32,
    //pub buildings_destroyed: u32,
    //pub teleports: u32,
    //pub support: u32,
    //pub killstreaks: Vec<Killstreak>,

    // Flags for internal state tracking but unused elsewhere
    #[serde(skip)]
    on_ground: bool,
    #[serde(skip)]
    in_water: bool,
    #[serde(skip)]
    started_flying: DemoTick,
    #[serde(skip)]
    class: Class,
    #[serde(skip)]
    health: u32,

    // Temporary stat for tracking healing score changes, not the
    // actual stat
    #[serde(skip)]
    scoreboard_healing: u32,

    #[serde(skip)]
    sim_time: u32,
    #[serde(skip)]
    origin: Vector,
    #[serde(skip)]
    eye: VectorXY,
    #[serde(skip)]
    cond: EnumSet<PlayerCondition>,
    #[serde(skip)]
    cond_source: u32,
    #[serde(skip)]
    handle: u32,
    #[serde(skip)]
    active_weapon_handle: u32,
    #[serde(skip)]
    weapon_handles: Box<[u32; 7]>,
    #[serde(skip)]
    charge: f32, // ie med charge -- not wired to always be up to date!
}

impl PlayerSummary {
    fn in_air(&self) -> bool {
        !self.on_ground && !self.in_water
    }

    fn update_cond<const OFFSET: usize>(&mut self, bits: u32) {
        let mask: u128 = 0xffffffff << OFFSET;
        let new_cond = (self.cond.as_repr() & !mask) | ((bits as u128) << OFFSET);
        self.cond = EnumSet::<PlayerCondition>::from_repr(new_cond);
        trace!("Player {} condition now {:?}", self.name, self.cond);
    }

    fn class_stats(&mut self) -> &mut Stats {
        self.classes.entry(self.class).or_default()
    }

    fn handle_damage_dealt(&mut self, hurt: &PlayerHurtEvent, damage_type: DamageType) {
        self.stats.handle_damage_dealt(hurt, damage_type);
        self.class_stats().handle_damage_dealt(hurt, damage_type);
    }

    fn handle_damage_taken(&mut self, hurt: &PlayerHurtEvent, damage_type: DamageType) {
        self.stats.handle_damage_taken(hurt, damage_type);
        self.class_stats().handle_damage_taken(hurt, damage_type);
    }

    fn handle_assist(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        self.stats.handle_assist(round_state, flags);
        self.class_stats().handle_assist(round_state, flags);
    }

    fn handle_kill(
        &mut self,
        round_state: RoundState,
        flags: EnumSet<Death>,
        damage_type: DamageType,
        airshot: bool,
    ) {
        self.stats
            .handle_kill(round_state, flags, damage_type, airshot);
        self.class_stats()
            .handle_kill(round_state, flags, damage_type, airshot);
    }

    fn handle_death(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        if self.class == Class::Medic && round_state == RoundState::Running {
            if self.charge == 1.0 {
                self.healing.drops += 1;
            } else if self.charge > 0.95 {
                self.healing.near_full_charge_death += 1;
            }
        }

        self.stats.handle_death(round_state, flags);
        self.class_stats().handle_death(round_state, flags);
    }

    fn handle_healing(&mut self, round_state: RoundState, amount: u32) {
        if round_state == RoundState::PreRound {
            self.healing.preround_healing += amount;
        } else if round_state == RoundState::TeamWin {
            self.healing.postround_healing += amount;
        } else {
            self.healing.healing += amount;
        }

        self.stats.handle_healing(round_state, amount);
        self.class_stats().handle_healing(round_state, amount);
    }
}

impl MatchAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    fn parse_user_info(
        &mut self,
        index: usize,
        text: Option<&str>,
        data: Option<Stream>,
    ) -> ReadResult<()> {
        if let Some(user_info) = UserInfo::parse_from_string_table(index as u16, text, data)? {
            let entity_id = user_info.entity_id;
            let id = user_info.player_info.user_id;
            debug!("user info {} {id} {entity_id}", user_info.player_info.name);

            let new_user_info = user_info.clone();
            self.users
                .entry(id)
                .and_modify(|info| {
                    info.entity_id = new_user_info.entity_id.into();
                })
                .or_insert_with(|| PlayerMeta {
                    name: new_user_info.player_info.name,
                    entity_id: new_user_info.entity_id.into(),
                    user_id: new_user_info.player_info.user_id.into(),
                    steam_id: new_user_info.player_info.steam_id,
                    is_fake_player: new_user_info.player_info.is_fake_player > 0,
                    is_hl_tv: new_user_info.player_info.is_hl_tv > 0,
                    is_replay: new_user_info.player_info.is_replay > 0,
                });

            self.state
                .player_summaries
                .entry(id)
                .and_modify(|summary| summary.connection_count += 1)
                .or_insert_with(|| PlayerSummary {
                    name: user_info.player_info.name,
                    steamid: user_info.player_info.steam_id,
                    ..Default::default()
                });

            self.user_entities.insert(entity_id, id);
        }

        Ok(())
    }

    fn handle_packet_entity(&mut self, packet: &PacketEntity, parser_state: &ParserState) {
        let Some(class) = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
        else {
            error!("Unknown server class: {}", packet.server_class);
            return;
        };

        match class.name.as_str() {
            "CTFPlayer" => self.handle_player_entity(packet, parser_state),
            "CTFPlayerResource" => self.handle_player_resource(packet, parser_state),
            "CTFGameRulesProxy" => self.handle_game_rules(packet, parser_state),
            _ => {
                trace!(
                    "Unhandled PacketEntity: {:?} {:?}",
                    packet,
                    class.name.as_str()
                );
            }
        }

        const MEDIGUN_CHARGE_LEVEL: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFWeaponMedigunDataNonLocal", "m_flChargeLevel");
        const SELF_HANDLE: SendPropIdentifier =
            SendPropIdentifier::new("DT_AttributeContainer", "m_hOuter");
        const ITEM_DEFINITION: SendPropIdentifier =
            SendPropIdentifier::new("DT_ScriptCreatedItem", "m_iItemDefinitionIndex");

        let mut handle: Option<u32> = None;
        let mut charge: Option<f32> = None;
        let mut id: Option<u32> = None;
        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (MEDIGUN_CHARGE_LEVEL, &SendPropValue::Float(z)) => {
                    charge = Some(z);
                }
                (SELF_HANDLE, &SendPropValue::Integer(h)) => handle = Some(h as u32),
                (ITEM_DEFINITION, &SendPropValue::Integer(x)) => id = Some(x as u32),
                _ => {}
            }
        }

        let mut existing_handle = self.entity_handles.get(&packet.entity_index).copied();

        if let Some(handle) = handle {
            self.entity_handles.insert(packet.entity_index, handle);
            existing_handle = Some(handle);
        }

        if let Some(handle) = existing_handle {
            let wep = self.weapon_handles.entry(handle).or_default();
            if let Some(charge) = charge {
                wep.charge = charge;
            }
            if let Some(id) = id {
                wep.id = id;
            }
            wep.class = class.name.to_string();
        } else {
            if charge.is_some() {
                error!("Could not find weapon handle for medigun {packet:?}");
            }
        }
    }

    fn handle_player_entity(&mut self, entity: &PacketEntity, _parser_state: &ParserState) {
        // These DT_TFPlayerScoringDataExclusive props are only present in PoV demos, not STV demos.
        //
        // Other fields: m_iDominations, m_iRevenge,
        // m_iBuildingsDestroyed, m_iHeadshots, m_iBackstabs,
        // m_iHealPoints, m_iInvulns, m_iTeleports, m_iDamageDone,
        // m_iBonusPoints, m_iPoints, m_iCaptures, m_iDefenses
        const KILLS: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iKills");
        const DEATHS: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iDeaths");
        const KILL_ASSISTS: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerScoringDataExclusive", "m_iKillAssists");

        // Props that always are available
        const FLAGS: SendPropIdentifier = SendPropIdentifier::new("DT_BasePlayer", "m_fFlags");
        const HEALTH: SendPropIdentifier = SendPropIdentifier::new("DT_BasePlayer", "m_iHealth");
        const CLASS: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerClassShared", "m_iClass");
        const SIM_TIME: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseEntity", "m_flSimulationTime");

        const ORIGIN_XY: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin");
        const ORIGIN_Z: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin[2]");

        const EYE_X: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[0]");
        const EYE_Y: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[1]");

        const HANDLE: SendPropIdentifier =
            SendPropIdentifier::new("DT_AttributeManager", "m_hOuter");

        const COND_0: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCond");
        const COND_1: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx");
        const COND_2: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx2");
        const COND_3: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx3");

        const ACTIVE_WEAPON_HANDLE: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseCombatCharacter", "m_hActiveWeapon");

        const COND_SOURCE: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFPlayerConditionSource", "m_pProvider");

        const WEP_0: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "000");
        const WEP_1: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "001");
        const WEP_2: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "002");
        const WEP_3: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "003");
        const WEP_4: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "004");
        const WEP_5: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "005");
        const WEP_6: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "006");
        const WEP_7: SendPropIdentifier = SendPropIdentifier::new("m_hMyWeapons", "007");

        let entity_id = &entity.entity_index;
        let Some(user_id) = self.user_entities.get(entity_id) else {
            error!("Unknown player entity id: {entity_id}");
            return;
        };

        let Some(summary) = self.state.player_summaries.get_mut(user_id) else {
            error!("Unknown player user id: {}", user_id);
            return;
        };

        for prop in &entity.props {
            match (prop.identifier, &prop.value) {
                (KILLS, SendPropValue::Integer(val)) => {
                    summary.scoreboard_kills = Some(*val as u32);
                }
                (KILL_ASSISTS, SendPropValue::Integer(val)) => {
                    // PoV demos include multiple different copies of
                    // this field -- maybe per round stats? We want
                    // the larger one.
                    summary.scoreboard_assists =
                        Some(summary.scoreboard_assists.unwrap_or(0).max(*val as u32));
                }
                (DEATHS, SendPropValue::Integer(val)) => {
                    summary.scoreboard_deaths = Some(*val as u32);
                }
                (FLAGS, SendPropValue::Integer(val)) => {
                    let was_in_air = summary.in_air();

                    let flags = *val as u16;
                    summary.on_ground = flags & ENTITY_ON_GROUND != 0;
                    summary.in_water = flags & ENTITY_IN_WATER != 0;

                    let now_in_air = summary.in_air();

                    if !was_in_air && now_in_air {
                        summary.started_flying = self.tick;
                    }
                }
                (CLASS, SendPropValue::Integer(val)) => {
                    let Ok(class) = Class::try_from(*val as u8) else {
                        error!("Unknown classid {val}");
                        continue;
                    };
                    summary.class = class;
                }
                (HEALTH, SendPropValue::Integer(val)) => {
                    summary.health = *val as u32;
                }
                (SIM_TIME, SendPropValue::Integer(val)) => {
                    summary.sim_time = *val as u32;
                }

                (ORIGIN_XY, SendPropValue::VectorXY(vec)) => {
                    summary.origin.x = vec.x;
                    summary.origin.y = vec.y;
                }
                (ORIGIN_Z, &SendPropValue::Float(z)) => summary.origin.z = z,

                (EYE_X, &SendPropValue::Float(x)) => summary.eye.x = x,
                (EYE_Y, &SendPropValue::Float(y)) => summary.eye.y = y,

                (HANDLE, &SendPropValue::Integer(h)) => {
                    trace!("player ({}) has handle {h}", summary.name);
                    summary.handle = h as u32;
                    self.user_handles.insert(h as u32, *user_id);
                }
                (COND_SOURCE, &SendPropValue::Integer(x)) => summary.cond_source = x as u32,

                (COND_0, &SendPropValue::Integer(x)) => summary.update_cond::<0>(x as u32),
                (COND_1, &SendPropValue::Integer(x)) => summary.update_cond::<32>(x as u32),
                (COND_2, &SendPropValue::Integer(x)) => summary.update_cond::<64>(x as u32),
                (COND_3, &SendPropValue::Integer(x)) => summary.update_cond::<96>(x as u32),

                (ACTIVE_WEAPON_HANDLE, &SendPropValue::Integer(x)) => {
                    if x as u32 == INVALID_HANDLE {
                        continue;
                    }
                    summary.active_weapon_handle = x as u32;
                }

                (WEP_0, &SendPropValue::Integer(x)) => summary.weapon_handles[0] = x as u32,
                (WEP_1, &SendPropValue::Integer(x)) => summary.weapon_handles[1] = x as u32,
                (WEP_2, &SendPropValue::Integer(x)) => summary.weapon_handles[2] = x as u32,
                (WEP_3, &SendPropValue::Integer(x)) => summary.weapon_handles[3] = x as u32,
                (WEP_4, &SendPropValue::Integer(x)) => summary.weapon_handles[4] = x as u32,
                (WEP_5, &SendPropValue::Integer(x)) => summary.weapon_handles[5] = x as u32,
                (WEP_6, &SendPropValue::Integer(x)) => summary.weapon_handles[6] = x as u32,
                (WEP_7, &SendPropValue::Integer(_)) => error!("Unexpected 8th weapons"),

                _ => {
                    trace!("Unhandled player ({}) entity prop {prop:?}", summary.name);
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
                let entity_id = EntityId::from(player_id);
                if let Some(user_id) = self.user_entities.get(&entity_id) {
                    if let Some(player) = self.state.player_summaries.get_mut(user_id) {
                        match table_name.as_str() {
                            "m_iTeam" => {}
                            "m_iHealing" => {
                                let hi = i64::try_from(&prop.value).unwrap_or_default();
                                if hi < 0 {
                                    error!("Negative healing of {hi} by {}", player.name);
                                    return;
                                }
                                let h = hi as u32;

                                // Skip the first real value; sometimes STV starts a little late and we can't distinguish the healing values.
                                if player.scoreboard_healing == 0 {
                                    player.scoreboard_healing = h;
                                    return;
                                }

                                // Add up deltas, as this tracker resets to 0 mid round.
                                let dh = h.saturating_sub(player.scoreboard_healing);
                                if dh > 300 {
                                    // Never saw a delta this large in our corpus; may be a sign of a miscount
                                    warn!("Huge healing delta of {dh} by {}", player.name);
                                }

                                player.handle_healing(self.round_state, dh);

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
        const WAITING_FOR_PLAYERS: SendPropIdentifier =
            SendPropIdentifier::new("DT_TeamplayRoundBasedRules", "m_bInWaitingForPlayers");
        const ROUND_STATE: SendPropIdentifier =
            SendPropIdentifier::new("DT_TeamplayRoundBasedRules", "m_iRoundState");

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

        let feigned = flags.contains(Death::Feign);

        if death.user_id == death.attacker {
            let Some(suicider) = self
                .state
                .player_summaries
                .get_mut(&UserId::from(death.attacker as u32))
            else {
                error!("Unknown suicider id: {}", death.user_id);
                return;
            };
            if self.round_state != RoundState::TeamWin {
                suicider.suicides += 1;
            }
            return;
        }

        let Some(victim) = self
            .state
            .player_summaries
            .get_mut(&UserId::from(death.user_id as u32))
        else {
            error!("Unknown victim id: {}", death.user_id);
            return;
        };

        if victim.class == Class::Medic {
            let medigun_h = victim.weapon_handles[1];
            if let Some(medigun) = self.weapon_handles.get(&medigun_h) {
                victim.charge = medigun.charge;
            } else {
                error!("Med died without a secondary {medigun_h}");
            }
        }

        victim.handle_death(self.round_state, flags);

        // TODO: Tune this definition. Suppstats uses "distance from
        // ground" but that doesn't seem much better.
        let airshot = victim.in_air() && (self.tick - victim.started_flying > 16);

        let attacker_is_world = death.attacker == 0;
        let attacker = self
            .state
            .player_summaries
            .get_mut(&UserId::from(death.attacker as u32));
        if let Some(attacker) = attacker {
            if !feigned {
                if self.round_state == RoundState::TeamWin {
                    attacker.postround_kills += 1;
                } else {
                    if airshot {
                        debug!("airshot by {}!", attacker.name);
                    }
                    let h = attacker.active_weapon_handle;
                    if let Some(wep) = self.weapon_handles.get(&h) {
                        info!(
                            "death with {} / {} vs entity: {} / {}",
                            death.weapon, death.weapon_log_class_name, wep.class, wep.id
                        );
                    } else {
                        info!(
                            "death with {} / {} but unknown player weapon handle: {h}",
                            death.weapon, death.weapon_log_class_name
                        );
                    }
                    attacker.handle_kill(self.round_state, flags, damage_type, airshot);
                }
            }
        } else if !attacker_is_world {
            error!("Unknown attacker id: {}", death.attacker);
            return;
        }

        if death.assister == 0xffff {
            return;
        }

        let assister = self
            .state
            .player_summaries
            .get_mut(&UserId::from(death.assister as u32));
        if let Some(assister) = assister {
            assister.handle_assist(self.round_state, flags);
        } else {
            error!("Unknown assister id: {}", death.assister);
        }
    }

    pub fn handle_point_captured(&mut self, cap: &TeamPlayPointCapturedEvent) {
        trace!("Point Captures {:?}", cap);

        for entity_id in cap.cappers.as_bytes() {
            let Some(uid) = self.user_entities.get(&EntityId::from(*entity_id as u32)) else {
                error!("Unknown entity id {entity_id} in capture event");
                continue;
            };

            let Some(summary) = self.state.player_summaries.get_mut(uid) else {
                error!("Unknown uid {uid} from entity id {entity_id} in capture event");
                continue;
            };
            debug!("Capture by {}", summary.name);
            summary.captures += 1;
        }
    }

    pub fn handle_capture_blocked(&mut self, cap: &TeamPlayCaptureBlockedEvent) {
        trace!("Capture blocked {:?}", cap);

        let entity_id = EntityId::from(cap.blocker as u32);
        let Some(uid) = self.user_entities.get(&entity_id) else {
            error!("Unknown entity id {entity_id} in capture blocked event");
            return;
        };

        let Some(summary) = self.state.player_summaries.get_mut(uid) else {
            error!("Unknown uid {uid} from entity id {entity_id} in capture blocked event");
            return;
        };

        summary.captures_blocked += 1;
    }

    pub fn handle_player_hurt(&mut self, hurt: &PlayerHurtEvent) {
        trace!("Player hurt {:?}", hurt);

        let damage_type = DamageType::try_from(hurt.custom).unwrap_or_else(|e| {
            error!("Unknown hurt damage type: {}, error: {e}", hurt.custom);
            DamageType::Normal
        });

        let uid = UserId::from(hurt.user_id);
        let Some(victim) = self.state.player_summaries.get_mut(&uid) else {
            error!("Unknown victim uid {uid} in player hurt event");
            return;
        };

        victim.handle_damage_taken(hurt, damage_type);

        let uid = UserId::from(hurt.user_id);
        let Some(attacker) = self.state.player_summaries.get_mut(&uid) else {
            error!("Unknown attacker uid {uid} in player hurt event");
            return;
        };

        let h = attacker.active_weapon_handle;
        if let Some(wep) = self.weapon_handles.get(&h) {
            info!(
                "hurt with {} vs entity: {} / {}",
                hurt.weapon_id, wep.class, wep.id
            );
        } else {
            info!(
                "hurt with {} but unknown player weapon handle: {h}",
                hurt.weapon_id
            );
        }

        attacker.handle_damage_dealt(hurt, damage_type);
    }

    pub fn handle_tick(&mut self, tick: &DemoTick, server_tick: Option<&NetTickMessage>) {
        if *tick != self.tick {
            self.on_tick();
        }

        self.tick = *tick;

        let server_tick = server_tick.map(|x| u32::from(x.tick)).unwrap_or(0);

        self.server_tick = server_tick;

        // Must explicitly drop the old span to avoid creating
        // a cycle where the new span points to the old span.
        self.span = None;

        self.span = Some(
            error_span!("Tick", tick = u32::from(*tick), server_tick = server_tick,).entered(),
        );
    }

    // Do processing at the end of a tick, once all entities have been
    // processed. This is important when referring to entities that
    // may have been both created and referenced in the same packet.
    fn on_tick(&mut self) {
        for (_, v) in &self.state.player_summaries {
            if v.active_weapon_handle != 0 {
                let Some(_) = self.weapon_handles.get(&v.active_weapon_handle) else {
                    error!("could not find weapon handle {:?}", v.active_weapon_handle);
                    continue;
                };
            }
        }
    }
}

impl MessageHandler for MatchAnalyzer {
    type Output = DemoSummary;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(
            message_type,
            MessageType::PacketEntities
                | MessageType::GameEvent
                | MessageType::NetTick
                | MessageType::TempEntities
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
                for entity in message.entities.iter() {
                    self.handle_packet_entity(entity, parser_state);
                }
            }
            Message::TempEntities(te) => {
                for e in &te.events {
                    match u16::from(e.class_id) {
                        // 												165 => self.handle_temp_anim_entity(e),
                        // 												152 => self.handle_temp_fire_bullets_entity(e),
                        // 												177 => self.handle_temp_blood_entity(e),
                        // 												149 => self.handle_temp_effect_data_entity(e),
                        // 												179 => self.handle_temp_particle_effect_entity(e),
                        129 => {} // metal sparks
                        178 => {} // explosion
                        147 => {} // Dust particle
                        161 => {} // Metal sparks particle
                        171 => {} // Smoke
                        172 => {} // Spark particle
                        146 => {} // decal
                        166 => {} // player decal
                        180 => {} // world decal

                        _ => {
                            debug!("Unknown temp entity: {:?}", e);
                        }
                    }
                }
            }
            Message::GameEvent(GameEventMessage { event, .. }) => match event {
                GameEvent::PlayerShoot(_) => {
                    debug!("PlayerShoot");
                }
                GameEvent::PlayerDeath(death) => self.handle_player_death(death),
                GameEvent::PlayerHurt(hurt) => self.handle_player_hurt(hurt),
                GameEvent::TeamPlayPointCaptured(cap) => self.handle_point_captured(cap),
                GameEvent::TeamPlayCaptureBlocked(block) => self.handle_capture_blocked(block),
                GameEvent::RoundStart(_) => {
                    debug!("round start");
                    // self.state.buildings.clear();
                }
                GameEvent::TeamPlayRoundStart(_e) => {
                    debug!("TeamPlayRoundStart");
                    //self.state.buildings.clear();
                }
                GameEvent::ObjectDestroyed(ObjectDestroyedEvent { index: _, .. }) => {
                    debug!("ObjectDestroyed");
                    //self.state.remove_building((*index as u32).into());
                }

                // Some STVs demos don't have these events; they are
                // present in PoV demos and some STV demos (possibly
                // based on server side plugins?)
                GameEvent::PlayerDisconnect(d) => debug!("PlayerDisconnect {d:?}"),
                GameEvent::PlayerHealed(heal) => debug!("PlayerHealed {heal:?}"),
                GameEvent::PlayerInvulned(invuln) => debug!("PlayerDisconnect {invuln:?}"),
                GameEvent::PlayerChargeDeployed(c) => debug!("PlayerChargeDeployed {c:?}"),

                _ => {
                    trace!("Unhandled game event: {event:?}");
                    let event_string = format!("{:?}", event);
                    if event_string.contains("Shoot") {
                        trace!("Player shoot event");
                    }
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
        } else {
            trace!("Unhandled string entry: {index} {entry:?}");
        }
    }

    fn handle_data_tables(
        &mut self,
        _parse_tables: &[ParseSendTable],
        _server_classes: &[ServerClass],
        _parser_state: &ParserState,
    ) {
    }

    fn into_output(self, _parser_state: &ParserState) -> <Self as MessageHandler>::Output {
        self.state
    }
}
