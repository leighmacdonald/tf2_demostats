use crate::parser::{
    game::{DamageType, Death, RoundState},
    is_zero,
};
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use tf_demo_parser::demo::gameevent_gen::PlayerHurtEvent;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

    #[serde(skip_serializing_if = "is_zero")]
    pub preround_healing: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub healing: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub postround_healing: u32,

    // med stats
    #[serde(skip_serializing_if = "is_zero")]
    pub drops: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub near_full_charge_death: u32, // TODO: This should be a continuous variable to be a more smooth metric

    // TODO: consolidate these to a single "charge deployed" stat, differentiated by weapon like
    // every other stat
    #[serde(skip_serializing_if = "is_zero")]
    pub charges_uber: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub charges_kritz: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub charges_quickfix: u32,

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

    #[serde(skip_serializing_if = "is_zero")]
    pub object_built: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_destroyed: u32,

    // Discrete heal events (PlayerHealed) as the healer. Kept separate from
    // medigun-sustain `healing`: the event also fires for self-regen ticks,
    // kits attributed to nobody (healer 0, skipped), and possibly crossbow
    // bolts (which additionally fire CrossbowHeal, counted below).
    #[serde(skip_serializing_if = "is_zero")]
    pub heals: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub healed: u32,

    // Crusader's Crossbow bolts (CrossbowHeal) as the healer.
    #[serde(skip_serializing_if = "is_zero")]
    pub crossbow_heals: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub crossbow_healing: u32,

    // Health-gain notices (PlayerHealOnHit) as the recipient. Observed to
    // mirror most PlayerHealed amounts (kits, regen, crossbow) with the
    // recipient's active weapon defindex attached (65535 = none/unknown).
    #[serde(skip_serializing_if = "is_zero")]
    pub heal_on_hit: u32,

    #[serde(skip_serializing_if = "is_zero")]
    pub extinguishes: u32,

    // Wrench healing on buildings (BuildingHealed) as the healer.
    #[serde(skip_serializing_if = "is_zero")]
    pub building_healing: u32,

    // Medic died with full uber (MedicDeath.charged). Event-sourced
    // complement to entity-derived `drops` (charge prop tracking is
    // lossy); the two should roughly agree.
    #[serde(skip_serializing_if = "is_zero")]
    pub dropped_ubers: u32,

    // Airblast reflects (ObjectDeflected) as the deflector.
    #[serde(skip_serializing_if = "is_zero")]
    pub reflects: u32,

    // Killed an enemy who was capping (KilledCappingPlayer as killer,
    // CapperKilled as blocker).
    #[serde(skip_serializing_if = "is_zero")]
    pub defenses: u32,

    // Direct projectile hits that were not kills (ProjectileDirectHit).
    #[serde(skip_serializing_if = "is_zero")]
    pub direct_hits: u32,

    // Teammates moved via this player's teleporter (PlayerTeleported).
    #[serde(skip_serializing_if = "is_zero")]
    pub teleports: u32,

    // Cart push distance attributed to the pusher (PayloadPushed).
    #[serde(skip_serializing_if = "is_zero")]
    pub push_distance: u32,

    // Trigger-hurt/environment kills. Victims are also counted in `deaths`
    // via player_death (attacker 0 = world); these tag the subset.
    #[serde(skip_serializing_if = "is_zero")]
    pub environmental_deaths: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub environmental_kills: u32,

    // Building lifecycle from broadcast events (PlayerBuiltObject family).
    // NOTE: `object_placed` overlaps entity-derived `object_built`
    // (placements vs completions-in-PVS); it additionally covers sappers,
    // which entities never count.
    #[serde(skip_serializing_if = "is_zero")]
    pub object_placed: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_upgraded: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_carried: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_dropped: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_removed: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub object_detonated: u32,

    // Ammo pack pickups (ItemPickup with ammo-ish names).
    #[serde(skip_serializing_if = "is_zero")]
    pub ammo_packs: u32,

    // Health-kit pickups (take_health, attributed via player entity).
    #[serde(skip_serializing_if = "is_zero")]
    pub health_packs: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub health_pack_healing: u32,
}

impl Stats {
    pub fn handle_fire_shot(&mut self) {
        self.shots += 1;
    }

    // Not done as part of handle_damage_dealt as we don't want to count sentry or damage over time.
    pub fn handle_shot_hit(&mut self) {
        self.hits += 1;
    }

    pub fn handle_object_built(&mut self) {
        self.object_built += 1;
    }

    pub fn handle_object_destroyed(&mut self) {
        self.object_destroyed += 1;
    }

    pub fn handle_drop(&mut self) {
        self.drops += 1;
    }

    pub fn handle_near_full_charge_death(&mut self) {
        self.near_full_charge_death += 1;
    }

    pub fn handle_charge_uber(&mut self) {
        self.charges_uber += 1;
    }
    pub fn handle_charge_kritz(&mut self) {
        self.charges_kritz += 1;
    }
    pub fn handle_charge_quickfix(&mut self) {
        self.charges_quickfix += 1;
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
        if round_state == RoundState::PreRound {
            self.preround_healing += amount;
        } else if round_state == RoundState::TeamWin {
            self.postround_healing += amount;
        } else {
            self.healing += amount;
        }
    }

    pub fn handle_heal_given(&mut self, amount: u32) {
        self.heals += 1;
        self.healed += amount;
    }

    pub fn handle_crossbow_heal(&mut self, amount: u32) {
        self.crossbow_heals += 1;
        self.crossbow_healing += amount;
    }

    pub fn handle_heal_on_hit(&mut self, amount: u32) {
        self.heal_on_hit += amount;
    }

    pub fn handle_extinguish(&mut self) {
        self.extinguishes += 1;
    }

    pub fn handle_building_heal(&mut self, amount: u32) {
        self.building_healing += amount;
    }

    pub fn handle_dropped_uber(&mut self) {
        self.dropped_ubers += 1;
    }

    pub fn handle_reflect(&mut self) {
        self.reflects += 1;
    }

    pub fn handle_defense(&mut self) {
        self.defenses += 1;
    }

    pub fn handle_direct_hit(&mut self) {
        self.direct_hits += 1;
    }

    pub fn handle_teleport(&mut self) {
        self.teleports += 1;
    }

    pub fn handle_push(&mut self, distance: u32) {
        self.push_distance += distance;
    }

    pub fn handle_environmental_death(&mut self) {
        self.environmental_deaths += 1;
    }

    pub fn handle_environmental_kill(&mut self) {
        self.environmental_kills += 1;
    }

    pub fn handle_object_placed(&mut self) {
        self.object_placed += 1;
    }

    pub fn handle_object_upgraded(&mut self) {
        self.object_upgraded += 1;
    }

    pub fn handle_object_carried(&mut self) {
        self.object_carried += 1;
    }

    pub fn handle_object_dropped(&mut self) {
        self.object_dropped += 1;
    }

    pub fn handle_object_removed(&mut self) {
        self.object_removed += 1;
    }

    pub fn handle_object_detonated(&mut self) {
        self.object_detonated += 1;
    }

    pub fn handle_ammo_pack(&mut self) {
        self.ammo_packs += 1;
    }

    pub fn handle_health_pack(&mut self, amount: u32) {
        self.health_packs += 1;
        self.health_pack_healing += amount;
    }
}
