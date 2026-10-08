use crate::{
    Vec3, convert_vec,
    parser::{
        entity::{Entity, EntityClass, SENTRY_BOX},
        props::{BUILDER, OBJECT_MAX_HEALTH, ORIGIN, UPGRADE_LEVEL},
        summarizer::{BuildingType, MatchAnalyzerView, Position},
    },
};
use parry3d::shape::SharedShape;
use std::any::Any;
use tf_demo_parser::{
    ParserState,
    demo::{
        message::packetentities::{EntityId, PacketEntity},
        sendprop::SendPropValue,
    },
};
use tracing::error;

#[optfield::optfield(SentryPatch, merge_fn, attrs)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sentry {
    pub origin: Vec3,
    pub owner: u32, // handle id
    pub owner_entity: EntityId,
    pub level: u32,
    pub is_mini: bool,
}

impl Sentry {
    fn parse(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> SentryPatch {
        let mut patch = SentryPatch::default();

        for prop in packet.props(parser_state) {
            match (prop.identifier, &prop.value) {
                (ORIGIN, &SendPropValue::Vector(o)) => patch.origin = Some(convert_vec(o)),
                (BUILDER, &SendPropValue::Integer(b)) => {
                    let h = u32::try_from(b).unwrap_or_default();
                    patch.owner = Some(h);
                    if let Some(eid) = game.entity_handles.get(&h) {
                        patch.owner_entity = Some(*eid);
                    }
                }
                (UPGRADE_LEVEL, &SendPropValue::Integer(l)) => {
                    patch.level = Some(u32::try_from(l).unwrap_or_default());
                }
                (OBJECT_MAX_HEALTH, &SendPropValue::Integer(l)) => {
                    patch.is_mini = Some(l == 100);
                }
                _ => {}
            }
        }
        patch
    }
}

impl Entity for Sentry {
    fn new(
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Self {
        let patch = Sentry::parse(packet, parser_state, game);

        if let Some(owner) = patch.owner {
            game.handle_object_built(&owner);
            let origin = patch.origin.unwrap_or_default();
            game.handle_building_built(
                &owner,
                BuildingType::Sentry,
                patch.level.unwrap_or_default(),
                patch.is_mini.unwrap_or_default(),
                Position {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
            );
        }

        Self {
            origin: patch.origin.unwrap_or_else(|| {
                error!("No origin for Sentry gun! {packet:?}");
                Vec3::default()
            }),
            owner: patch.owner.unwrap_or_else(|| {
                error!("No owner for Sentry gun! {packet:?}");
                0
            }),
            owner_entity: patch.owner_entity.unwrap_or_else(|| {
                error!("No owner entity for Sentry gun! {packet:?}");
                EntityId::default()
            }),
            level: patch.level.unwrap_or_else(|| {
                error!("No level for Sentry gun! {packet:?}");
                0
            }),
            is_mini: patch.is_mini.unwrap_or_else(|| {
                error!("No is_mini based on max hp for Sentry gun! {packet:?}");
                false
            }),
        }
    }

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        Box::new(Sentry::parse(packet, parser_state, game))
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<SentryPatch>().unwrap();
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
        EntityClass::Sentry
    }
    fn sentry(&self) -> Option<&Sentry> {
        Some(self)
    }
}
