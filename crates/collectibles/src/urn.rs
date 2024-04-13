use bevy::log::warn;
use std::marker::PhantomData;

use itertools::Itertools;

use crate::CollectibleType;

#[derive(Debug, PartialEq, Eq)]
pub struct CollectibleUrnErr {
    msg: &'static str,
    value: String,
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone)]
pub struct CollectibleUrn<T: CollectibleType> {
    urn: String,
    _p: PhantomData<fn() -> T>,
}

impl<T: CollectibleType> std::fmt::Display for CollectibleUrn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.urn.fmt(f)
    }
}

impl<T: CollectibleType> TryFrom<&str> for CollectibleUrn<T> {
    type Error = CollectibleUrnErr;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_new_and_token(value)
            .map(|(urn, _)| urn)
            .map_err(|e| {
                warn!("invalid collectible urn: {}, {}", e.msg, e.value);
                e
            })
    }
}

impl<T: CollectibleType> From<CollectibleUrn<T>> for String {
    fn from(value: CollectibleUrn<T>) -> Self {
        value.urn
    }
}

impl<T: CollectibleType> AsRef<CollectibleUrn<T>> for CollectibleUrn<T> {
    fn as_ref(&self) -> &CollectibleUrn<T> {
        self
    }
}

impl<T: CollectibleType> CollectibleUrn<T> {
    fn try_new_and_token(value: &str) -> Result<(Self, Option<String>), CollectibleUrnErr> {
        let mut urn = value.to_lowercase();
        let count = urn.chars().filter(|c| *c == ':').count();
        if count == 0 {
            let Some(base) = T::base_collection() else {
                return Err(CollectibleUrnErr {
                    msg: "single segment urn with no base",
                    value: value.to_owned(),
                });
            };

            urn = format!("{}:{}", base, urn);
        }

        let parts: Vec<_> = urn.split(':').collect();

        let Some(collection) = parts.get(3) else {
            return Err(CollectibleUrnErr {
                msg: "no collection on urn",
                value: value.to_owned(),
            });
        };

        let collection_segments = match *collection {
            "base-avatars" | "base-emotes" | "scene-emote" => 4,
            "collections-v1" | "collections-v2" => 5,
            "collections-thirdparty" => 6,
            _ => {
                return Err(CollectibleUrnErr {
                    msg: "unrecognised collection type",
                    value: value.to_owned(),
                })
            }
        };

        let mut iter = parts.into_iter();
        let urn = iter.by_ref().take(collection_segments + 1).join(":");

        let token = iter.join(":").to_owned();
        let token = if token.is_empty() { None } else { Some(token) };

        Ok((
            Self {
                urn,
                _p: PhantomData,
            },
            token,
        ))
    }

    pub fn new(val: &str) -> Result<Self, CollectibleUrnErr> {
        Self::try_from(val)
    }

    pub fn chain(&self) -> Option<&str> {
        self.urn.split(':').nth(2)
    }

    pub fn is_offchain(&self) -> bool {
        self.chain() == Some("off-chain")
    }

    pub fn collection_type(&self) -> Option<&str> {
        self.urn.split(':').nth(3)
    }

    pub fn collection(&self) -> Option<&str> {
        match self.collection_type()? {
            "collections-thirdparty" => Some(self.skip_take(4, 2)),
            _ => Some(self.skip_take(4, 1)),
        }
    }

    pub fn skip_take(&self, skip: usize, take: usize) -> &str {
        let mut parts = 0;
        let mut iter = self.urn.chars();
        let mut start = 0;
        let mut end = 0;

        while parts < skip + take {
            let Some(next) = iter.next() else {
                return &self.urn;
            };

            if parts < skip {
                start += next.len_utf8();
            }
            end += next.len_utf8();
            if next == ':' {
                parts += 1;
            }
        }
        &self.urn[start..end]
    }

    pub fn as_str(&self) -> &str {
        self.urn.as_str()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone)]
pub struct CollectibleInstance<T: CollectibleType> {
    base: CollectibleUrn<T>,
    token: Option<String>,
}

impl<T: CollectibleType> TryFrom<&str> for CollectibleInstance<T> {
    type Error = CollectibleUrnErr;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (base, token) = CollectibleUrn::<T>::try_new_and_token(value)?;
        Ok(Self { base, token })
    }
}

impl<T: CollectibleType> CollectibleInstance<T> {
    pub fn new(urn: &str) -> Result<Self, CollectibleUrnErr> {
        Self::try_from(urn)
    }

    pub fn new_with_token(urn: &str, token: Option<String>) -> Result<Self, CollectibleUrnErr> {
        Ok(Self {
            token,
            ..Self::new(urn)?
        })
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
