use bevy::reflect::Reflect;
use bevy_hanabi::{Attribute, ExprHandle, Modifier};

use crate::plugin::set_position_box_modifier::calc_func_id;

/// A modifier to set the velocity of particles in the direction of
/// the normal of a circle, with some spread proporcional to the distance
/// to the center.
///
/// # Attributes
///
/// This modifier requires the following particle attributes:
/// - [`Attribute::POSITION`]
/// - [`Attribute::VELOCITY`]
#[derive(Clone, Copy, Hash, Reflect)]
pub struct SetVelocitySpreadModifier {
    /// The circle center, relative to the emitter position.
    ///
    /// Expression type: `Vec3`
    pub center: ExprHandle,
    /// The radius of the circle.
    ///
    /// Expression type: `f32`
    pub radius: ExprHandle,
    /// The circle axis, which is the normalized normal of the circle's plane.
    ///
    /// Expression type: `Vec3`
    pub axis: ExprHandle,
    /// Spread of the particles, in radias.
    ///
    /// Expression type: `f32`
    pub spread: ExprHandle,
    /// The initial speed distribution of a particle when it spawns.
    ///
    /// Expression type: `f32`
    pub speed: ExprHandle,
}

impl SetVelocitySpreadModifier {
    fn eval(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut dyn bevy_hanabi::EvalContext,
    ) -> Result<String, bevy_hanabi::ExprError> {
        let func_id = calc_func_id(self);
        let func_name = format!("set_velocity_spread_{0:016X}", func_id);

        context.make_fn(
            &func_name,
            "transform: mat4x4<f32>, particle: ptr<function, Particle>",
            module,
            &mut |m: &mut bevy_hanabi::Module,
                  ctx: &mut dyn bevy_hanabi::EvalContext|
             -> Result<String, bevy_hanabi::ExprError> {
                let center = ctx.eval(m, self.center)?;
                let axis = ctx.eval(m, self.axis)?;
                let radius = ctx.eval(m, self.radius)?;
                let spread = ctx.eval(m, self.spread)?;
                let speed = ctx.eval(m, self.speed)?;

                Ok(format!(
                    r##"    let axis = {axis};
    let ab = axis;
    let ap = (*particle).{0} - axis - {center};
    let t = dot(ap, ab) / dot(ab, ab);
    let closest_point = axis + t * ab;
    let distance_to_axis = distance((*particle).{0}, closest_point);

    if distance_to_axis > 0 {{
        let axis_of_rotation = normalize(cross(axis, (*particle).{0}));
        let factor = clamp(distance_to_axis / max({radius}, 0.00000011920929f), 0., 1.);
        let theta = {spread} * factor;
        // https://en.wikipedia.org/wiki/Rodrigues%27_rotation_formula
        let s1 = axis * cos(theta);
        let s2 = cross(axis_of_rotation, axis) * sin(theta);
        let s3 = axis_of_rotation * dot(axis_of_rotation, axis) * (1 - cos(theta));
        (*particle).{1} = s1 + s2 + s3;
    }} else {{
        (*particle).{1} = axis * {speed};
    }}
"##,
                    Attribute::POSITION.name(),
                    Attribute::VELOCITY.name(),
                ))
            },
        )?;

        let code = format!("{}(transform, &particle);\n", func_name);

        Ok(code)
    }
}

impl Modifier for SetVelocitySpreadModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        bevy_hanabi::ModifierContext::Init | bevy_hanabi::ModifierContext::Update
    }

    fn attributes(&self) -> &[Attribute] {
        &[Attribute::POSITION, Attribute::VELOCITY]
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
