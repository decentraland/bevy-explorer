use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::reflect::Reflect;
use bevy_hanabi::{ExprHandle, Modifier};

#[derive(Clone, Copy, Hash, Reflect)]
pub struct SetPositionBoxModifier {
    pub scale: ExprHandle,
    pub dimension: bevy_hanabi::ShapeDimension,
}

impl SetPositionBoxModifier {
    fn eval(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut dyn bevy_hanabi::EvalContext,
    ) -> Result<String, bevy_hanabi::ExprError> {
        let func_id = calc_func_id(self);
        let func_name = format!("set_position_box_{0:016X}", func_id);

        context.make_fn(
            &func_name,
            "particle: ptr<function, Particle>",
            module,
            &mut |m: &mut bevy_hanabi::Module,
                  ctx: &mut dyn bevy_hanabi::EvalContext|
             -> Result<String, bevy_hanabi::ExprError> {
                let scale = ctx.eval(m, self.scale)?;

                let code = match self.dimension {
                    bevy_hanabi::ShapeDimension::Surface => {
                        format!(
                            r#"    let scale = {};

    let face = frand();
    let rand1 = frand() - 0.5;
    let rand2 = frand() - 0.5;
    let fixed = 0.5;

    var x: f32;
    var y: f32;
    var z: f32;

    if face < (1. / 6.) {{
        x = rand1 * scale.x;
        y = fixed * scale.y;
        z = rand2 * scale.z;
    }} else if face < (2. / 6.) {{
        x = rand1 * scale.x;
        y = -fixed * scale.y;
        z = rand2 * scale.z;
    }} else if face < (3. / 6.) {{
        x = fixed * scale.x;
        y = rand1 * scale.y;
        z = rand2 * scale.z;
    }} else if face < (4. / 6.) {{
        x = -fixed * scale.x;
        y = rand1 * scale.y;
        z = rand2 * scale.z;
    }} else if face < (5. / 6.) {{
        x = rand1 * scale.x;
        y = rand2 * scale.y;
        z = fixed * scale.z;
    }} else {{
        x = rand1 * scale.x;
        y = rand2 * scale.y;
        z = -fixed * scale.z;
    }}
    (*particle).{} = vec3(x, y, z);
"#,
                            scale,
                            bevy_hanabi::Attribute::POSITION.name()
                        )
                    }
                    bevy_hanabi::ShapeDimension::Volume => format!(
                        r#"    let scale = {};
    let mult = vec3(frand(), frand(), frand()) - 0.5;
    (*particle).{} = scale * mult;
"#,
                        scale,
                        bevy_hanabi::Attribute::POSITION.name()
                    ),
                };

                Ok(code)
            },
        )?;

        let code = format!("{}(&particle);\n", func_name);

        Ok(code)
    }
}

impl Modifier for SetPositionBoxModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        bevy_hanabi::ModifierContext::Init | bevy_hanabi::ModifierContext::Update
    }

    fn attributes(&self) -> &[bevy_hanabi::Attribute] {
        &[bevy_hanabi::Attribute::POSITION]
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

/// Calculate a function ID by hashing the given value representative of the
/// function.
pub(crate) fn calc_func_id<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
