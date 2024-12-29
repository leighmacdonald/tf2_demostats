use crate::parser::{
    game::RoundState,
    weapon::{Weapon, WeaponDetail},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tf_demo_parser::demo::gameevent_gen::ObjectDestroyedEvent;
use tf_demo_parser::demo::gamevent::GameEvent;
use tf_demo_parser::demo::message::gameevent::GameEventMessage;
use tf_demo_parser::demo::message::packetentities::{EntityId, PacketEntity};
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::packet::datatable::{ClassId, ParseSendTable, ServerClass};
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::gamestateanalyser::UserId;
use tf_demo_parser::demo::parser::gamestateanalyser::{Class, Team};
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::sendprop::{SendPropIdentifier, SendPropValue};
use tf_demo_parser::demo::{
    data::{DemoTick, UserInfo},
    gameevent_gen::PlayerDeathEvent,
};
use tf_demo_parser::{MessageType, ParserState, ReadResult, Stream};
use tracing::{error, info_span, span::EnteredSpan, trace};

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

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct DemoSummary {
    pub player_summaries: HashMap<UserId, PlayerSummary>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MatchAnalyzer {
    state: DemoSummary,
    user_entities: HashMap<EntityId, UserId>,
    users: BTreeMap<UserId, PlayerMeta>,
    waiting_for_players: bool,
    round_state: RoundState,
    user_id_map: HashMap<EntityId, u32>,
    #[serde(skip)]
    span: Option<EnteredSpan>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Killstreak {
    pub user_id: u32,
    pub class: Class,
    pub duration: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PlayerClass {
    pub class: Class,
    pub kills: u32,
    pub assists: u32,
    pub deaths: u32,
    pub playtime: u32,
    pub dominations: u32,
    pub dominated: u32,
    pub revenges: u32,
    pub damage: u32,
    pub damage_taken: u32,
    pub healing_taken: u32,
    pub captures: u32,
    pub captures_blocked: u32,
    pub building_destroyed: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct HealersSummary {
    pub healing: u32,
    pub charges_uber: u32,
    pub charges_kritz: u32,
    pub charges_vacc: u32,
    pub charges_quickfix: u32,
    pub drops: u32,
    pub near_full_charge_death: u32,
    pub avg_uber_length: u32,
    pub major_adv_lost: u32,
    pub biggest_adv_lost: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PlayerSummary {
    pub name: String,
    pub steamid: String,

    pub team: Team,
    pub time_start: u32, // ticks instead?
    pub time_end: u32,
    pub points: u32,
    pub connection_count: u32,
    pub bonus_points: u32,

    pub kills: u32,
    pub scoreboard_kills: u32,
    pub postround_kills: u32,

    pub assists: u32,
    pub scoreboard_assists: u32, // Only present in PoV demos
    pub postround_assists: u32,

    pub suicides: u32,

    pub deaths: u32,
    pub scoreboard_deaths: u32,
    pub postround_deaths: u32,

    // TODO
    pub defenses: u32,
    pub dominations: u32,
    pub dominated: u32,
    pub revenges: u32,
    pub damage: u32,
    pub damage_taken: u32,
    pub healing_taken: u32,
    pub health_packs: u32,
    pub healing_packs: u32, // total healing from packs
    pub captures: u32,
    pub captures_blocked: u32,
    pub extinguishes: u32,
    pub building_built: u32,
    pub buildings_destroyed: u32,
    pub airshots: u32,
    pub ubercharges: u32,
    pub headshots: u32,
    pub shots: u32,
    pub hits: u32,
    pub teleports: u32,
    pub backstabs: u32,
    pub support: u32,
    pub damage_dealt: u32,

    pub healing: HealersSummary,
    //pub bonus_points: u32,
    //pub support: u32,
    pub classes: HashMap<ClassId, PlayerClass>,
    pub killstreaks: Vec<Killstreak>,
    pub weapons: HashMap<Weapon, WeaponDetail>,
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
            trace!("user info {} {id} {entity_id}", user_info.player_info.name);

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

        let entity_id = &EntityId::from(entity.entity_index);
        let Some(user_id) = self.user_entities.get(entity_id) else {
            error!("Unknown player entity id: {}", entity_id);
            return;
        };

        let Some(summary) = self.state.player_summaries.get_mut(user_id) else {
            error!("Unknown player user id: {}", user_id);
            return;
        };

        for prop in &entity.props {
            trace!("Player entity {} {prop:?}", summary.name);
            let SendPropValue::Integer(val) = prop.value else {
                continue;
            };

            match prop.identifier {
                KILLS => {
                    summary.scoreboard_kills = val as u32;
                }
                KILL_ASSISTS => {
                    // PoV demos include multiple different copies of
                    // this field -- maybe per round stats? We want
                    // the larger one.
                    summary.scoreboard_assists = summary.scoreboard_assists.max(val as u32);
                }
                DEATHS => {
                    summary.scoreboard_deaths = val as u32;
                }
                _ => {}
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
                                // TODO
                            }
                            "m_iTotalScore" => {
                                player.points =
                                    i64::try_from(&prop.value).unwrap_or_default() as u32
                            }
                            "m_iDamage" => {
                                player.damage_dealt =
                                    i64::try_from(&prop.value).unwrap_or_default() as u32
                            }
                            "m_iDeaths" => {
                                player.scoreboard_deaths =
                                    i64::try_from(&prop.value).unwrap_or_default() as u32
                            }
                            "m_iScore" => {
                                // iScore is close to number of kills; but counts post-game kills and decrements on suicide.
                                player.scoreboard_kills =
                                    i64::try_from(&prop.value).unwrap_or_default() as u32
                            }
                            "m_iBonusPoints" => {
                                player.bonus_points =
                                    i64::try_from(&prop.value).unwrap_or_default() as u32
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
        trace!(
            "PlayerDeath {death:?} {} {:?}",
            self.waiting_for_players,
            self.round_state
        );
        if self.waiting_for_players {
            return;
        }
        let feigned = death.death_flags & 0x0020 != 0;

        if death.user_id == death.attacker {
            let Some(suicider) = self
                .state
                .player_summaries
                .get_mut(&UserId::from(death.attacker as u32))
            else {
                error!("Unknown suicider id: {}", death.user_id);
                return;
            };
            suicider.suicides += 1;
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
        if !feigned {
            if self.round_state == RoundState::TeamWin {
                victim.postround_deaths += 1;
            } else {
                victim.deaths += 1;
            }
        }

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
                    attacker.kills += 1;
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
            if !feigned {
                if self.round_state == RoundState::TeamWin {
                    assister.postround_assists += 1;
                } else {
                    assister.assists += 1;
                }
            }
        } else {
            error!("Unknown assister id: {}", death.assister);
            return;
        }
    }
}

impl MessageHandler for MatchAnalyzer {
    type Output = DemoSummary;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(
            message_type,
            MessageType::PacketEntities | MessageType::GameEvent | MessageType::NetTick
        )
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) {
        match message {
            Message::NetTick(t) => {
                // Must explicitly drop the old span to avoid creating
                // a cycle where the new span points to the old span.
                self.span = None;

                self.span = Some(
                    info_span!(
                        "Tick",
                        "Tick={},ServerTick={}",
                        (u32::from(tick)),
                        (u32::from(t.tick))
                    )
                    .entered(),
                );
            }
            Message::PacketEntities(message) => {
                for entity in message.entities.iter() {
                    self.handle_packet_entity(entity, parser_state);
                }
            }
            Message::GameEvent(GameEventMessage { event, .. }) => match event {
                GameEvent::PlayerShoot(_) => {
                    trace!("PlayerShoot");
                }
                GameEvent::PlayerDeath(death) => self.handle_player_death(death),
                GameEvent::RoundStart(_) => {
                    trace!("round start");
                    // self.state.buildings.clear();
                }
                GameEvent::TeamPlayRoundStart(_e) => {
                    trace!("TeamPlayRoundStart");
                    //self.state.buildings.clear();
                }
                GameEvent::ObjectDestroyed(ObjectDestroyedEvent { index: _, .. }) => {
                    //println!("ObjectDestroyed");
                    //self.state.remove_building((*index as u32).into());
                }
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
