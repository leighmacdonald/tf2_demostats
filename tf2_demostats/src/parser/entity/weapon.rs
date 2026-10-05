use crate::parser::{
    entity::{Entity, EntityClass},
    game::INVALID_HANDLE,
    props::{
        ITEM_DEFINITION, MEDIGUN_CHARGE_LEVEL, MEDIGUN_CHARGE_RELEASED, MEDIGUN_HEALING_TARGET,
        MODEL, RESET_PARITY, SELF_HANDLE, WEAPON_OWNER,
    },
    summarizer::{Event, MatchAnalyzerView},
};
use std::any::Any;
use tf_demo_parser::{
    ParserState,
    demo::{
        message::packetentities::PacketEntity, packet::datatable::ClassId, sendprop::SendPropValue,
    },
};
use tracing::trace;

#[optfield::optfield(WeaponPatch, merge_fn, attrs)]
#[derive(Debug, PartialEq, Default)]
pub struct Weapon {
    pub class_name: String,

    // medigunn
    pub last_high_charge: f32,
    pub charge: f32,
    pub charge_released: bool,

    /// Entity handle of the current heal target (`m_hHealingTarget`),
    /// or `INVALID_HANDLE` when beaming nobody. Only mediguns carry it.
    pub healing_target: u32,

    pub handle: u32,
    pub owner: u32,

    pub schema_id: u32,
    pub model_id: u32,

    pub reset_parity: u32,
}

impl Weapon {
    fn parse(packet: &PacketEntity, parser_state: &ParserState, patch: &mut WeaponPatch) {
        let class_name = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
            .map_or("UNKNOWN_PROJECTILE".to_string(), |s| s.name.to_string());
        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (MEDIGUN_CHARGE_LEVEL, &SendPropValue::Float(z)) => {
                    patch.charge = Some(z);
                }
                (MEDIGUN_CHARGE_RELEASED, &SendPropValue::Integer(b)) => {
                    patch.charge_released = Some(b == 1);
                }
                (MEDIGUN_HEALING_TARGET, &SendPropValue::Integer(h)) => {
                    patch.healing_target = Some(u32::try_from(h).unwrap_or_default());
                }
                (SELF_HANDLE, &SendPropValue::Integer(h)) => {
                    patch.handle = Some(u32::try_from(h).unwrap_or_default());
                }
                (ITEM_DEFINITION, &SendPropValue::Integer(x)) => {
                    patch.schema_id = Some(u32::try_from(x).unwrap_or_default());
                }
                (MODEL, &SendPropValue::Integer(x)) => {
                    patch.model_id = Some(u32::try_from(x).unwrap_or_default());
                }
                (WEAPON_OWNER, &SendPropValue::Integer(x)) => {
                    patch.owner = Some(u32::try_from(x).unwrap_or_default());
                }
                (RESET_PARITY, &SendPropValue::Integer(x)) => {
                    patch.reset_parity = Some(u32::try_from(x).unwrap_or_default());
                }

                _ => {
                    trace!(
                        "Unknown weaapon prop on {} {class_name}: {prop:?}",
                        packet.entity_index
                    );
                }
            }
        }
    }
}

impl Entity for Weapon {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        _game: &mut MatchAnalyzerView,
    ) -> Self {
        let class_name = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
            .map_or("UNKNOWN_PROJECTILE".to_string(), |s| s.name.to_string());

        let mut p = WeaponPatch::default();
        Weapon::parse(packet, parser_state, &mut p);

        let mut s = Self {
            class_name,
            ..Default::default()
        };
        s.merge_opt(p);
        if s.healing_target == 0 {
            // Absent prop decodes as 0; normalize to the invalid handle so
            // "beaming nobody" has one representation.
            s.healing_target = INVALID_HANDLE;
        }
        s
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        let mut p = WeaponPatch::default();
        Weapon::parse(packet, parser_state, &mut p);

        if let Some(released) = p.charge_released
            && released
            && self.charge_released != released
        {
            game.tick_events.push(Event::MedigunCharged(self.handle));
        }

        Box::new(p)
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<WeaponPatch>().unwrap();

        // Hack: mediguns get set to 0 charge the same tick that the med dies, but we want to keep
        // the value around a bit longer. So just delay resetting the medigun until the next update.
        if let Some(charge) = patch.charge {
            if charge > 0.0 {
                self.last_high_charge = charge;
            }
        } else if patch.reset_parity.is_some() {
            self.last_high_charge = 0.0;
        }

        self.merge_opt(*patch);
    }

    fn owner(&self) -> Option<u32> {
        Some(self.owner)
    }

    fn handle(&self) -> Option<u32> {
        Some(self.handle)
    }

    fn class(&self) -> EntityClass {
        EntityClass::Weapon
    }

    fn weapon(&self) -> Option<&Weapon> {
        Some(self)
    }
}
