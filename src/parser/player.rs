use crate::{
    parser::{
        game::{DamageType, Death, RoundState},
        is_false, is_zero,
        stats::Stats,
    },
    schema::{Attribute, Item},
    Vec3,
};
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tf_demo_parser::demo::{
    data::DemoTick, gameevent_gen::PlayerHurtEvent, message::packetentities::EntityId,
    parser::gamestateanalyser::Class,
};
use tracing::error;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PlayerSummary {
    pub name: String,
    pub steamid: String,

    pub tick_start: Option<DemoTick>,
    pub tick_end: Option<DemoTick>,
    pub points: Option<u32>,
    pub connection_count: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus_points: Option<u32>,

    #[serde(flatten)]
    pub stats: Stats,

    pub classes: HashMap<Class, Stats>,
    pub weapons: HashMap<String, Stats>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoreboard_kills: Option<u32>,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_kills: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoreboard_assists: Option<u32>, // Only present in PoV demos
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_assists: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub suicides: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoreboard_deaths: Option<u32>,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_deaths: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub captures: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub captures_blocked: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoreboard_damage: Option<u32>,

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
    #[serde(skip_serializing_if = "is_false")]
    pub is_fake_player: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_hl_tv: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_replay: bool,

    // Flags for internal state tracking but unused elsewhere
    #[serde(skip)]
    pub entity_id: EntityId,
    #[serde(skip)]
    pub user_id: u32,
    #[serde(skip)]
    pub on_ground: bool,
    #[serde(skip)]
    pub in_water: bool,
    #[serde(skip)]
    pub started_flying: DemoTick,
    #[serde(skip)]
    pub class: Class,
    #[serde(skip)]
    pub health: u32,

    // Temporary stat for tracking healing score changes, not the
    // actual stat
    #[serde(skip)]
    pub scoreboard_healing: u32,

    // TODO: Move this to always be read from the entity
    #[serde(skip)]
    pub origin: Vec3,

    #[serde(skip)]
    pub charge: f32, // ie med charge -- not wired to always be up to date!
    #[serde(skip)]
    pub kritzed: bool,
}

impl PlayerSummary {
    pub fn in_air(&self) -> bool {
        !self.on_ground && !self.in_water
    }

    pub fn class_stats(&mut self) -> &mut Stats {
        self.classes.entry(self.class).or_default()
    }

    pub fn weapon_stats(&mut self, weapon: &str) -> &mut Stats {
        self.weapons.entry(weapon.into()).or_default()
    }

    pub fn handle_fire_shot(&mut self, weapon: &str) {
        self.stats.handle_fire_shot();
        self.class_stats().handle_fire_shot();
        self.weapon_stats(weapon).handle_fire_shot();
    }

    pub fn handle_shot_hit(&mut self, weapon: &str) {
        self.stats.handle_shot_hit();
        self.class_stats().handle_shot_hit();
        self.weapon_stats(weapon).handle_shot_hit();
    }

    pub fn handle_object_built(&mut self, weapon: &str) {
        self.stats.handle_object_built();
        // This can only happen as engi, so no class_stats() update
        self.weapon_stats(weapon).handle_object_built();
    }

    pub fn handle_object_destroyed(&mut self, weapon: &str) {
        self.stats.handle_object_destroyed();
        self.class_stats().handle_object_destroyed();
        self.weapon_stats(weapon).handle_object_destroyed();
    }

    pub fn handle_damage_dealt(
        &mut self,
        weapon: &str,
        hurt: &PlayerHurtEvent,
        damage_type: DamageType,
    ) {
        self.stats.handle_damage_dealt(hurt, damage_type);
        self.class_stats().handle_damage_dealt(hurt, damage_type);
        self.weapon_stats(weapon)
            .handle_damage_dealt(hurt, damage_type);
    }

    pub fn handle_damage_taken(
        &mut self,
        weapon: &str,
        hurt: &PlayerHurtEvent,
        damage_type: DamageType,
    ) {
        self.stats.handle_damage_taken(hurt, damage_type);
        self.class_stats().handle_damage_taken(hurt, damage_type);
        self.weapon_stats(weapon)
            .handle_damage_taken(hurt, damage_type);
    }

    pub fn handle_assist(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        self.stats.handle_assist(round_state, flags);
        self.class_stats().handle_assist(round_state, flags);
    }

    pub fn handle_kill(
        &mut self,
        round_state: RoundState,
        weapon: &str,
        flags: EnumSet<Death>,
        damage_type: DamageType,
        airshot: bool,
    ) {
        self.stats
            .handle_kill(round_state, flags, damage_type, airshot);
        self.class_stats()
            .handle_kill(round_state, flags, damage_type, airshot);
        self.weapon_stats(weapon)
            .handle_kill(round_state, flags, damage_type, airshot);
    }

    pub fn handle_death(&mut self, round_state: RoundState, flags: EnumSet<Death>) {
        if self.class == Class::Medic && round_state == RoundState::Running {
            if self.charge == 1.0 {
                self.stats.handle_drop();
                self.class_stats().handle_drop();
            } else if self.charge > 0.95 {
                self.stats.handle_near_full_charge_death();
                self.class_stats().handle_near_full_charge_death();
            }
        }

        self.stats.handle_death(round_state, flags);
        self.class_stats().handle_death(round_state, flags);
    }

    pub fn handle_charge_uber(&mut self) {
        self.stats.handle_charge_uber();
        self.class_stats().handle_charge_uber();
    }
    pub fn handle_charge_kritz(&mut self) {
        self.stats.handle_charge_kritz();
        self.class_stats().handle_charge_kritz();
    }
    pub fn handle_charge_quickfix(&mut self) {
        self.stats.handle_charge_quickfix();
        self.class_stats().handle_charge_quickfix();
    }

    pub fn handle_healing(&mut self, round_state: RoundState, amount: u32) {
        self.stats.handle_healing(round_state, amount);
        self.class_stats().handle_healing(round_state, amount);
    }

    pub fn handle_capture(&mut self) {
        self.stats.handle_capture();
        self.class_stats().handle_capture();
    }

    pub fn handle_capture_blocked(&mut self) {
        self.stats.handle_capture_blocked();
        self.class_stats().handle_capture_blocked();
    }

    // Uber/Kritz/Quickfix
    //
    // TODO: Vaccinator.... find an indicative prop or maybe check
    // "lost ~0.25 and gained a resist type in the same tick"? But
    // vacc rarely takes 0.5 when activated(!?) and also lots of other
    // weird edge cases like if hit by a pomson while switching or if
    // popping vacc while headed by another vacc. See if there is an
    // indicative anim event or sound effect?
    pub fn handle_charged(&mut self, medigun_item: &Item) {
        let charge_type = medigun_item
            .attributes
            .get("set_charge_type")
            .and_then(|x| match x {
                Attribute::Float(float) => Some(float.value),
                _ => None,
            })
            .unwrap_or(0.0);

        match charge_type {
            0.0 => self.handle_charge_uber(),
            1.0 => self.handle_charge_kritz(),
            2.0 => self.handle_charge_quickfix(),
            x => error!("Unknown medigun charge type: {}", x),
        }
    }

    pub fn reset_stats(&mut self) {
        self.stats = Stats::default();
        self.classes.clear();
        self.weapons.clear();
        // scoreboard_healing is temporary and reset implicitly or explicitly elsewhere.
        // postround_kills, assists, deaths are reset by virtue of Stats::default()
        self.suicides = 0; // Reset suicides per round
        self.captures = 0;
        self.captures_blocked = 0;
        // charge and kritzed are transient states, not long-term stats to be reset here.
        // points, bonus_points, scoreboard_kills, scoreboard_assists, scoreboard_deaths, scoreboard_damage
        // are generally cumulative or snapshot from game messages, not reset here unless explicitly required
        // to be per-round from source. For now, assuming they reflect overall demo state or are updated by game.
    }
}
