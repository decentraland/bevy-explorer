use bevy::reflect::Reflect;
use bevy_hanabi::{Attribute, EvalContext, ExprHandle, Modifier, ModifierContext};

/// Dampens [`Attribute::Velocity`] of the particle if it is above
/// [`SpeedDampenModifier::max_speed`].
#[derive(Clone, Copy, Reflect)]
pub struct SpeedDampenModifier {
    pub max_speed: ExprHandle,
    pub dampen: ExprHandle,
}

impl Modifier for SpeedDampenModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> &[Attribute] {
        &[Attribute::VELOCITY]
    }

    fn boxed_clone(&self) -> bevy_hanabi::BoxedModifier {
        Box::new(*self)
    }

    fn apply(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut bevy_hanabi::ShaderWriter,
    ) -> Result<(), bevy_hanabi::ExprError> {
        let velocity_attr = module.attr(Attribute::VELOCITY);
        let velocity = context.eval(module, velocity_attr)?;
        let max_speed = context.eval(module, self.max_speed)?;
        let dampen = context.eval(module, self.dampen)?;
        context.main_code += &format!(
            r#"    {{
        let velocity_length = length({velocity});
        let normalized_velocity = normalize({velocity});
        let excess = max(velocity_length - {max_speed}, 0.);
        let dampened_magnitude = mix(velocity_length, velocity_length - excess, {dampen});
        {velocity} = normalized_velocity * dampened_magnitude;
    }}
"#
        );
        Ok(())
    }
}
