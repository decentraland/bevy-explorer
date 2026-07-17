use bevy::reflect::Reflect;
use bevy_hanabi::{Attribute, ExprHandle, Modifier};

use crate::plugin::set_position_box_modifier::calc_func_id;

#[derive(Clone, Copy, Hash, Reflect)]
pub struct SetVelocityDirectionModifier {
    /// Direction that the particles will move towards.
    ///
    /// Expression type: `Vec3`
    pub direction: ExprHandle,
    /// The initial speed distribution of a particle when it spawns.
    ///
    /// Expression type: `f32`
    pub speed: ExprHandle,
}

impl SetVelocityDirectionModifier {
    fn eval(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut dyn bevy_hanabi::EvalContext,
    ) -> Result<String, bevy_hanabi::ExprError> {
        let func_id = calc_func_id(self);
        let func_name = format!("set_velocity_direction_{0:016X}", func_id);

        context.make_fn(
            &func_name,
            "particle: ptr<function, Particle>",
            module,
            &mut |m: &mut bevy_hanabi::Module,
                  ctx: &mut dyn bevy_hanabi::EvalContext|
             -> Result<String, bevy_hanabi::ExprError> {
                let direction = ctx.eval(m, self.direction)?;
                let speed = ctx.eval(m, self.speed)?;

                Ok(format!(
                    "    (*particle).{} = {} * {};",
                    Attribute::VELOCITY.name(),
                    direction,
                    speed,
                ))
            },
        )?;

        let code = format!("{}(&particle);\n", func_name);

        Ok(code)
    }
}

impl Modifier for SetVelocityDirectionModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        bevy_hanabi::ModifierContext::Init | bevy_hanabi::ModifierContext::Update
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
        let code = self.eval(module, context)?;
        context.main_code += &code;
        Ok(())
    }
}
