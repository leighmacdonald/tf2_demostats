use crate::{
    parser::{
        entity::{Entity, EntityClass},
        game::{update_condition, Flags, PlayerCondition, INVALID_HANDLE},
        props::*,
        summarizer::MatchAnalyzerView,
    },
    Vec2, Vec3,
};
use enumset::EnumSet;
use std::any::Any;
use tf_demo_parser::{
    demo::{
        data::DemoTick,
        message::packetentities::PacketEntity,
        parser::analyser::{Class, Team, UserId},
        sendprop::SendPropValue,
        vector::VectorXY,
    },
    ParserState,
};
use tracing::{error, trace};

//#[optfield::optfield(PlayerPatch, merge_fn, attrs)]
#[derive(Debug, PartialEq, Default)]
pub struct Player {
    pub class: Class,
    pub team: Team,
    pub tick_start: Option<DemoTick>,
    pub tick_end: Option<DemoTick>,
    pub user_id: UserId,

    pub health: u32,

    // medic
    pub charge: f32,
    pub kritzed: bool,

    pub points: Option<u32>,
    pub connection_count: u32,
    pub bonus_points: Option<u32>,

    pub scoreboard_kills: u32,
    pub scoreboard_assists: u32,
    pub scoreboard_deaths: u32,
    pub scoreboard_healing: u32,

    pub captures: u32,
    pub captures_blocked: u32,

    pub scoreboard_damage: u32,
    pub on_ground: bool,
    pub in_water: bool,
    pub started_flying: DemoTick,

    pub sim_time: u32,
    pub origin: Vec3,
    pub eye: Vec2,
    pub condition: EnumSet<PlayerCondition>,
    pub condition_source: u32,

    pub last_active_weapon_handle: u32,
    pub active_weapon_handle: u32,
    pub weapon_handles: Box<[u32; 7]>,
    pub cosmetic_handles: Box<[u32; 8]>,

    pub handle: u32,
}

#[derive(Default)]
struct PlayerPatch {
    scoreboard_kills: Option<u32>,
    scoreboard_assists: Option<u32>,
    scoreboard_deaths: Option<u32>,
    flags: Option<EnumSet<Flags>>,
    class: Option<Class>,
    team: Option<Team>,
    health: Option<u32>,
    sim_time: Option<u32>,
    origin_xy: Option<VectorXY>,
    origin_z: Option<f32>,
    eye_x: Option<f32>,
    eye_y: Option<f32>,
    handle: Option<u32>,
    kritzed: Option<bool>,
    active_weapon_handle: Option<u32>,
    condition_source: Option<u32>,
    condition_bits: [Option<u32>; 4],
    weapon_handles: [Option<u32>; 7],

    num_cosmetics: Option<u32>,
    cosmetics: [Option<u32>; 8],
}

impl Player {
    fn parse(packet: &PacketEntity, parser_state: &ParserState, patch: &mut PlayerPatch) {
        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (KILLS, &SendPropValue::Integer(val)) => {
                    patch.scoreboard_kills = Some(val as u32);
                }
                (KILL_ASSISTS, &SendPropValue::Integer(val)) => {
                    // PoV demos include multiple different copies of
                    // this field -- maybe per round stats? We want
                    // the larger one.
                    // TODO
                    patch.scoreboard_assists = Some(val as u32);
                }
                (DEATHS, &SendPropValue::Integer(val)) => {
                    patch.scoreboard_deaths = Some(val as u32);
                }
                (FLAGS, &SendPropValue::Integer(val)) => {
                    patch.flags = Some(EnumSet::<Flags>::try_from_repr(val as u32).unwrap_or_else(
                        || {
                            error!("Unknown player flags: {}", val);
                            EnumSet::<Flags>::new()
                        },
                    ));
                }
                (CLASS, &SendPropValue::Integer(val)) => {
                    let Ok(class) = Class::try_from(val as u8) else {
                        error!("Unknown classid {val}");
                        continue;
                    };
                    patch.class = Some(class);
                }
                (TEAM, &SendPropValue::Integer(val)) => {
                    let Ok(team) = Team::try_from(val as u8) else {
                        error!("Unknown team id {val}");
                        continue;
                    };
                    patch.team = Some(team);
                }
                (HEALTH, &SendPropValue::Integer(val)) => {
                    patch.health = Some(val as u32);
                }
                (SIM_TIME, &SendPropValue::Integer(val)) => {
                    patch.sim_time = Some(val as u32);
                }

                (ORIGIN_XY, &SendPropValue::VectorXY(vec)) => {
                    patch.origin_xy = Some(vec);
                }
                (ORIGIN_Z, &SendPropValue::Float(z)) => patch.origin_z = Some(z),
                (EYE_X, &SendPropValue::Float(x)) => patch.eye_x = Some(x),
                (EYE_Y, &SendPropValue::Float(y)) => patch.eye_y = Some(y),

                (HANDLE, &SendPropValue::Integer(h)) => {
                    patch.handle = Some(h as u32);
                }
                (COND_SOURCE, &SendPropValue::Integer(x)) => {
                    patch.condition_source = Some(x as u32)
                }

                (COND_0, &SendPropValue::Integer(x)) => patch.condition_bits[0] = Some(x as u32),
                (COND_1, &SendPropValue::Integer(x)) => patch.condition_bits[1] = Some(x as u32),
                (COND_2, &SendPropValue::Integer(x)) => patch.condition_bits[2] = Some(x as u32),
                (COND_3, &SendPropValue::Integer(x)) => patch.condition_bits[3] = Some(x as u32),

                (COND_BITS, &SendPropValue::Integer(x)) => {
                    if x == 0 || x == 2048 {
                        patch.kritzed = Some(x == 2048);
                    } else {
                        error!("Unknown _condition_bits: {x}");
                    }
                }

                (ACTIVE_WEAPON_HANDLE, &SendPropValue::Integer(x)) => {
                    patch.active_weapon_handle = Some(x as u32);
                }

                (WEP_0, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[0] = Some(x as u32);
                }
                (WEP_1, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[1] = Some(x as u32);
                }
                (WEP_2, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[2] = Some(x as u32);
                }
                (WEP_3, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[3] = Some(x as u32);
                }
                (WEP_4, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[4] = Some(x as u32);
                }
                (WEP_5, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[5] = Some(x as u32);
                }
                (WEP_6, &SendPropValue::Integer(x)) => {
                    patch.weapon_handles[6] = Some(x as u32);
                }

                (NUM_COSMETICS, &SendPropValue::Integer(n)) => patch.num_cosmetics = Some(n as u32),
                (COSMETIC_0, &SendPropValue::Integer(x)) => patch.cosmetics[0] = Some(x as u32),
                (COSMETIC_1, &SendPropValue::Integer(x)) => patch.cosmetics[1] = Some(x as u32),
                (COSMETIC_2, &SendPropValue::Integer(x)) => patch.cosmetics[2] = Some(x as u32),
                (COSMETIC_3, &SendPropValue::Integer(x)) => patch.cosmetics[3] = Some(x as u32),
                (COSMETIC_4, &SendPropValue::Integer(x)) => patch.cosmetics[4] = Some(x as u32),
                (COSMETIC_5, &SendPropValue::Integer(x)) => patch.cosmetics[5] = Some(x as u32),
                (COSMETIC_6, &SendPropValue::Integer(x)) => patch.cosmetics[6] = Some(x as u32),
                (COSMETIC_7, &SendPropValue::Integer(x)) => patch.cosmetics[7] = Some(x as u32),

                _ => {}
            }
            trace!("player wep {:?} {prop:?}", packet.entity_index);
        }
    }

    fn apply_patch(&mut self, patch: &PlayerPatch) {
        self.handle = patch.handle.unwrap_or(self.handle);
        self.health = patch.health.unwrap_or(self.health);
        self.condition_source = patch.condition_source.unwrap_or(self.condition_source);
        self.kritzed = patch.kritzed.unwrap_or(self.kritzed);
        self.class = patch.class.unwrap_or(self.class);
        self.team = patch.team.unwrap_or(self.team);

        if let Some(xy) = patch.origin_xy {
            self.origin.x = xy.x;
            self.origin.y = xy.y;
        }
        if let Some(z) = patch.origin_z {
            self.origin.z = z;
        }

        if let Some(x) = patch.eye_x {
            self.eye.x = x;
        }
        if let Some(y) = patch.eye_y {
            self.eye.y = y;
        }

        if let Some(bits) = patch.condition_bits[0] {
            update_condition::<0>(&mut self.condition, bits);
        }
        if let Some(bits) = patch.condition_bits[1] {
            update_condition::<32>(&mut self.condition, bits);
        }
        if let Some(bits) = patch.condition_bits[2] {
            update_condition::<64>(&mut self.condition, bits);
        }
        if let Some(bits) = patch.condition_bits[3] {
            update_condition::<96>(&mut self.condition, bits);
        }

        if let Some(aw) = patch.active_weapon_handle {
            self.active_weapon_handle = aw;
            if aw != INVALID_HANDLE {
                self.last_active_weapon_handle = aw;
            }
        }

        for (i, &w) in patch.weapon_handles.iter().enumerate() {
            if let Some(w) = w {
                self.weapon_handles[i] = w;
            }
        }

        for (i, &c) in patch.cosmetics.iter().enumerate() {
            if let Some(c) = c {
                self.cosmetic_handles[i] = c;
            }
        }
    }
}

impl Entity for Player {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Self {
        let mut patch = PlayerPatch::default();
        Player::parse(packet, parser_state, &mut patch);

        let mut s = Self::default();

        if let Some(&user_id) = game.user_entities.get(&packet.entity_index) {
            s.user_id = user_id;

            if let Some(summary) = game.player_summaries.get_mut(&user_id) {
                summary.class = patch.class.unwrap_or(summary.class);
                summary.health = patch.health.unwrap_or(summary.health);

                for &w in patch.weapon_handles.iter() {
                    if let Some(w) = w {
                        game.weapon_owners.insert(w, user_id);
                    }
                }

                for &c in patch.cosmetics.iter() {
                    if let Some(c) = c {
                        game.cosmetic_owners.insert(c, user_id);
                    }
                }
            } else {
                error!("No summary for new player user id: {}", s.user_id);
            }
        } else {
            error!("No user id ready for new user! {packet:?}")
        }

        s.apply_patch(&patch);

        s.tick_start = Some(game.tick);

        s
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        let user_id = self.user_id;

        let mut patch = Box::new(PlayerPatch::default());
        Player::parse(packet, parser_state, &mut patch);

        let Some(summary) = game.player_summaries.get_mut(&user_id) else {
            error!("Unknown player user id: {}", user_id);
            return patch;
        };

        if let Some(kills) = patch.scoreboard_kills {
            summary.scoreboard_kills = Some(kills);
        }

        // PoV demos include multiple different copies of this field -- maybe per round stats? We
        // want the larger one.*val as u32));
        if let Some(assists) = patch.scoreboard_assists {
            summary.scoreboard_assists = Some(summary.scoreboard_assists.unwrap_or(0).max(assists));
        }

        // PoV demos include multiple different copies of this field -- maybe per round stats? We
        // want the larger one.*val as u32));
        if let Some(deaths) = patch.scoreboard_deaths {
            summary.scoreboard_deaths = Some(deaths);
        }

        if let Some(flags) = patch.flags {
            let was_in_air = summary.in_air();
            summary.on_ground = flags.contains(Flags::OnGround);
            summary.in_water = flags.contains(Flags::InWater);
            let now_in_air = summary.in_air();
            if !was_in_air && now_in_air {
                summary.started_flying = game.tick;
            }
        }

        if let Some(xy) = patch.origin_xy {
            summary.origin.x = xy.x;
            summary.origin.y = xy.y;
        }
        if let Some(z) = patch.origin_z {
            summary.origin.z = z;
        }

        if let Some(aw) = patch.active_weapon_handle {
            game.weapon_owners.insert(aw, user_id);
        }

        for &w in patch.weapon_handles.iter() {
            if let Some(w) = w {
                game.weapon_owners.insert(w, user_id);
            }
        }

        for &c in patch.cosmetics.iter() {
            if let Some(c) = c {
                game.cosmetic_owners.insert(c, user_id);
            }
        }

        summary.class = patch.class.unwrap_or(summary.class);
        summary.health = patch.health.unwrap_or(summary.health);

        patch
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<PlayerPatch>().unwrap();
        self.apply_patch(&patch);
    }

    fn delete(self: Box<Self>, game: &mut MatchAnalyzerView) {
        let user_id = self.user_id;
        let Some(summary) = game.player_summaries.get_mut(&user_id) else {
            error!("Unknown player user id: {}", user_id);
            return;
        };
        trace!("Player left {self:?}");
        if summary.tick_end.is_some() {
            error!("Player left twice?");
        }
        summary.tick_end = Some(game.tick);
    }

    fn handle(&self) -> Option<u32> {
        Some(self.handle)
    }

    fn class(&self) -> EntityClass {
        EntityClass::Player
    }

    fn player(&self) -> Option<&Player> {
        Some(self)
    }
}
