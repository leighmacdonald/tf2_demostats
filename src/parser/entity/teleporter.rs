use crate::{
    convert_vec,
    parser::{
        entity::{Entity, EntityClass, SENTRY_BOX},
        props::*,
        summarizer::MatchAnalyzerView,
    },
    Vec3,
};
use parry3d::shape::SharedShape;
use std::any::Any;
use tf_demo_parser::{
    demo::{
        message::packetentities::{EntityId, PacketEntity},
        sendprop::SendPropValue,
    },
    ParserState,
};
use tracing::error;

#[optfield::optfield(TeleporterPatch, merge_fn, attrs)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Teleporter {
    pub origin: Vec3,
    pub owner: u32, // handle id
    pub owner_entity: EntityId,
    pub level: u32,
}

impl Teleporter {
    fn parse(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> TeleporterPatch {
        let mut patch = TeleporterPatch::default();

        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (ORIGIN, &SendPropValue::Vector(o)) => patch.origin = Some(convert_vec(o)),
                (BUILDER, &SendPropValue::Integer(b)) => {
                    let h = b as u32;
                    patch.owner = Some(h);
                    if let Some(eid) = game.entity_handles.get(&h) {
                        patch.owner_entity = Some(*eid);
                    }
                }
                (UPGRADE_LEVEL, &SendPropValue::Integer(l)) => patch.level = Some(l as u32),
                _ => {}
            }
        }
        patch
    }
}

impl Entity for Teleporter {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Self {
        let patch = Teleporter::parse(packet, parser_state, game);

        if let Some(owner) = patch.owner {
            game.handle_object_built(&owner);
        }

        Self {
            origin: patch.origin.unwrap_or_else(|| {
                error!("No origin for Teleporter gun! {packet:?}");
                Vec3::default()
            }),
            owner: patch.owner.unwrap_or_else(|| {
                error!("No owner for Teleporter gun! {packet:?}");
                0
            }),
            owner_entity: patch.owner_entity.unwrap_or_else(|| {
                error!("No owner entity for Teleporter gun! {packet:?}");
                EntityId::default()
            }),
            level: patch.level.unwrap_or_else(|| {
                error!("No level for Teleporter gun! {packet:?}");
                0
            }),
        }
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        Box::new(Teleporter::parse(packet, parser_state, game))
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<TeleporterPatch>().unwrap();
        self.merge_opt(*patch);
    }

    fn shape(&self) -> Option<SharedShape> {
        Some(SENTRY_BOX.clone())
    }
    fn origin(&self) -> Option<Vec3> {
        Some(self.origin)
    }

    fn owner(&self) -> Option<u32> {
        Some(self.owner)
    }

    fn class(&self) -> EntityClass {
        EntityClass::Teleporter
    }
}
