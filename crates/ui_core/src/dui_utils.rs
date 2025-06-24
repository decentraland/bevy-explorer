use anyhow::anyhow;
use bevy::{
    asset::{Asset, Handle},
    color::Color,
    ui::{UiRect, Val},
};
use bevy_dui::{DuiContext, DuiProps};
use std::str::FromStr;

pub trait DuiFromStr {
    fn from_str(ctx: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

macro_rules! impl_dui_str {
    ($T:ty) => {
        impl<'a> DuiFromStr for $T {
            fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error> {
                <Self as FromStr>::from_str(value).map_err(|_| {
                    anyhow!(
                        "failed to convert `{value}` to {}",
                        std::any::type_name::<$T>()
                    )
                })
            }
        }
    };
}

impl_dui_str!(bool);
impl_dui_str!(u32);
impl_dui_str!(f32);
impl_dui_str!(usize);
impl_dui_str!(isize);
impl_dui_str!(i32);

impl DuiFromStr for Val {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .val()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Val"))
    }
}

impl DuiFromStr for UiRect {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .rect()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Rect"))
    }
}

impl DuiFromStr for Color {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .color()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Color"))
    }
}

impl<T: Asset> DuiFromStr for Handle<T> {
    fn from_str(ctx: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        Ok(ctx.asset_server().load::<T>(value.to_owned()))
    }
}

pub trait PropsExt {
    fn take_as<T: DuiFromStr + 'static>(
        &mut self,
        ctx: &DuiContext,
        label: &str,
    ) -> Result<Option<T>, anyhow::Error>;
}

impl PropsExt for DuiProps {
    fn take_as<T: DuiFromStr + 'static>(
        &mut self,
        ctx: &DuiContext,
        label: &str,
    ) -> Result<Option<T>, anyhow::Error> {
        if let Ok(value) = self.take::<T>(label) {
            return Ok(value);
        }

        if let Ok(Some(value)) = self.take::<String>(label) {
            Ok(Some(<T as DuiFromStr>::from_str(ctx, &value)?))
        } else {
            Err(anyhow!("unrecognised type for key `{label}`"))
        }
    }
}
