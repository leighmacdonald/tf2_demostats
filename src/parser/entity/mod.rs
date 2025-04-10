use crate::{parser::summarizer::MatchAnalyzerView, Vec3};
use nalgebra::vector;
use optfield::optfield;
use parry3d::shape::{Cuboid, SharedShape};
use std::any::Any;
use tf_demo_parser::{demo::message::packetentities::PacketEntity, ParserState};

pub mod sentry;
pub use sentry::*;

pub mod dispenser;
pub use dispenser::*;

pub mod teleporter;
pub use teleporter::*;

pub mod projectile;
pub use projectile::*;

pub mod weapon;
pub use weapon::*;

pub mod player;
pub use player::*;

pub mod shield;
pub use shield::*;

#[derive(Debug, PartialEq)]
pub enum EntityClass {
    Projectile,
    Sentry,
    Dispenser,
    Teleporter,
    Weapon,
    Shield,
    Player,
    PlayerResource,
    Unknown,
}

// TODO: Use a real ECS like bevy_ecs? Or at least switch back to an enum with variants.

pub trait Entity: std::fmt::Debug {
    fn new(packet: &PacketEntity, parser_state: &ParserState, game: &mut MatchAnalyzerView) -> Self
    where
        Self: Sized;

    fn parse_preserve(
        &self,
        packet: &PacketEntity,
        parser_state: &ParserState,
        game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any>;

    fn apply_preserve(&mut self, patch: Box<dyn Any>);

    // Entities are stored as Box<dyn T> for polymorphism, but that means
    // the consuming methods delete()/leave() cannot be invoked until
    // https://github.com/rust-lang/rust/issues/48055 is stabilizied.
    //
    // This Boxed trait is a workaround from
    // https://users.rust-lang.org/t/call-consuming-method-for-dyn-trait-object/69596/7
    fn delete(self: Box<Self>, _game: &mut MatchAnalyzerView) {}
    fn leave(self: Box<Self>, game: &mut MatchAnalyzerView) {
        self.delete(game);
    }

    // Optional collision
    fn shape(&self) -> Option<SharedShape> {
        None
    }
    fn origin(&self) -> Option<Vec3> {
        None
    }

    fn owner(&self) -> Option<u32> {
        None
    }

    fn handle(&self) -> Option<u32> {
        None
    }

    fn class(&self) -> EntityClass;

    // Hacky downcasts
    fn player(&self) -> Option<&Player> {
        None
    }
    fn weapon(&self) -> Option<&Weapon> {
        None
    }
    fn projectile(&self) -> Option<&Projectile> {
        None
    }
    fn sentry(&self) -> Option<&Sentry> {
        None
    }
    fn shield(&self) -> Option<&Shield> {
        None
    }
}

#[optfield(UnknownPatch, merge_fn, attrs)]
#[derive(Debug, Default)]
pub struct Unknown {}

impl Entity for Unknown {
    fn new(
        _packet: &PacketEntity,
        _parser_state: &ParserState,
        _game: &mut MatchAnalyzerView,
    ) -> Self
    where
        Self: Sized,
    {
        Unknown {}
    }

    fn parse_preserve(
        &self,
        _packet: &PacketEntity,
        _parser_state: &ParserState,
        _game: &mut MatchAnalyzerView,
    ) -> Box<dyn Any> {
        Box::new(UnknownPatch::default())
    }

    fn apply_preserve(&mut self, patch: Box<dyn Any>) {
        let patch = patch.downcast::<UnknownPatch>().unwrap();
        self.merge_opt(*patch);
    }

    fn class(&self) -> EntityClass {
        EntityClass::Unknown
    }
}

lazy_static::lazy_static! {
    // TODO: real valuess, switch by sentry level
    static ref SENTRY_BOX: SharedShape = SharedShape::new(Cuboid::new(vector![49.0, 49.0, 83.0]));

    // TODO: real valuess, switch by projectile type
    static ref PROJECTILE_BOX: SharedShape = SharedShape::new(Cuboid::new(vector![10.0, 10.0, 10.0]));
}
