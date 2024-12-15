use std::collections::HashMap;
use fnv::{FnvHashMap, FnvHashSet};
use tf_demo_parser::demo::packet::datatable::{ClassId, ParseSendTable, SendTableName, ServerClass};
use tf_demo_parser::demo::sendprop::{SendProp, SendPropIdentifier, SendPropName};
use tf_demo_parser::{MessageType, ParserState, ReadResult, Stream};
use tf_demo_parser::demo::data::{DemoTick, UserInfo};
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::MessageHandler;
use serde::{Deserialize, Serialize};
use tf_demo_parser::demo::gameevent_gen::ObjectDestroyedEvent;
use tf_demo_parser::demo::gamevent::GameEvent;
use tf_demo_parser::demo::message::gameevent::GameEventMessage;
use tf_demo_parser::demo::message::packetentities::{EntityId, PacketEntity};
use tf_demo_parser::demo::parser::gamestateanalyser::UserId;
use tf_demo_parser::demo::parser::player_summary_analyzer::PlayerSummaryState;
use crate::parser::weapon::{Weapon, WeaponDetail};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct MatchAnalyzer {
    props: FnvHashSet<SendPropIdentifier>,
    prop_names: FnvHashMap<SendPropIdentifier, (SendTableName, SendPropName)>,
    state: PlayerSummaryState,
    user_id_map: HashMap<EntityId, UserId>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PlayerSummary {
    pub points: u32,
    pub kills: u32,
    pub assists: u32,
    pub deaths: u32,
    pub buildings_destroyed: u32,
    pub captures: u32,
    pub defenses: u32,
    pub dominations: u32,
    pub revenges: u32,
    pub ubercharges: u32,
    pub headshots: u32,
    pub teleports: u32,
    pub healing: u32,
    pub backstabs: u32,
    pub bonus_points: u32,
    pub support: u32,
    pub damage_dealt: u32,
    pub weapon_map: HashMap<Weapon, WeaponDetail>,
}

impl MatchAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MessageHandler for MatchAnalyzer {
    type Output = PlayerSummaryState;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(message_type, MessageType::PacketEntities)
    }

    fn handle_message(&mut self, message: &Message, _tick: DemoTick, parser_state: &ParserState) {
        match message {
            // Message::PacketEntities(message) => {
            //     for entity in message.entities.iter() {
            //         self.handle_packet_entity(entity, parser_state);
            //     }
            // }

            Message::GameEvent(GameEventMessage { event, .. }) => match event {
                GameEvent::PlayerShoot(_) => {
                    println!("PlayerShoot");
                }
                GameEvent::PlayerDeath(death) => {
                    println!("PlayerDeath {:?}", death.user_id);
                    //self.state.kills.push(Kill::new(self.tick, death.as_ref()))
                }
                GameEvent::RoundStart(_) => {
                    println!("shoot");
                    // self.state.buildings.clear();
                }
                GameEvent::TeamPlayRoundStart(_) => {
                    println!("TeamPlayRoundStart");
                    //self.state.buildings.clear();
                }
                GameEvent::ObjectDestroyed(ObjectDestroyedEvent { index: _, .. }) => {
                    println!("ObjectDestroyed");
                    //self.state.remove_building((*index as u32).into());
                }
                _ => {
                    let event_string = format!("{:?}", event);
                    if event_string.contains("Shoot") {
                        println!("player shoot event");
                    }
                }
            },
            _ => {
                //println!("unhandled message: {message:?}");
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
        }
    }

    fn handle_data_tables(
        &mut self,
        parse_tables: &[ParseSendTable],
        _server_classes: &[ServerClass],
        _parser_state: &ParserState,
    ) {
        for table in parse_tables {
            for prop_def in &table.props {
                self.prop_names.insert(
                    prop_def.identifier(),
                    (table.name.clone(), prop_def.name.clone()),
                );
            }
        }
    }
    fn into_output(self, _parser_state: &ParserState) -> <Self as MessageHandler>::Output {
        self.state
    }

    // fn into_output(self, _state: &ParserState) -> Self::Output {
    //     let names = self.prop_names;
    //     let mut props = self
    //         .props
    //         .into_iter()
    //         .map(|prop| {
    //             let (table, name) = names.get(&prop).unwrap();
    //             format!("{}.{}", table, name)
    //         })
    //         .collect::<Vec<_>>();
    //     props.sort();
    //     props
    // }
}

/**
 * Helper function to make processing integer properties easier.
 *
 * parse_integer_prop(packet, "DT_TFPlayerScoringDataExclusive", "m_iPoints", |points| { println!("Scored {} points", points) });
 */
fn parse_integer_prop<F>(
    packet: &PacketEntity,
    table: &str,
    name: &str,
    parser_state: &ParserState,
    handler: F,
) where
    F: FnOnce(u32),
{
    use tf_demo_parser::demo::sendprop::SendPropValue;

    if let Some(SendProp {
                    value: SendPropValue::Integer(val),
                    ..
                }) = packet.get_prop_by_name(table, name, parser_state)
    {
        handler(val as u32);
    }
}


impl MatchAnalyzer {
    fn handle_packet_entity(&mut self, packet: &PacketEntity, parser_state: &ParserState) {
        use tf_demo_parser::demo::sendprop::SendPropValue;

        //println!("Known server classes: {:?}", parser_state.server_classes);

        if let Some(class) = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
        {
            //println!("Got a {} data packet: {:?}", class.name, packet);
            match class.name.as_str() {
                "CTFPlayer" => {
                    if let Some(user_id) = self.user_id_map.get(&packet.entity_index) {
                        let summaries = &mut self.state.player_summaries;
                        let player_summary = summaries.entry(*user_id).or_default();

                        // Extract scoreboard information, if present, and update the player's summary accordingly
                        // NOTE: Multiple DT_TFPlayerScoringDataExclusive structures may be present - one for the entire match,
                        //       and one for just the current round.  Since we're only interested in the overall match scores,
                        //       we need to ignore the round-specific values.  Fortunately, this is easy - just ignore the
                        //       lesser value (if multiple values are present), since none of these scores are able to decrement.

                        /*
                         * Member: m_iCaptures (offset 4) (type integer) (bits 10) (Unsigned)
                         * Member: m_iDefenses (offset 8) (type integer) (bits 10) (Unsigned)
                         * Member: m_iKills (offset 12) (type integer) (bits 10) (Unsigned)
                         * Member: m_iDeaths (offset 16) (type integer) (bits 10) (Unsigned)
                         * Member: m_iSuicides (offset 20) (type integer) (bits 10) (Unsigned)
                         * Member: m_iDominations (offset 24) (type integer) (bits 10) (Unsigned)
                         * Member: m_iRevenge (offset 28) (type integer) (bits 10) (Unsigned)
                         * Member: m_iBuildingsBuilt (offset 32) (type integer) (bits 10) (Unsigned)
                         * Member: m_iBuildingsDestroyed (offset 36) (type integer) (bits 10) (Unsigned)
                         * Member: m_iHeadshots (offset 40) (type integer) (bits 10) (Unsigned)
                         * Member: m_iBackstabs (offset 44) (type integer) (bits 10) (Unsigned)
                         * Member: m_iHealPoints (offset 48) (type integer) (bits 20) (Unsigned)
                         * Member: m_iInvulns (offset 52) (type integer) (bits 10) (Unsigned)
                         * Member: m_iTeleports (offset 56) (type integer) (bits 10) (Unsigned)
                         * Member: m_iDamageDone (offset 60) (type integer) (bits 20) (Unsigned)
                         * Member: m_iCrits (offset 64) (type integer) (bits 10) (Unsigned)
                         * Member: m_iResupplyPoints (offset 68) (type integer) (bits 10) (Unsigned)
                         * Member: m_iKillAssists (offset 72) (type integer) (bits 12) (Unsigned)
                         * Member: m_iBonusPoints (offset 76) (type integer) (bits 10) (Unsigned)
                         * Member: m_iPoints (offset 80) (type integer) (bits 10) (Unsigned)
                         *
                         * NOTE: support points aren't included here, but is equal to the sum of m_iHealingAssist and m_iDamageAssist
                         * TODO: pull data for support points
                         */
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iCaptures",
                            parser_state,
                            |captures| {
                                if captures > player_summary.captures {
                                    player_summary.captures = captures;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iDefenses",
                            parser_state,
                            |defenses| {
                                if defenses > player_summary.defenses {
                                    player_summary.defenses = defenses;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iKills",
                            parser_state,
                            |kills| {
                                if kills > player_summary.kills {
                                    // TODO: This might not be accruate.  Tested with a demo file with 89 kills (88 on the scoreboard),
                                    // but only a 83 were reported in the scoring data.
                                    player_summary.kills = kills;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iDeaths",
                            parser_state,
                            |deaths| {
                                if deaths > player_summary.deaths {
                                    player_summary.deaths = deaths;
                                }
                            },
                        );
                        // ignore m_iSuicides
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iDominations",
                            parser_state,
                            |dominations| {
                                if dominations > player_summary.dominations {
                                    player_summary.dominations = dominations;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iRevenge",
                            parser_state,
                            |revenges| {
                                if revenges > player_summary.revenges {
                                    player_summary.revenges = revenges;
                                }
                            },
                        );
                        // ignore m_iBuildingsBuilt
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iBuildingsDestroyed",
                            parser_state,
                            |buildings_destroyed| {
                                if buildings_destroyed > player_summary.buildings_destroyed {
                                    player_summary.buildings_destroyed = buildings_destroyed;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iHeadshots",
                            parser_state,
                            |headshots| {
                                if headshots > player_summary.headshots {
                                    player_summary.headshots = headshots;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iBackstabs",
                            parser_state,
                            |backstabs| {
                                if backstabs > player_summary.backstabs {
                                    player_summary.backstabs = backstabs;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iHealPoints",
                            parser_state,
                            |healing| {
                                if healing > player_summary.healing {
                                    player_summary.healing = healing;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iInvulns",
                            parser_state,
                            |ubercharges| {
                                if ubercharges > player_summary.ubercharges {
                                    player_summary.ubercharges = ubercharges;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iTeleports",
                            parser_state,
                            |teleports| {
                                if teleports > player_summary.teleports {
                                    player_summary.teleports = teleports;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iDamageDone",
                            parser_state,
                            |damage_dealt| {
                                if damage_dealt > player_summary.damage_dealt {
                                    player_summary.damage_dealt = damage_dealt;
                                }
                            },
                        );
                        // ignore m_iCrits
                        // ignore m_iResupplyPoints
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iKillAssists",
                            parser_state,
                            |assists| {
                                if assists > player_summary.assists {
                                    player_summary.assists = assists;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iBonusPoints",
                            parser_state,
                            |bonus_points| {
                                if bonus_points > player_summary.bonus_points {
                                    player_summary.bonus_points = bonus_points;
                                }
                            },
                        );
                        parse_integer_prop(
                            packet,
                            "DT_TFPlayerScoringDataExclusive",
                            "m_iPoints",
                            parser_state,
                            |points| {
                                if points > player_summary.points {
                                    player_summary.points = points;
                                }
                            },
                        );
                    }
                }
                "CTFPlayerResource" => {
                    // Player summaries - including entity IDs!
                    // look for props like m_iUserID.<entity_id> = <user_id>
                    // for example, `m_iUserID.024 = 2523` means entity 24 is user 2523
                    for i in 0..33 {
                        // 0 to 32, inclusive (1..33 might also work, not sure if there's a user 0 or not).  Not exhaustive and doesn't work for servers with > 32 players
                        if let Some(SendProp {
                                        value: SendPropValue::Integer(x),
                                        ..
                                    }) = packet.get_prop_by_name(
                            "m_iUserID",
                            format!("{:0>3}", i).as_str(),
                            parser_state,
                        ) {
                            let entity_id = EntityId::from(i as u32);
                            let user_id = UserId::from(x as u32);
                            self.user_id_map.insert(entity_id, user_id);
                        }
                    }
                }
                "CTFTeam" => {}
                "CWorld" => {}
                "CTFObjectiveResource" => {}
                "CMonsterResource" => {}
                "CMannVsMachineStats" => {}
                "CBaseEntity" => {}
                "CVoteController" => {}
                "CPhysicsProp" => {}
                "CFuncTrackTrain" => {}
                "CObjectCartDispenser" => {}
                "CParticleSystem" => {}
                "CDynamicProp" => {}
                "CBaseDoor" => {}
                "CBaseAnimating" => {}
                "CFuncRespawnRoomVisualizer" => {}
                "CSprite" => {}
                "CRopeKeyframe" => {}
                "CLightGlow" => {}
                "CTFGameRulesProxy" => {}
                "CTeamRoundTimer" => {}
                "CTeamTrainWatcher" => {}
                "CEnvTonemapController" => {}
                "CFuncAreaPortalWindow" => {}
                "CFogController" => {}
                "CFunc_Dust" => {}
                "CBeam" => {}
                "CShadowControl" => {}
                "CTFViewModel" => {}
                "CSceneEntity" => {}
                "CTFSniperRifle" => {}
                "CTFSMG" => {}
                "CTFClub" => {}
                "CTFWearable" => {}
                "CTFFlameThrower" => {}
                "CTFFlareGun_Revenge" => {}
                "CTFFireAxe" => {}
                "CTFRocketLauncher" => {}
                "CTFShotgun_Soldier" => {}
                "CTFShovel" => {}
                "CTFFlareGun" => {}
                "CTFSlap" => {}
                "CTFMinigun" => {}
                "CTFLunchBox" => {}
                "CTFFists" => {}
                "CTFFlameManager" => {}
                "CTFGrenadeLauncher" => {}
                "CTFPipebombLauncher" => {}
                "CTFBottle" => {}
                "CTFScatterGun" => {}
                "CTFCleaver" => {}
                "CTFBat_Wood" => {}
                "CTFRocketLauncher_DirectHit" => {}
                "CTFProjectile_Rocket" => {}
                "CTFProjectile_Flare" => {}
                "CTFBuffItem" => {}
                "CTFPEPBrawlerBlaster" => {}
                "CTFJarMilk" => {}
                "CTFBat" => {}
                "CTFStunBall" => {}
                "CSpriteTrail" => {}
                "CSniperDot" => {}
                "CTFGrenadePipebombProjectile" => {}
                "CTFProjectile_Cleaver" => {}
                "CTFDroppedWeapon" => {}
                "CTFAmmoPack" => {}
                "CHalloweenSoulPack" => {}
                "CTFRagdoll" => {}
                "CTFRevolver" => {}
                "CTFKnife" => {}
                "CTFWeaponBuilder" => {}
                "CTFWeaponPDA_Spy" => {}
                "CTFWeaponInvis" => {}
                "CVGuiScreen" => {}
                "CTFShotgun_HWG" => {}
                "CTFPistol_Scout" => {}
                "CTFSniperRifleDecap" => {}
                "CTFJar" => {}
                "CTFCrossbow" => {}
                "CWeaponMedigun" => {}
                "CTFBonesaw" => {}
                "CTFProjectile_HealingBolt" => {}
                "CTFWearableRazorback" => {}
                "CTFCannon" => {}
                "CTFProjectile_JarMilk" => {}
                "CTFWeaponSapper" => {}
                "CTFSniperRifleClassic" => {}
                "CTFWearableVM" => {}
                "CTFPistol_ScoutSecondary" => {}
                "CTFWearableDemoShield" => {}
                "CTFSword" => {}
                "CTFShotgun_Pyro" => {}
                "CTFPowerupBottle" => {}
                "CTFWeaponFlameBall" => {}
                "CTFProjectile_BallOfFire" => {}
                "CTFShotgunBuildingRescue" => {}
                "CTFLaserPointer" => {}
                "CTFWrench" => {}
                "CTFWeaponPDA_Engineer_Build" => {}
                "CTFWeaponPDA_Engineer_Destroy" => {}
                "CTFJarGas" => {}
                "CTFSpellBook" => {}
                "CObjectTeleporter" => {}
                "CObjectSentrygun" => {}
                "CLaserDot" => {}
                "CObjectDispenser" => {}
                "CTFProjectile_Arrow" => {}
                "CObjectSapper" => {}
                "CTFPistol" => {}
                "CTFRobotArm" => {}
                "CTFWearableRobotArm" => {}
                "CTFShotgun" => {}
                "CTFParticleCannon" => {}
                "CTFProjectile_EnergyBall" => {}
                "CTFShotgun_Revenge" => {}
                "CTFKatana" => {}
                "CTFMechanicalArm" => {}
                "CTFProjectile_MechanicalArmOrb" => {}
                "CTFProjectile_JarGas" => {}
                "CTFGasManager" => {}
                "CTFTauntProp" => {}
                "CTFSyringeGun" => {}
                "CTFProjectile_Jar" => {}
                "CTFSodaPopper" => {}
                "CTFRocketPack" => {}
                "CTFProjectile_SentryRocket" => {}
                "CTFBat_Giftwrap" => {}
                "CTFBall_Ornament" => {}
                "CTFCompoundBow" => {}
                "CTFChargedSMG" => {}
                "CSun" => {}
                "CFunc_LOD" => {}
                "CTFPistol_ScoutPrimary" => {}
                "CCaptureFlag" => {}
                "CCaptureFlagReturnIcon" => {}
                "CBoneFollower" => {}
                "CWaterLODControl" => {}
                "CFuncOccluder" => {}
                "CCaptureZone" => {}
                "CTFProjectile_EnergyRing" => {}
                "CTFRaygun" => {}
                "CTFLunchBox_Drink" => {}
                "CTFBat_Fish" => {}
                "CFuncRotating" => {}
                "CPhysicsPropMultiplayer" => {}
                "CTFPlayerDestructionLogic" => {}
                "CSpotlightEnd" => {}
                "CTFRocketLauncher_AirStrike" => {}
                "CTFParachute_Secondary" => {}
                "CTFBreakableSign" => {}

                _other => {
                    println!("\"{}\" => {}", class.name.as_str(), "{}");
                }
            }
        }
    }

    fn parse_user_info(
        &mut self,
        index: usize,
        text: Option<&str>,
        data: Option<Stream>,
    ) -> ReadResult<()> {
        if let Some(user_info) =
            UserInfo::parse_from_string_table(index as u16, text, data)?
        {
            self.state
                .users
                .entry(user_info.player_info.user_id)
                .and_modify(|info| {
                    info.entity_id = user_info.entity_id;
                })
                .or_insert_with(|| user_info.into());
        }

        Ok(())
    }
}