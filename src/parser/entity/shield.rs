use crate::parser::{
    entity::{Entity, EntityClass},
    props::*,
    summarizer::MatchAnalyzerView,
};
use std::any::Any;
use tf_demo_parser::{
    demo::{
        message::packetentities::PacketEntity, packet::datatable::ClassId, sendprop::SendPropValue,
    },
    ParserState,
};

#[optfield::optfield(ShieldPatch, merge_fn, attrs)]
#[derive(Debug, PartialEq, Default)]
pub struct Shield {
    pub class_name: String,
    pub owner: u32,
    pub handle: u32,
    pub schema_id: u32,
}

impl Shield {
    fn parse(packet: &PacketEntity, parser_state: &ParserState, patch: &mut ShieldPatch) {
        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (SELF_HANDLE, &SendPropValue::Integer(h)) => {
                    patch.handle = Some(h as u32);
                }
                (OWNER, &SendPropValue::Integer(h)) => {
                    patch.owner = Some(h as u32);
                }
                (ITEM_DEFINITION, &SendPropValue::Integer(id)) => {
                    patch.schema_id = Some(id as u32);
                }
                _ => {}
            }
        }
    }
}

impl Entity for Shield {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        _game: &mut MatchAnalyzerView,
    ) -> Self {
        let class_name = parser_state
            .server_classes
            .get(<ClassId as Into<usize>>::into(packet.server_class))
            .map(|s| s.name.to_string())
            .unwrap_or("UNKNOWN_PROJECTILE".to_string());

        let mut p = ShieldPatch::default();
        Shield::parse(packet, parser_state, &mut p);

        let mut s = Self {
            class_name,
            ..Default::default()
        };
        s.merge_opt(p);
        s
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        _game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        let mut p = Box::new(ShieldPatch::default());
        Shield::parse(packet, parser_state, &mut p);
        p
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<ShieldPatch>().unwrap();
        self.merge_opt(*patch);
    }

    fn owner(&self) -> Option<u32> {
        Some(self.owner)
    }

    fn handle(&self) -> Option<u32> {
        Some(self.handle)
    }

    fn class(&self) -> EntityClass {
        EntityClass::Shield
    }

    fn shield(&self) -> Option<&Shield> {
        Some(self)
    }
}
