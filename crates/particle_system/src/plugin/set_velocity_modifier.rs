use bevy::reflect::Reflect;
use bevy_hanabi::{
    Modifier, SetPositionCircleModifier, SetPositionCone3dModifier, SetPositionSphereModifier,
    SetVelocityCircleModifier, SetVelocitySphereModifier, SetVelocityTangentModifier,
};

macro_rules! dispatch {
    ($a:expr, $method:ident) => {
        match $a {
            Self::Sphere(sphere) => sphere.$method(),
            Self::Circle(circle) => circle.$method(),
            Self::Tangent(cone3d) => cone3d.$method(),
        }
    };
    ($a:expr, $method:ident, $arg1:expr, $arg2:expr) => {
        match $a {
            Self::Circle(circle) => circle.$method($arg1, $arg2),
            Self::Sphere(sphere) => sphere.$method($arg1, $arg2),
            Self::Tangent(cone3d) => cone3d.$method($arg1, $arg2),
        }
    };
}

#[derive(Reflect)]
pub enum SetVelocityModifier {
    Circle(SetVelocityCircleModifier),
    Sphere(SetVelocitySphereModifier),
    Tangent(SetVelocityTangentModifier),
}

impl Modifier for SetVelocityModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        dispatch!(self, context)
    }

    fn attributes(&self) -> &[bevy_hanabi::Attribute] {
        dispatch!(self, attributes)
    }

    fn boxed_clone(&self) -> bevy_hanabi::BoxedModifier {
        dispatch!(self, boxed_clone)
    }

    fn apply(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut bevy_hanabi::ShaderWriter,
    ) -> Result<(), bevy_hanabi::ExprError> {
        dispatch!(self, apply, module, context)
    }
}
