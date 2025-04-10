use crate::parser::{
    game::{DamageType, Death, RoundState},
    is_zero,
};
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use tf_demo_parser::demo::gameevent_gen::PlayerHurtEvent;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Stats {
    #[serde(skip_serializing_if = "is_zero")]
    pub kills: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub assists: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub deaths: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_kills: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_assists: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_deaths: u32,

    // Dupes with HealersSummary to cover non-med healing
    #[serde(skip_serializing_if = "is_zero")]
    pub preround_healing: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub healing: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_healing: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub damage: u32, // Added up PlayerHurt events
    #[serde(skip_serializing_if = "is_zero")]
    pub damage_taken: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub dominations: u32, // This player dominated another player
    #[serde(skip_serializing_if = "is_zero")]
    pub dominated: u32, // Another player dominated this player
    #[serde(skip_serializing_if = "is_zero")]
    pub revenges: u32, // This player got revenge on another player
    #[serde(skip_serializing_if = "is_zero")]
    pub revenged: u32, // Another player got revenge on this player

    // Kills where the victim was in the air for a decent amount of time.
    // TOOD: clarify this definition
    #[serde(skip_serializing_if = "is_zero")]
    pub airshots: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub headshot_kills: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub backstab_kills: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub headshots: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub backstabs: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub captures: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub captures_blocked: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub was_headshot: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub was_backstabbed: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub shots: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub hits: u32,
}

impl Stats {
    pub fn handle_fire_shot(&mut self) {
        self.shots += 1;
    }

    // Not done as part of handle_damage_dealt as we don't want to count sentry or damage over time.
    pub fn handle_shot_hit(&mut self) {
        self.hits += 1;
    }

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

    pub fn handle_capture(&mut self) {
        self.captures += 1;
    }

    pub fn handle_capture_blocked(&mut self) {
        self.captures_blocked += 1;
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
