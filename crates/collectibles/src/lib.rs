pub mod base_wearables;
pub mod emotes;
pub mod wearables;

use bevy::prelude::Plugin;
use std::marker::PhantomData;

pub use emotes::*;
use itertools::Itertools;
use wearables::WearablePlugin;

pub struct CollectiblesPlugin;

impl Plugin for CollectiblesPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((EmotesPlugin, WearablePlugin));
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone)]
pub struct CollectibleUrn<T> {
    urn: String,
    _p: PhantomData<fn() -> T>,
}

impl<T> std::fmt::Display for CollectibleUrn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.urn.fmt(f)
    }
}

impl<T> From<String> for CollectibleUrn<T> {
    fn from(urn: String) -> Self {
        let urn = urn.to_lowercase().split(':').take(6).join(":");
        Self {
            urn,
            _p: PhantomData,
        }
    }
}

impl<T> From<CollectibleUrn<T>> for String {
    fn from(value: CollectibleUrn<T>) -> Self {
        value.urn
    }
}

impl<T> AsRef<CollectibleUrn<T>> for CollectibleUrn<T> {
    fn as_ref(&self) -> &CollectibleUrn<T> {
        self
    }
}

impl<T> CollectibleUrn<T> {
    pub fn new(val: impl Into<String>) -> Self {
        Self::from(val.into())
    }

    pub fn collection(&self) -> String {
        self.urn.split(':').take(6).join(":")
    }

    pub fn as_str(&self) -> &str {
        self.urn.as_str()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone)]
pub struct CollectibleInstance<T> {
    base: CollectibleUrn<T>,
    token: Option<String>,
}

impl<T> From<String> for CollectibleInstance<T> {
    fn from(value: String) -> Self {
        let value = value.to_lowercase();
        let mut parts = value.split(':');
        let urn = parts.by_ref().take(6).join(":");
        let token: String = parts.collect();
        Self {
            base: CollectibleUrn::<T> {
                urn,
                _p: PhantomData,
            },
            token: (!token.is_empty()).then_some(token),
        }
    }
}

impl<T> CollectibleInstance<T> {
    pub fn new(urn: impl Into<String>) -> Self {
        Self::from(urn.into())
    }

    pub fn new_with_token(urn: impl Into<String>, token: Option<String>) -> Self {
        Self {
            token,
            ..Self::new(urn)
        }
    }

    pub fn base(&self) -> &CollectibleUrn<T> {
        &self.base
    }

    pub fn instance_urn(&self) -> String {
        match &self.token {
            Some(t) => format!("{}:{}", self.base, t),
            None => self.base.to_string(),
        }
    }
}
