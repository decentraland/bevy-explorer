use bevy::reflect::Reflect;
use bevy_hanabi::{Attribute, EvalContext, Modifier, ModifierContext};

#[derive(Clone, Copy, Reflect)]
pub struct UpdateSpriteIndexModifier {
    pub frame_count: u32,
    pub frames_per_second: f32,
}

impl Modifier for UpdateSpriteIndexModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> &[bevy_hanabi::Attribute] {
        &[Attribute::AGE, Attribute::SPRITE_INDEX]
    }

    fn boxed_clone(&self) -> bevy_hanabi::BoxedModifier {
        Box::new(*self)
    }

    fn apply(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut bevy_hanabi::ShaderWriter,
    ) -> Result<(), bevy_hanabi::ExprError> {
        let age_attr = module.attr(Attribute::AGE);
        let sprite_index_attr = module.attr(Attribute::SPRITE_INDEX);
        let frame_count_expr = module.lit(self.frame_count);
        let frames_per_second_expr = module.lit(self.frames_per_second);

        let age = context.eval(module, age_attr)?;
        let sprite_index = context.eval(module, sprite_index_attr)?;
        let frame_count = context.eval(module, frame_count_expr)?;
        let frames_per_second = context.eval(module, frames_per_second_expr)?;

        context.main_code += &format!(
            r#"
    {{
        let frame_count: u32 = {frame_count};
        let frames_per_second = {frames_per_second};
        let sprite_index = u32({age} * frames_per_second) % frame_count;
        {sprite_index} = i32(sprite_index);
    }}        
"#
        );
        Ok(())
    }
}
