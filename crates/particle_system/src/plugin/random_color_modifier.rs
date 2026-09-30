use bevy::reflect::Reflect;
use bevy_hanabi::{
    Attribute, BuiltInExpr, BuiltInOperator, EvalContext, Expr, ExprHandle, Modifier,
    ModifierContext, ScalarType,
};

#[derive(Clone, Copy, Reflect)]
pub struct RandomColorModifier {
    pub start: ExprHandle,
    pub end: ExprHandle,
}

impl Modifier for RandomColorModifier {
    fn context(&self) -> bevy_hanabi::ModifierContext {
        ModifierContext::Init
    }

    fn attributes(&self) -> &[bevy_hanabi::Attribute] {
        &[Attribute::COLOR]
    }

    fn boxed_clone(&self) -> bevy_hanabi::BoxedModifier {
        Box::new(*self)
    }

    fn apply(
        &self,
        module: &mut bevy_hanabi::Module,
        context: &mut bevy_hanabi::ShaderWriter,
    ) -> Result<(), bevy_hanabi::ExprError> {
        let color_expr = module.attr(Attribute::COLOR);

        let color = context.eval(module, color_expr)?;
        let start = context.eval(module, self.start)?;
        let end = context.eval(module, self.end)?;
        let rand = Expr::BuiltIn(BuiltInExpr::new(BuiltInOperator::Rand(
            ScalarType::Float.into(),
        )))
        .eval(module, context)?;

        context.main_code += &format!(
            r#"    {{
        let color_lerp = mix({start}, {end}, {rand});
        let r = u32(clamp(color_lerp.r * 255., 0., 255.));
        let g = u32(clamp(color_lerp.g * 255., 0., 255.));
        let b = u32(clamp(color_lerp.b * 255., 0., 255.));
        let a = u32(clamp(color_lerp.a * 255., 0., 255.));
        {color} = a << 24 | b << 16 | g << 8 | r;
    }}
"#
        );
        Ok(())
    }
}
