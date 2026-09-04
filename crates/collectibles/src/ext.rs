use bevy::{
    animation::AnimationClip,
    asset::Handle,
    platform::{collections::HashMap, hash::FixedHasher},
};

pub trait AvatarEmotesExt {
    fn any_avatar_emote(&self) -> bool;
    fn find_avatar_emote(&self) -> Option<(&str, &Handle<AnimationClip>)>;
}

impl AvatarEmotesExt for HashMap<Box<str>, Handle<AnimationClip>, FixedHasher> {
    fn any_avatar_emote(&self) -> bool {
        self.keys().any(|key| avatar_emote_name(key))
    }

    fn find_avatar_emote(&self) -> Option<(&str, &Handle<AnimationClip>)> {
        self.iter()
            .find(|(key, _)| avatar_emote_name(key))
            .map(|(key, value)| (key.as_ref(), value))
    }
}

fn avatar_emote_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("_avatar")
}
