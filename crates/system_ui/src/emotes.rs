use anyhow::anyhow;
use avatar::{
    avatar_texture::{BoothInstance, PhotoBooth},
    AvatarShape,
};
use bevy::{
    color::palettes::css,
    prelude::*,
    render::render_resource::Extent3d,
    tasks::{IoTaskPool, Task},
    platform::collections::{HashMap, HashSet},
};
use bevy_dui::{
    DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry, DuiWalker,
};
use collectibles::{
    base_wearables::default_bodyshape_instance,
    emotes::{Emote, EmoteInstance},
    wearables::WearableInstance,
    BaseEmotes, CollectibleData, CollectibleError, CollectibleManager,
};
use common::{
    structs::{PrimaryUser, SettingsTab, PROFILE_UI_RENDERLAYER},
    util::{TaskCompat, TaskExt},
};
use comms::profile::CurrentUserProfile;
use ipfs::IpfsAssetServer;
use serde::Deserialize;
use tween::SystemTween;
use ui_core::{
    button::{DuiButton, TabSelection},
    combo_box::ComboBox,
    interact_style::{InteractStyle, InteractStyles},
    text_entry::TextEntryValue,
    toggle::Toggled,
    ui_actions::{Click, DataChanged, Enabled, On, UiCaller},
};

use crate::profile::SettingsDialog;

pub struct EmoteSettingsPlugin;

impl Plugin for EmoteSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<GetOwnedEmotes>()
            .add_event::<SelectItem>()
            .add_systems(
                Update,
                (
                    set_emotes_content,
                    (
                        apply_deferred,
                        get_owned_emotes,
                        update_emotes_list,
                        apply_deferred,
                        update_emote_item,
                        update_selected_item,
                    )
                        .chain()
                        .run_if(|q: Query<&SettingsTab>| {
                            q.single().is_ok_and(|tab| tab == &SettingsTab::Emotes)
                        }),
                )
                    .chain(),
            );
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Newest,
    Oldest,
    Alphabetic,
    ReverseAlphabetic,
    Rarest,
    ReverseRarest,
    #[default]
    Equipped,
}

impl SortBy {
    const STRS: [&'static str; 7] = [
        "Newest",
        "Oldest",
        "Alphabetic",
        "Reverse Alphabetic",
        "Rarest",
        "Reverse Rarest",
        "Equipped First",
    ];

    fn strings() -> Vec<String> {
        Self::STRS.iter().cloned().map(ToOwned::to_owned).collect()
    }

    fn from(value: &str) -> Self {
        match Self::STRS.iter().position(|s| *s == value) {
            Some(0) => Self::Newest,
            Some(1) => Self::Oldest,
            Some(2) => Self::Alphabetic,
            Some(3) => Self::ReverseAlphabetic,
            Some(4) => Self::Rarest,
            Some(5) => Self::ReverseRarest,
            _ => Self::Equipped,
        }
    }

    fn to(&self) -> &'static str {
        match self {
            SortBy::Newest => Self::STRS[0],
            SortBy::Oldest => Self::STRS[1],
            SortBy::Alphabetic => Self::STRS[2],
            SortBy::ReverseAlphabetic => Self::STRS[3],
            SortBy::Rarest => Self::STRS[4],
            SortBy::ReverseRarest => Self::STRS[5],
            SortBy::Equipped => Self::STRS[6],
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IndividualData {
    transferred_at: String,
    token_id: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OwnedEmoteData {
    pub urn: String,
    pub name: String,
    pub category: String,
    pub rarity: String,
    pub individual_data: Vec<IndividualData>,
}

#[derive(Deserialize)]
pub struct OwnedEmoteServerResponse {
    elements: Vec<OwnedEmoteData>,
}

#[derive(Component, Clone)]
pub struct EmotesSettings {
    pub body_shape: WearableInstance,
    pub only_collectibles: bool,
    pub sort_by: SortBy,
    pub search_filter: Option<String>,
    pub current_emotes: HashMap<usize, (EmoteInstance, CollectibleData<Emote>)>,
    pub owned_emotes: Vec<OwnedEmoteData>,
    current_list: Vec<EmoteEntry>,
    pub selected_slot: usize,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn set_emotes_content(
    mut commands: Commands,
    dialog: Query<(
        Entity,
        Option<&BoothInstance>,
        Option<&AvatarShape>,
        Ref<SettingsDialog>,
    )>,
    mut q: Query<(
        Entity,
        &SettingsTab,
        Option<&mut EmotesSettings>,
        Has<SelectItem>,
    )>,
    dui: Res<DuiRegistry>,
    mut booth: PhotoBooth,
    player: Query<&AvatarShape, (Without<SettingsDialog>, With<PrimaryUser>)>,
    mut prev_tab: Local<Option<SettingsTab>>,
    ipfas: IpfsAssetServer,
    mut emote_loader: CollectibleManager<Emote>,
    mut e: EventWriter<GetOwnedEmotes>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab, emote_settings, has_select) in q.iter_mut() {
        let Ok((settings_entity, maybe_instance, _, dialog)) = dialog.single() else {
            return;
        };

        if *prev_tab == Some(*tab) && !dialog.is_changed() {
            continue;
        }

        if tab != &SettingsTab::Emotes {
            *prev_tab = Some(*tab);
            return;
        }

        debug!("redraw");

        commands.entity(ent).despawn_descendants();

        let instance = maybe_instance.cloned().unwrap_or_else(|| {
            let avatar = player.single().unwrap();
            let instance = booth.spawn_booth(
                PROFILE_UI_RENDERLAYER,
                avatar.clone(),
                Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: 1,
                },
                true,
            );
            commands
                .entity(settings_entity)
                .try_insert((instance.clone(), avatar.clone()));
            instance
        });

        let booth_camera = instance.camera;
        if let Ok(mut commands) = commands.get_entity(booth_camera) {
            commands.try_insert(SystemTween {
                target: Transform {
                    translation: Vec3::new(1.2844446, 1.1353853, -2.876228),
                    rotation: Quat::from_xyzw(0.0, 0.978031, 0.0, 0.20845993),
                    scale: Vec3::ONE,
                },
                time: 0.5,
            });
        };

        let new_settings;
        let emote_settings = match emote_settings {
            Some(mut settings) => {
                // reset cached data
                settings.current_list = Vec::default();
                settings.into_inner()
            }
            None => {
                let player_shape = &player.single().unwrap().0;
                let body_instance =
                    WearableInstance::new(player_shape.body_shape.as_ref().unwrap())
                        .unwrap_or_else(|_| default_bodyshape_instance());

                let mut all_loaded = true;

                new_settings = EmotesSettings {
                    body_shape: body_instance.clone(),
                    current_emotes: player_shape
                        .emotes
                        .iter()
                        .map(|w| EmoteInstance::new(w).ok())
                        .map(|maybe_instance| {
                            let instance = maybe_instance?;
                            match emote_loader.get_data(instance.base()) {
                                Ok(w) => Some((instance, w.clone())),
                                Err(CollectibleError::Loading) => {
                                    all_loaded = false;
                                    None
                                }
                                _ => None,
                            }
                        })
                        .enumerate()
                        .flat_map(|(slot, maybe_emote)| maybe_emote.map(|emote| (slot, emote)))
                        .collect(),
                    only_collectibles: Default::default(),
                    sort_by: Default::default(),
                    search_filter: Default::default(),
                    owned_emotes: Default::default(),
                    current_list: Default::default(),
                    selected_slot: Default::default(),
                };

                if !all_loaded {
                    debug!("bail loading all");
                    return;
                }

                commands.entity(ent).try_insert(new_settings.clone());
                &new_settings
            }
        };

        if !has_select {
            commands.entity(ent).try_insert(SelectItem(None));
        }

        let empty_img = ipfas
            .asset_server()
            .load::<Image>("images/backpack/empty.png");

        let slot_tabs: Vec<_> = (0usize..=9)
            .map(|slot| {
                let emote_img = emote_settings
                    .current_emotes
                    .get(&slot)
                    .map(|(_, data)| data.thumbnail.clone())
                    .unwrap_or_else(|| empty_img.clone());

                let content = commands
                    .spawn_template(
                        &dui,
                        "emote-slot",
                        DuiProps::new()
                            .with_prop("slot-id", format!("{slot}"))
                            .with_prop("emote-img", emote_img),
                    )
                    .unwrap()
                    .root;

                DuiButton {
                    styles: Some(InteractStyles {
                        active: Some(InteractStyle {
                            background: Some(css::ORANGE.into()),
                            border: Some(Color::BLACK),
                            ..Default::default()
                        }),
                        inactive: Some(InteractStyle {
                            background: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                            border: Some(Color::NONE),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: Some(content),
                    ..Default::default()
                }
            })
            .collect();

        let selected_slot = Some(emote_settings.selected_slot);

        let props = DuiProps::new()
            .with_prop("booth-instance", instance)
            .with_prop(
                "only-collectibles",
                On::<DataChanged>::new(only_collectibles),
            )
            .with_prop("only-collectibles-set", emote_settings.only_collectibles)
            .with_prop("slot-tabs", slot_tabs)
            .with_prop("selected-slot", selected_slot)
            .with_prop(
                "slot-changed",
                On::<DataChanged>::new(
                    move |caller: Res<UiCaller>,
                          tab: Query<&TabSelection>,
                          mut settings: Query<&mut EmotesSettings>,
                          booth_instance: Query<&BoothInstance, With<SettingsDialog>>,
                          mut booth: PhotoBooth| {
                        let Ok(selection) = tab.get(caller.0) else {
                            warn!("failed to get tab");
                            return;
                        };

                        let Ok(mut settings) = settings.single_mut() else {
                            warn!("failed to get settings");
                            return;
                        };

                        settings.selected_slot = selection.selected.unwrap();

                        if let Some(selected_emote) =
                            settings.current_emotes.get(&settings.selected_slot)
                        {
                            if let Ok(instance) = booth_instance.single() {
                                debug!("playing");
                                booth.play_emote(instance, selected_emote.0.base().clone());
                            } else {
                                debug!("no instance");
                            }
                        } else {
                            debug!("no emote");
                        }
                    },
                ),
            )
            .with_prop("sort-by", SortBy::strings())
            .with_prop(
                "initial-sort-by",
                SortBy::strings()
                    .iter()
                    .position(|sb| sb == emote_settings.sort_by.to())
                    .unwrap() as isize,
            )
            .with_prop(
                "sort-by-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&ComboBox>,
                     mut settings: Query<&mut EmotesSettings>| {
                        let Some(value) = q.get(caller.0).ok().and_then(|cb| cb.selected()) else {
                            warn!("no value from sort combo?");
                            return;
                        };
                        settings.single_mut().sort_by = SortBy::from(value.as_str());
                    },
                ),
            )
            .with_prop(
                "initial-filter",
                emote_settings.search_filter.clone().unwrap_or_default(),
            )
            .with_prop(
                "filter-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&TextEntryValue>,
                     mut settings: Query<&mut EmotesSettings>| {
                        let Ok(value) = q.get(caller.0).map(|te| te.0.clone()) else {
                            warn!("no value from text entry?");
                            return;
                        };
                        if value.is_empty() {
                            settings.single_mut().search_filter = None;
                        } else {
                            settings.single_mut().search_filter = Some(value);
                        }
                    },
                ),
            );

        let components = commands
            .entity(ent)
            .apply_template(&dui, "emotes", props)
            .unwrap();
        commands.entity(ent).try_insert(components);

        e.send_default();
        *prev_tab = Some(*tab);
    }
}

fn only_collectibles(
    caller: Res<UiCaller>,
    toggle: Query<&Toggled>,
    mut q: Query<&mut EmotesSettings>,
) {
    let Ok(toggle) = toggle.get(**caller) else {
        warn!("toggle access failed");
        return;
    };

    let Ok(mut settings) = q.single_mut() else {
        warn!("settings access failed");
        return;
    };

    settings.only_collectibles = toggle.0;
}

#[derive(Event, Default)]
struct GetOwnedEmotes;

fn get_owned_emotes(
    mut e: EventReader<GetOwnedEmotes>,
    mut task: Local<Option<Task<Result<OwnedEmoteServerResponse, anyhow::Error>>>>,
    mut q: Query<&mut EmotesSettings>,
    ipfas: IpfsAssetServer,
    current_profile: Res<CurrentUserProfile>,
) {
    let ev = e.read().last().is_some();

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(emote_data)) => {
                if let Ok(mut settings) = q.single_mut() {
                    debug!("emote task ok");
                    settings.owned_emotes = emote_data.elements;
                }
            }
            Some(Err(e)) => {
                warn!("owned emote task failed: {e}");
            }
            None => {
                *task = Some(t);
            }
        }
    } else if ev {
        let Some(endpoint) = ipfas.ipfs().lambda_endpoint() else {
            warn!("not connected");
            return;
        };
        let Some(address) = current_profile
            .profile
            .as_ref()
            .map(|p| p.content.eth_address.clone())
        else {
            warn!("no profile, not loading custom emotes");
            return;
        };

        let client = ipfas.ipfs().client();
        *task = Some(IoTaskPool::get().spawn_compat(async move {
            let response = client
                .get(format!("{endpoint}/users/{address}/emotes"))
                .send()
                .await
                .map_err(|e| anyhow!(e))?;
            response
                .json::<OwnedEmoteServerResponse>()
                .await
                .map_err(|e| anyhow!(e))
        }));
    }
}

#[derive(Component, Clone, Debug)]
struct EmoteEntry {
    pub instance: EmoteInstance,
    pub name: String,
    pub rarity: Rarity,
    pub individual_data: Vec<IndividualData>,
}

impl PartialEq for EmoteEntry {
    fn eq(&self, other: &Self) -> bool {
        self.instance.eq(&other.instance)
    }
}

impl EmoteEntry {
    fn base(data: &CollectibleData<Emote>) -> Option<Self> {
        Some(Self {
            instance: EmoteInstance::new(&data.urn).ok()?,
            name: data.name.clone(),
            rarity: Rarity::Free,
            individual_data: Default::default(),
        })
    }

    fn owned(owned: OwnedEmoteData) -> Option<Self> {
        Some(Self {
            instance: EmoteInstance::new_with_token(
                &owned.urn,
                owned
                    .individual_data
                    .first()
                    .map(|data| data.token_id.clone()),
            )
            .ok()?,
            name: owned.name,
            rarity: Rarity::from(owned.rarity.as_str()),
            individual_data: owned.individual_data,
        })
    }

    fn time(&self) -> i64 {
        self.individual_data
            .first()
            .and_then(|t| t.transferred_at.parse::<i64>().ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum Rarity {
    Free,
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
    Mythic,
    Unique,
}

impl From<&str> for Rarity {
    fn from(value: &str) -> Self {
        match value {
            "" => Rarity::Free,
            "common" => Rarity::Common,
            "uncommon" => Rarity::Uncommon,
            "rare" => Rarity::Rare,
            "epic" => Rarity::Epic,
            "legendary" => Rarity::Legendary,
            "mythic" => Rarity::Mythic,
            "unique" => Rarity::Unique,
            _ => {
                warn!("unrecognised rarity {value}");
                Rarity::Free
            }
        }
    }
}

pub trait ColorHexEx {
    fn to_hex_color(&self) -> String;
}

impl ColorHexEx for Color {
    fn to_hex_color(&self) -> String {
        let color = self.to_linear().to_u8_array();
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color[0], color[1], color[2], color[3]
        )
    }
}

impl Rarity {
    pub fn color(&self) -> Color {
        match self {
            Rarity::Free => Color::srgb(0.9, 0.9, 0.9),
            Rarity::Common => Color::srgb(0.7, 0.7, 0.7),
            Rarity::Uncommon => Color::srgb(1.0, 0.8, 0.4),
            Rarity::Rare => Color::srgb(0.6, 1.0, 0.6),
            Rarity::Epic => Color::srgb(0.6, 0.6, 1.0),
            Rarity::Legendary => Color::srgb(0.8, 0.4, 0.8),
            Rarity::Mythic => Color::srgb(1.0, 0.6, 1.0),
            Rarity::Unique => Color::srgb(1.0, 1.0, 0.4),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_emotes_list(
    mut commands: Commands,
    dialog: Query<Ref<SettingsDialog>>,
    mut q: Query<(&mut EmotesSettings, &DuiEntities, &SelectItem)>,
    dui: Res<DuiRegistry>,
    mut emote_loader: CollectibleManager<Emote>,
    asset_server: Res<AssetServer>,
    mut retry: Local<bool>,
    base_emotes: Res<BaseEmotes>,
) {
    let Ok((mut settings, components, selected)) = q.single_mut() else {
        return;
    };

    if !*retry && !settings.is_changed() {
        return;
    }

    debug!("updating emotes here");

    let mut all_base_loaded = true;

    let mut emotes = if settings.only_collectibles {
        Vec::default()
    } else {
        base_emotes
            .0
            .iter()
            .cloned()
            .filter_map(|urn| {
                emote_loader
                    .get_data(&urn)
                    .map_err(|e| {
                        if matches!(e, CollectibleError::Loading) {
                            all_base_loaded = false
                        };
                        e
                    })
                    .ok()
                    .and_then(EmoteEntry::base)
            })
            .collect()
    };

    if !all_base_loaded {
        *retry = true;
        debug!("exit due to base loads");
        return;
    }
    *retry = false;
    debug!("base loads done");

    emotes.extend(
        settings
            .owned_emotes
            .iter()
            .cloned()
            .flat_map(EmoteEntry::owned),
    );

    if let Some(search) = &settings.search_filter {
        emotes.retain(|w| w.name.contains(search));
    }

    match settings.sort_by {
        SortBy::Newest => emotes.sort_by_key(|w| -w.time()),
        SortBy::Oldest => emotes.sort_by_key(|w| w.time()),
        SortBy::Alphabetic => emotes.sort_by(|w, w2| w.name.cmp(&w2.name)),
        SortBy::ReverseAlphabetic => emotes.sort_by(|w, w2| w2.name.cmp(&w.name)),
        SortBy::Rarest => {
            emotes.sort_by_key(|w| w.rarity);
            emotes.reverse();
        }
        SortBy::ReverseRarest => emotes.sort_by_key(|w| w.rarity),
        SortBy::Equipped => {
            let worn = settings
                .current_emotes
                .values()
                .map(|(urn, _)| urn)
                .collect::<HashSet<_>>();

            emotes.sort_by_key(|w| (!worn.contains(&w.instance), -w.time()))
        }
    }

    if emotes == settings.current_list && !dialog.single().is_ok_and(|d| d.is_changed()) {
        // emotes list matches and dialog has not changed (so current emotes have not changed)
        return;
    }

    settings.current_list.clone_from(&emotes);

    let worn = settings
        .current_emotes
        .values()
        .map(|(urn, _)| urn)
        .collect::<HashSet<_>>();

    commands
        .entity(components.named("items"))
        .despawn_descendants();

    let mut initial = None;
    let buttons: Vec<_> = emotes
        .into_iter()
        .enumerate()
        .map(|(ix, emote)| {
            if selected.0.as_ref().map(|w| &w.instance) == Some(&emote.instance) {
                initial = Some(ix);
            }
            let (inactive_color, inactive_border) = if worn.contains(&emote.instance) {
                (Color::Srgba(css::ORANGE), Color::srgb(0.5, 0.325, 0.0))
            } else {
                (
                    Color::srgba(0.0, 0.0, 0.0, 0.0),
                    Color::srgba(0.0, 0.0, 0.0, 0.0),
                )
            };

            let content = commands
                .spawn_template(&dui, "emote-item-pending", DuiProps::new())
                .unwrap()
                .root;
            commands
                .entity(content)
                .try_insert((emote, EmoteItemState::PendingMeta(ix)));

            DuiButton {
                styles: Some(InteractStyles {
                    active: Some(InteractStyle {
                        background: Some(css::RED.into()),
                        border: Some(Color::srgb(0.5, 0.0, 0.0)),
                        ..Default::default()
                    }),
                    inactive: Some(InteractStyle {
                        background: Some(inactive_color),
                        border: Some(inactive_border),
                        ..Default::default()
                    }),
                    disabled: Some(InteractStyle {
                        background: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                        border: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                image: Some(asset_server.load("images/backpack/item_bg.png")),
                children: Some(content),
                ..Default::default()
            }
        })
        .collect();

    let item_components = commands
        .entity(components.named("items"))
        .spawn_template(
            &dui,
            "emote-items",
            DuiProps::new()
                .with_prop("tabs", buttons)
                .with_prop("initial", initial)
                .with_prop(
                    "onchanged",
                    On::<DataChanged>::new(
                        |caller: Res<UiCaller>,
                         tab: Query<&TabSelection>,
                         emote: Query<&EmoteEntry>,
                         mut e: EventWriter<SelectItem>| {
                            let selection = tab
                                .get(caller.0)
                                .ok()
                                .and_then(|tab| tab.selected_entity())
                                .and_then(|nodes| emote.get(nodes.named("label")).ok());
                            e.send(SelectItem(selection.cloned()));
                            debug!("selected {:?}", selection)
                        },
                    ),
                ),
        )
        .unwrap();

    commands
        .entity(components.named("items"))
        .insert(item_components);
}

#[derive(Component, Debug)]
pub enum EmoteItemState {
    PendingMeta(usize),
    PendingImage(Handle<Image>),
}

#[allow(clippy::too_many_arguments)]
fn update_emote_item(
    mut commands: Commands,
    mut q: Query<(Entity, &EmoteEntry, &mut EmoteItemState)>,
    mut emote_loader: CollectibleManager<Emote>,
    ipfas: IpfsAssetServer,
    dui: Res<DuiRegistry>,
    settings: Query<(Entity, &EmotesSettings)>,
    walker: DuiWalker,
) {
    let Ok((settings_ent, settings)) = settings.single() else {
        return;
    };

    for (ent, entry, mut state) in q.iter_mut() {
        debug!("checking pending {:?}", state);

        let mut modified = true;
        while modified {
            modified = false;
            let urn = &entry.instance;
            match &*state {
                EmoteItemState::PendingMeta(ix) => {
                    let ix = *ix;
                    match emote_loader.get_data(urn.base()) {
                        Ok(data) => {
                            debug!("found {:?} -> {data:?}", entry.instance);
                            let fits = data
                                .available_representations
                                .contains(settings.body_shape.base().as_str());

                            *state = EmoteItemState::PendingImage(data.thumbnail.clone());

                            modified = true;

                            let Some(button_bg) = walker
                                .walk(settings_ent, format!("items.tab {ix}.button-background"))
                            else {
                                warn!("failed to find bg");
                                continue;
                            };

                            commands.entity(button_bg).try_insert(Enabled(fits));
                        }
                        Err(CollectibleError::Loading) => (),
                        other => {
                            warn!("failed to load emote: {other:?}");
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<EmoteItemState>()
                                .spawn_template(
                                    &dui,
                                    "emote-item",
                                    DuiProps::new()
                                        .with_prop(
                                            "img",
                                            ipfas
                                                .asset_server()
                                                .load::<Image>("images/backback/empty.png"),
                                        )
                                        .with_prop("rarity-color", entry.rarity.color()),
                                )
                                .unwrap();
                        }
                    }
                }
                EmoteItemState::PendingImage(handle) => {
                    let Ok(data) = emote_loader.get_data(urn.base()) else {
                        panic!();
                    };

                    let fits = data
                        .available_representations
                        .contains(settings.body_shape.base().as_str());

                    let (image_color, rarity_color) = if fits {
                        (Color::WHITE, entry.rarity.color())
                    } else {
                        (Color::BLACK, Color::Srgba(css::DARK_GRAY))
                    };
                    match ipfas.asset_server().load_state(handle.id()) {
                        bevy::asset::LoadState::Loading => (),
                        bevy::asset::LoadState::Loaded => {
                            debug!("loaded image");
                            debug!(
                                "image color {:?}, rarity color {:?}",
                                image_color, rarity_color
                            );
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<EmoteItemState>()
                                .spawn_template(
                                    &dui,
                                    "emote-item",
                                    DuiProps::new()
                                        .with_prop("img", handle.clone())
                                        .with_prop("rarity-color", rarity_color)
                                        .with_prop("img-color", image_color),
                                )
                                .unwrap();
                        }
                        bevy::asset::LoadState::Failed(_) | bevy::asset::LoadState::NotLoaded => {
                            warn!("failed to load emote image");
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<EmoteItemState>()
                                .spawn_template(
                                    &dui,
                                    "emote-item",
                                    DuiProps::new()
                                        .with_prop(
                                            "img",
                                            ipfas
                                                .asset_server()
                                                .load::<Image>("images/backback/empty.png"),
                                        )
                                        .with_prop("rarity-color", rarity_color)
                                        .with_prop("img-color", image_color),
                                )
                                .unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[derive(Event, Clone)]
struct SelectItem(Option<EmoteEntry>);

#[allow(clippy::too_many_arguments)]
fn update_selected_item(
    mut commands: Commands,
    mut e: EventReader<SelectItem>,
    settings: Query<(Entity, Ref<EmotesSettings>, &DuiEntities, &SelectItem)>,
    avatar: Query<(&AvatarShape, &BoothInstance), With<SettingsDialog>>,
    dui: Res<DuiRegistry>,
    mut emote_loader: CollectibleManager<Emote>,
    mut retry: Local<bool>,
    mut booth: PhotoBooth,
) {
    let Ok((settings_ent, settings, components, selection)) = settings.single() else {
        return;
    };

    let Ok((_avatar, instance)) = avatar.single() else {
        return;
    };

    let current_selection = if let Some(new_selection) = e.read().last() {
        commands
            .entity(settings_ent)
            .try_insert(new_selection.clone());

        if let Some(sel) = new_selection.0.as_ref() {
            booth.play_emote(instance, sel.instance.base().clone());
        }

        new_selection
    } else {
        if !settings.is_changed() && !*retry {
            return;
        }

        selection
    };

    *retry = false;

    let current_selection = current_selection
        .0
        .as_ref()
        .and_then(|sel| settings.current_list.iter().find(|s| s == &sel));
    commands
        .entity(components.named("selected-item"))
        .despawn_descendants();

    if let Some(sel) = current_selection {
        let body_shape = settings.body_shape.base().as_str();
        let Ok(_emote) = emote_loader.get_representation(sel.instance.base(), body_shape) else {
            *retry = true;
            return;
        };

        let Ok(data_ref) = emote_loader.get_data(sel.instance.base()) else {
            *retry = true;
            return;
        };

        let data = data_ref.clone();
        let instance = sel.instance.clone();
        let selected_slot = settings.selected_slot;
        let is_remove = settings
            .current_emotes
            .get(&selected_slot)
            .is_some_and(|(instance, _)| instance == &sel.instance);

        let label = if is_remove { "REMOVE" } else { "EQUIP" };

        let equip_action = On::<Click>::new(
            move |mut commands: Commands,
                  ipfas: IpfsAssetServer,
                  mut emotes: Query<(&mut EmotesSettings, &DuiEntities)>,
                  mut dialog: Query<(&mut SettingsDialog, &BoothInstance, &mut AvatarShape)>,
                  mut booth: PhotoBooth,
                  walker: DuiWalker| {
                let (mut emote_settings, components) = emotes.single_mut();
                if is_remove {
                    emote_settings.current_emotes.remove(&selected_slot)
                } else {
                    emote_settings
                        .current_emotes
                        .insert(selected_slot, (instance.clone(), data.clone()))
                };

                let Ok((mut dialog, booth_instance, mut avatar)) = dialog.single_mut() else {
                    warn!("fail to update dialog+booth instance");
                    return;
                };

                // mark profile as modified
                dialog.modified = true;

                // update emotes on avatar
                let old_emotes = avatar.0.emotes.clone();
                let mut emotes = avatar
                    .0
                    .emotes
                    .drain(..)
                    .map(|e| EmoteInstance::new(&e).ok())
                    .enumerate()
                    .flat_map(|(ix, maybe_emote)| maybe_emote.map(|e| (ix, e)))
                    .collect::<HashMap<_, _>>();

                if is_remove {
                    emotes.remove(&selected_slot);
                } else {
                    emotes.insert(selected_slot, instance.clone());
                }

                let new_emotes = (0..=9)
                    .map(|slot| {
                        emotes
                            .remove(&slot)
                            .map(|emote| emote.instance_urn())
                            .unwrap_or_default()
                    })
                    .collect();
                debug!("emotes change\n{:?}\n{:?}", old_emotes, new_emotes);
                avatar.0.emotes = new_emotes;
                // and photobooth
                booth.update_shape(booth_instance, avatar.clone());

                // update image on slot tab
                let Some(image_entity) = walker.walk(
                    components.root,
                    format!("slot-tabs.tab {selected_slot}.label.item-image"),
                ) else {
                    warn!("failed to find image entity");
                    return;
                };

                let empty_img = ipfas
                    .asset_server()
                    .load::<Image>("images/backpack/empty.png");
                let emote_img = emote_settings
                    .current_emotes
                    .get(&selected_slot)
                    .map(|(_, data)| data.thumbnail.clone())
                    .unwrap_or_else(|| empty_img.clone());

                commands
                    .entity(image_entity)
                    .try_insert(UiImage::new(emote_img));
            },
        );

        commands
            .entity(components.named("selected-item"))
            .spawn_template(
                &dui,
                "emote-selection",
                DuiProps::new()
                    .with_prop("rarity-color", sel.rarity.color())
                    .with_prop("selection-image", data_ref.thumbnail.clone())
                    .with_prop("title", data_ref.name.clone())
                    .with_prop("body", data_ref.description.clone())
                    .with_prop("label", label.to_owned())
                    .with_prop("onclick", equip_action),
            )
            .unwrap();
    }
}
