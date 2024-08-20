use std::str::FromStr;

use anyhow::anyhow;
use avatar::{
    avatar_texture::{BoothInstance, PhotoBooth, PROFILE_UI_RENDERLAYER},
    AvatarShape,
};
use bevy::{
    color::palettes::css,
    prelude::*,
    render::render_resource::Extent3d,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bevy_dui::{
    DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry, DuiWalker,
};
use collectibles::{
    base_wearables::{self, base_wearable_urns, default_bodyshape_instance},
    wearables::{Wearable, WearableCategory, WearableCollections, WearableInstance},
    CollectibleData, CollectibleError, CollectibleManager,
};
use common::{
    structs::{PrimaryUser, SettingsTab},
    util::{TaskExt, TryPushChildrenEx},
};
use comms::profile::CurrentUserProfile;
use ipfs::IpfsAssetServer;
use isahc::ReadResponseExt;
use serde::Deserialize;
use tween::SystemTween;
use ui_core::{
    button::{DuiButton, TabSelection},
    color_picker::ColorPicker,
    combo_box::ComboBox,
    interact_style::{InteractStyle, InteractStyles},
    textentry::TextEntry,
    toggle::Toggled,
    ui_actions::{Click, DataChanged, Enabled, On, UiCaller},
};

use crate::profile::SettingsDialog;

pub struct WearableSettingsPlugin;

impl Plugin for WearableSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<GetOwnedWearables>()
            .add_event::<SelectItem>()
            .add_systems(
                Update,
                (
                    set_wearables_content,
                    (
                        apply_deferred,
                        get_owned_wearables,
                        update_wearables_list,
                        apply_deferred,
                        update_wearable_item,
                        update_selected_item,
                    )
                        .chain()
                        .run_if(|q: Query<&SettingsTab>| {
                            q.get_single()
                                .map_or(false, |tab| tab == &SettingsTab::Wearables)
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
pub struct OwnedWearableData {
    pub urn: String,
    pub name: String,
    pub category: String,
    pub rarity: String,
    pub individual_data: Vec<IndividualData>,
}

#[derive(Deserialize)]
pub struct OwnedWearableServerResponse {
    elements: Vec<OwnedWearableData>,
}

#[derive(Component, Clone)]
pub struct WearablesSettings {
    pub body_shape: WearableInstance,
    pub only_collectibles: bool,
    pub category: Option<&'static WearableCategory>,
    pub collection: Option<String>,
    pub sort_by: SortBy,
    pub search_filter: Option<String>,
    pub current_wearables: HashMap<WearableCategory, (WearableInstance, CollectibleData<Wearable>)>,
    pub owned_wearables: Vec<OwnedWearableData>,
    current_list: Vec<WearableEntry>,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn set_wearables_content(
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
        Option<&mut WearablesSettings>,
        Has<SelectItem>,
    )>,
    dui: Res<DuiRegistry>,
    mut booth: PhotoBooth,
    player: Query<&AvatarShape, (Without<SettingsDialog>, With<PrimaryUser>)>,
    mut prev_tab: Local<Option<SettingsTab>>,
    ipfas: IpfsAssetServer,
    mut wearable_loader: CollectibleManager<Wearable>,
    mut e: EventWriter<GetOwnedWearables>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab, wearable_settings, has_select) in q.iter_mut() {
        let Ok((settings_entity, maybe_instance, _, dialog)) = dialog.get_single() else {
            return;
        };

        if *prev_tab == Some(*tab) && !dialog.is_changed() {
            continue;
        }

        if tab != &SettingsTab::Wearables {
            *prev_tab = Some(*tab);
            return;
        }

        debug!("redraw");

        commands.entity(ent).despawn_descendants();

        let instance = maybe_instance.cloned().unwrap_or_else(|| {
            let avatar = player.get_single().unwrap();
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

        let new_settings;
        let wearable_settings = match wearable_settings {
            Some(mut settings) => {
                // reset cached data
                settings.current_list = Vec::default();
                settings.into_inner()
            }
            None => {
                let player_shape = &player.get_single().unwrap().shape;
                let body_instance =
                    WearableInstance::new(player_shape.body_shape.as_ref().unwrap())
                        .unwrap_or_else(|_| default_bodyshape_instance());
                let body = match wearable_loader.get_data(body_instance.base()) {
                    Ok(body) => body.clone(),
                    Err(CollectibleError::Loading) => {
                        debug!("bail loading");
                        return; // retry next frame
                    }
                    _ => {
                        let Ok(default) =
                            wearable_loader.get_data(base_wearables::default_bodyshape_urn())
                        else {
                            debug!("bail loading default");
                            return;
                        };
                        default.clone()
                    }
                };

                let mut all_loaded = true;

                new_settings = WearablesSettings {
                    body_shape: body_instance.clone(),
                    current_wearables: player_shape
                        .wearables
                        .iter()
                        .flat_map(|w| WearableInstance::new(w).ok())
                        .flat_map(|instance| match wearable_loader.get_data(instance.base()) {
                            Ok(w) => Some((instance, w.clone())),
                            Err(CollectibleError::Loading) => {
                                all_loaded = false;
                                None
                            }
                            _ => None,
                        })
                        .map(|(instance, wearable)| {
                            (wearable.extra_data.category, (instance, wearable))
                        })
                        .chain(std::iter::once((
                            WearableCategory::BODY_SHAPE,
                            (body_instance.clone(), body),
                        )))
                        .collect(),
                    only_collectibles: Default::default(),
                    category: Default::default(),
                    collection: Default::default(),
                    sort_by: Default::default(),
                    search_filter: Default::default(),
                    owned_wearables: Default::default(),
                    current_list: Default::default(),
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
        let category_tabs: Vec<_> = WearableCategory::iter()
            .map(|category| {
                let wearable_img = wearable_settings
                    .current_wearables
                    .get(category)
                    .map(|(_, data)| data.thumbnail.clone())
                    .unwrap_or_else(|| empty_img.clone());

                let content = commands
                    .spawn_template(
                        &dui,
                        "wearable-category",
                        DuiProps::new()
                            .with_prop(
                                "category-img",
                                ipfas.asset_server().load::<Image>(format!(
                                    "images/backpack/wearable_categories/{}.png",
                                    category.slot
                                )),
                            )
                            .with_prop("wearable-img", wearable_img),
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

        let initial_category = wearable_settings.category.and_then(|c| {
            WearableCategory::iter()
                .enumerate()
                .find(|(_, w)| w == &c)
                .map(|(ix, _)| ix)
        });

        let booth_camera = instance.camera;
        let props = DuiProps::new()
            .with_prop("booth-instance", instance)
            .with_prop(
                "only-collectibles",
                On::<DataChanged>::new(only_collectibles),
            )
            .with_prop("only-collectibles-set", wearable_settings.only_collectibles)
            .with_prop("collections", Vec::<String>::default())
            .with_prop("initial-collection", -1isize)
            .with_prop("category-tabs", category_tabs)
            .with_prop("initial-category", initial_category)
            .with_prop(
                "category-changed",
                On::<DataChanged>::new(
                    move |mut commands: Commands,
                          caller: Res<UiCaller>,
                          tab: Query<&TabSelection>,
                          mut settings: Query<&mut WearablesSettings>| {
                        let Ok(selection) = tab.get(caller.0) else {
                            warn!("failed to get tab");
                            return;
                        };

                        let Ok(mut settings) = settings.get_single_mut() else {
                            warn!("failed to get settings");
                            return;
                        };

                        settings.category = selection
                            .selected
                            .and_then(|selection| WearableCategory::iter().nth(selection));

                        if let Some(mut commands) = commands.get_entity(booth_camera) {
                            commands.try_insert(SystemTween {
                                target: target_position(
                                    settings.category.unwrap_or(&WearableCategory::BODY_SHAPE),
                                ),
                                time: 0.5,
                            });
                        };
                    },
                ),
            )
            .with_prop("sort-by", SortBy::strings())
            .with_prop(
                "initial-sort-by",
                SortBy::strings()
                    .iter()
                    .position(|sb| sb == wearable_settings.sort_by.to())
                    .unwrap() as isize,
            )
            .with_prop(
                "sort-by-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&ComboBox>,
                     mut settings: Query<&mut WearablesSettings>| {
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
                wearable_settings.search_filter.clone().unwrap_or_default(),
            )
            .with_prop(
                "filter-changed",
                On::<DataChanged>::new(
                    |caller: Res<UiCaller>,
                     q: Query<&TextEntry>,
                     mut settings: Query<&mut WearablesSettings>| {
                        let Ok(value) = q.get(caller.0).map(|te| te.content.clone()) else {
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
            .apply_template(&dui, "wearables", props)
            .unwrap();
        commands.entity(ent).try_insert(components);

        e.send_default();
        *prev_tab = Some(*tab);
    }
}

fn only_collectibles(
    caller: Res<UiCaller>,
    toggle: Query<&Toggled>,
    mut q: Query<&mut WearablesSettings>,
) {
    let Ok(toggle) = toggle.get(**caller) else {
        warn!("toggle access failed");
        return;
    };

    let Ok(mut settings) = q.get_single_mut() else {
        warn!("settings access failed");
        return;
    };

    settings.only_collectibles = toggle.0;
}

#[derive(Event, Default)]
struct GetOwnedWearables;

fn get_owned_wearables(
    mut e: EventReader<GetOwnedWearables>,
    mut task: Local<Option<Task<Result<OwnedWearableServerResponse, anyhow::Error>>>>,
    mut q: Query<&mut WearablesSettings>,
    ipfas: IpfsAssetServer,
    current_profile: Res<CurrentUserProfile>,
    collections: Res<WearableCollections>,
    mut collections_box: Query<(&mut ComboBox, &Name)>,
) {
    let ev = e.read().last().is_some();

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(wearable_data)) => {
                if let Ok(mut settings) = q.get_single_mut() {
                    debug!("wearable task ok");
                    settings.owned_wearables = wearable_data.elements;

                    let owned = settings
                        .owned_wearables
                        .iter()
                        .flat_map(|w| WearableEntry::owned(w.clone()))
                        .collect::<Vec<_>>();

                    let mut collection_names = owned
                        .iter()
                        .flat_map(|w| w.instance.base().collection())
                        .filter_map(|c| match collections.0.get(c) {
                            Some(name) => Some(name.clone()),
                            None => {
                                debug!("collection not found: {c} not in {:?}", collections.0);
                                None
                            }
                        })
                        .collect::<HashSet<_>>();

                    collection_names.insert("Base Wearables".to_owned());
                    let mut collections_box = collections_box
                        .iter_mut()
                        .filter(|(_, name)| name.as_str() == "collections")
                        .map(|(cb, _)| cb)
                        .next()
                        .unwrap();
                    let current_selection = collections_box.selected().cloned();
                    collections_box.options = collection_names.into_iter().collect::<Vec<_>>();
                    collections_box.options.sort();
                    collections_box.selected = current_selection
                        .and_then(|sel| collections_box.options.iter().position(|i| i == &sel))
                        .map(|ix| ix as isize)
                        .unwrap_or(-1);
                }
            }
            Some(Err(e)) => {
                warn!("owned wearable task failed: {e}");
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
            warn!("no profile, not loading custom wearables");
            return;
        };

        *task = Some(IoTaskPool::get().spawn(async move {
            let mut response = isahc::get(format!("{endpoint}/users/{address}/wearables"))
                .map_err(|e| anyhow!(e))?;
            response
                .json::<OwnedWearableServerResponse>()
                .map_err(|e| anyhow!(e))
        }));
    }
}

#[derive(Component, Clone, Debug)]
struct WearableEntry {
    pub instance: WearableInstance,
    pub name: String,
    pub category: WearableCategory,
    pub rarity: Rarity,
    pub individual_data: Vec<IndividualData>,
}

impl PartialEq for WearableEntry {
    fn eq(&self, other: &Self) -> bool {
        self.instance.eq(&other.instance)
    }
}

impl WearableEntry {
    fn base(data: &CollectibleData<Wearable>) -> Option<Self> {
        Some(Self {
            instance: WearableInstance::new(&data.urn).ok()?,
            name: data.name.clone(),
            category: data.extra_data.category,
            rarity: Rarity::Free,
            individual_data: Default::default(),
        })
    }

    fn owned(owned: OwnedWearableData) -> Option<Self> {
        Some(Self {
            instance: WearableInstance::new_with_token(
                &owned.urn,
                owned
                    .individual_data
                    .first()
                    .map(|data| data.token_id.clone()),
            )
            .ok()?,
            name: owned.name,
            category: WearableCategory::from_str(&owned.category)
                .unwrap_or(WearableCategory::UNKNOWN),
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
fn update_wearables_list(
    mut commands: Commands,
    dialog: Query<Ref<SettingsDialog>>,
    mut q: Query<(&mut WearablesSettings, &DuiEntities, &SelectItem)>,
    dui: Res<DuiRegistry>,
    mut wearable_loader: CollectibleManager<Wearable>,
    asset_server: Res<AssetServer>,
    collections: Res<WearableCollections>,
    mut retry: Local<bool>,
) {
    let Ok((mut settings, components, selected)) = q.get_single_mut() else {
        return;
    };

    if !*retry && !settings.is_changed() {
        debug!("not updating wearables here");
        return;
    }

    debug!("updating wearables here");

    let mut all_base_loaded = true;

    let mut wearables = if settings.only_collectibles {
        Vec::default()
    } else {
        base_wearable_urns()
            .into_iter()
            .filter_map(|urn| {
                wearable_loader
                    .get_data(&urn)
                    .map_err(|e| {
                        if matches!(e, CollectibleError::Loading) {
                            all_base_loaded = false
                        };
                        e
                    })
                    .ok()
                    .and_then(WearableEntry::base)
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

    wearables.extend(
        settings
            .owned_wearables
            .iter()
            .cloned()
            .flat_map(WearableEntry::owned),
    );

    if let Some(category) = settings.category {
        wearables.retain(|w| &w.category == category);
    }

    if let Some(collection) = &settings.collection {
        wearables.retain(|w| {
            w.instance
                .base()
                .collection()
                .and_then(|c| collections.0.get(c))
                == Some(collection)
        });
    }

    if let Some(search) = &settings.search_filter {
        wearables.retain(|w| w.name.contains(search));
    }

    match settings.sort_by {
        SortBy::Newest => wearables.sort_by_key(|w| -w.time()),
        SortBy::Oldest => wearables.sort_by_key(|w| w.time()),
        SortBy::Alphabetic => wearables.sort_by(|w, w2| w.name.cmp(&w2.name)),
        SortBy::ReverseAlphabetic => wearables.sort_by(|w, w2| w2.name.cmp(&w.name)),
        SortBy::Rarest => {
            wearables.sort_by_key(|w| w.rarity);
            wearables.reverse();
        }
        SortBy::ReverseRarest => wearables.sort_by_key(|w| w.rarity),
        SortBy::Equipped => {
            let worn = settings
                .current_wearables
                .values()
                .map(|(urn, _)| urn)
                .collect::<HashSet<_>>();

            wearables.sort_by_key(|w| (!worn.contains(&w.instance), -w.time()))
        }
    }

    if wearables == settings.current_list && !dialog.get_single().map_or(false, |d| d.is_changed())
    {
        // wearables list matches and dialog has not changed (so current wearables have not changed)
        return;
    }

    settings.current_list.clone_from(&wearables);

    let worn = settings
        .current_wearables
        .values()
        .map(|(urn, _)| urn)
        .collect::<HashSet<_>>();

    commands
        .entity(components.named("items"))
        .despawn_descendants();

    let mut initial = None;
    let buttons: Vec<_> = wearables
        .into_iter()
        .enumerate()
        .map(|(ix, wearable)| {
            if selected.0.as_ref().map(|w| &w.instance) == Some(&wearable.instance) {
                initial = Some(ix);
            }
            let (inactive_color, inactive_border) = if worn.contains(&wearable.instance) {
                (Color::Srgba(css::ORANGE), Color::srgb(0.5, 0.325, 0.0))
            } else {
                if wearable.category == WearableCategory::BODY_SHAPE {
                    debug!("worn does not contain {:?} - {:?}", wearable.instance, worn);
                }
                (
                    Color::srgba(0.0, 0.0, 0.0, 0.0),
                    Color::srgba(0.0, 0.0, 0.0, 0.0),
                )
            };

            let content = commands
                .spawn_template(&dui, "wearable-item-pending", DuiProps::new())
                .unwrap()
                .root;
            commands
                .entity(content)
                .try_insert((wearable, WearableItemState::PendingMeta(ix)));

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
            "wearable-items",
            DuiProps::new()
                .with_prop("tabs", buttons)
                .with_prop("initial", initial)
                .with_prop(
                    "onchanged",
                    On::<DataChanged>::new(
                        |caller: Res<UiCaller>,
                         tab: Query<&TabSelection>,
                         wearable: Query<&WearableEntry>,
                         mut e: EventWriter<SelectItem>| {
                            let selection = tab
                                .get(caller.0)
                                .ok()
                                .and_then(|tab| tab.selected_entity())
                                .and_then(|nodes| wearable.get(nodes["label"]).ok());
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
pub enum WearableItemState {
    PendingMeta(usize),
    PendingImage(Handle<Image>),
}

#[allow(clippy::too_many_arguments)]
fn update_wearable_item(
    mut commands: Commands,
    mut q: Query<(Entity, &WearableEntry, &mut WearableItemState)>,
    mut wearable_loader: CollectibleManager<Wearable>,
    ipfas: IpfsAssetServer,
    dui: Res<DuiRegistry>,
    settings: Query<(Entity, &WearablesSettings)>,
    walker: DuiWalker,
) {
    let Ok((settings_ent, settings)) = settings.get_single() else {
        return;
    };

    for (ent, entry, mut state) in q.iter_mut() {
        debug!("checking pending {:?}", state);

        let mut modified = true;
        while modified {
            modified = false;
            let urn = &entry.instance;
            match &*state {
                WearableItemState::PendingMeta(ix) => {
                    let ix = *ix;
                    match wearable_loader.get_data(urn.base()) {
                        Ok(data) => {
                            debug!("found {:?} -> {data:?}", entry.instance);
                            let fits = entry.category == WearableCategory::BODY_SHAPE
                                || data
                                    .available_representations
                                    .contains(settings.body_shape.base().as_str());

                            *state = WearableItemState::PendingImage(data.thumbnail.clone());

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
                            warn!("failed to load wearable: {other:?}");
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<WearableItemState>()
                                .spawn_template(
                                    &dui,
                                    "wearable-item",
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
                WearableItemState::PendingImage(handle) => {
                    let Ok(data) = wearable_loader.get_data(urn.base()) else {
                        panic!();
                    };

                    let fits = entry.category == WearableCategory::BODY_SHAPE
                        || data
                            .available_representations
                            .contains(settings.body_shape.base().as_str());

                    let (image_color, rarity_color) = if fits {
                        (Color::WHITE.to_hex_color(), entry.rarity.color())
                    } else {
                        (Color::BLACK.to_hex_color(), Color::Srgba(css::DARK_GRAY))
                    };
                    match ipfas.asset_server().load_state(handle.id()) {
                        bevy::asset::LoadState::Loading => (),
                        bevy::asset::LoadState::Loaded => {
                            debug!("loaded image");
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<WearableItemState>()
                                .spawn_template(
                                    &dui,
                                    "wearable-item",
                                    DuiProps::new()
                                        .with_prop("img", handle.clone())
                                        .with_prop("rarity-color", rarity_color)
                                        .with_prop("img-color", image_color),
                                )
                                .unwrap();
                        }
                        bevy::asset::LoadState::Failed(_) | bevy::asset::LoadState::NotLoaded => {
                            warn!("failed to load wearable image");
                            commands
                                .entity(ent)
                                .despawn_descendants()
                                .remove::<WearableItemState>()
                                .spawn_template(
                                    &dui,
                                    "wearable-item",
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
struct SelectItem(Option<WearableEntry>);

#[allow(clippy::too_many_arguments)]
fn update_selected_item(
    mut commands: Commands,
    mut e: EventReader<SelectItem>,
    settings: Query<(Entity, Ref<WearablesSettings>, &DuiEntities, &SelectItem)>,
    avatar: Query<&AvatarShape, With<SettingsDialog>>,
    dui: Res<DuiRegistry>,
    mut wearable_loader: CollectibleManager<Wearable>,
    mut retry: Local<bool>,
) {
    let Ok((settings_ent, settings, components, selection)) = settings.get_single() else {
        return;
    };

    let Ok(avatar) = avatar.get_single() else {
        return;
    };

    let current_selection = if let Some(new_selection) = e.read().last() {
        commands
            .entity(settings_ent)
            .try_insert(new_selection.clone());
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

    let worn = settings
        .current_wearables
        .values()
        .map(|(urn, _)| urn)
        .collect::<HashSet<_>>();

    if let Some(sel) = current_selection {
        let body_shape = if sel.category == WearableCategory::BODY_SHAPE {
            sel.instance.base().as_str()
        } else {
            settings.body_shape.base().as_str()
        };
        let Ok(wearable) = wearable_loader.get_representation(sel.instance.base(), body_shape)
        else {
            *retry = true;
            return;
        };
        let mut hides = wearable.hides.clone().into_iter().collect::<Vec<_>>();
        hides.sort_unstable();

        let Ok(data_ref) = wearable_loader.get_data(sel.instance.base()) else {
            *retry = true;
            return;
        };

        let data = data_ref.clone();
        let category = sel.category;
        let instance = sel.instance.clone();
        let is_remove = worn.contains(&sel.instance);

        let label = if is_remove { "REMOVE" } else { "EQUIP" };

        let enabled = !(matches!(category, WearableCategory::BODY_SHAPE) && is_remove);

        let equip_action = On::<Click>::new(
            move |mut commands: Commands,
                  ipfas: IpfsAssetServer,
                  mut wearables: Query<(&mut WearablesSettings, &DuiEntities)>,
                  mut dialog: Query<(&mut SettingsDialog, &BoothInstance, &mut AvatarShape)>,
                  mut booth: PhotoBooth,
                  walker: DuiWalker| {
                let (mut wearable_settings, components) = wearables.single_mut();
                let prev = if is_remove {
                    wearable_settings.current_wearables.remove(&category)
                } else {
                    wearable_settings
                        .current_wearables
                        .insert(category, (instance.clone(), data.clone()))
                };

                let Ok((mut dialog, booth_instance, mut avatar)) = dialog.get_single_mut() else {
                    warn!("fail to update dialog+booth instance");
                    return;
                };

                // mark profile as modified
                dialog.modified = true;

                // update wearables on avatar
                let old_wearables = avatar.shape.wearables.clone();
                let mut wearables = avatar
                    .shape
                    .wearables
                    .drain(..)
                    .flat_map(|w| WearableInstance::new(&w).ok())
                    .collect::<HashSet<_>>();
                if let Some((old_instance, _)) = prev {
                    if category != WearableCategory::BODY_SHAPE && !wearables.remove(&old_instance)
                    {
                        warn!("failed to remove {old_instance:?} from {wearables:?}");
                    }
                }
                match category {
                    WearableCategory::BODY_SHAPE => {
                        avatar.shape.body_shape = Some(instance.instance_urn());
                        wearable_settings.body_shape = instance.clone();
                    }
                    _ => {
                        if !is_remove {
                            wearables.insert(instance.clone());
                        }
                    }
                }
                let new_wearables = wearables.into_iter().map(|w| w.instance_urn()).collect();
                debug!("wearables change\n{:?}\n{:?}", old_wearables, new_wearables);
                avatar.shape.wearables = new_wearables;
                // and photobooth
                booth.update_shape(booth_instance, avatar.clone());

                // update image on category tab
                let Some(button_ix) = category.index() else {
                    warn!("failed to find cat ix");
                    return;
                };
                let Some(image_entity) = walker.walk(
                    components.root,
                    format!("category-tabs.tab {button_ix}.label.item-image"),
                ) else {
                    warn!("failed to find image entity");
                    return;
                };

                let empty_img = ipfas
                    .asset_server()
                    .load::<Image>("images/backpack/empty.png");
                let wearable_img = wearable_settings
                    .current_wearables
                    .get(&category)
                    .map(|(_, data)| data.thumbnail.clone())
                    .unwrap_or_else(|| empty_img.clone());

                commands
                    .entity(image_entity)
                    .try_insert(UiImage::new(wearable_img));
            },
        );

        let (picker_display, color) = match category {
            WearableCategory::EYEBROWS | WearableCategory::FACIAL_HAIR | WearableCategory::HAIR => {
                (
                    "flex".to_owned(),
                    Color::from(avatar.shape.hair_color.unwrap_or_default()),
                )
            }
            WearableCategory::EYES => (
                "flex".to_owned(),
                Color::from(avatar.shape.eye_color.unwrap_or_default()),
            ),
            WearableCategory::BODY_SHAPE => (
                "flex".to_owned(),
                Color::from(avatar.shape.skin_color.unwrap_or_default()),
            ),
            _ => ("none".to_owned(), default()),
        };

        debug!("display : {picker_display}");
        let color_picker_changed = On::<DataChanged>::new(
            move |caller: Res<UiCaller>,
                  picker: Query<&ColorPicker>,
                  mut dialog: Query<(&mut SettingsDialog, &BoothInstance, &mut AvatarShape)>,
                  mut booth: PhotoBooth| {
                let Ok(picker) = picker.get(caller.0) else {
                    warn!("failed to get picker");
                    return;
                };

                let Ok((mut dialog, instance, mut avatar)) = dialog.get_single_mut() else {
                    warn!("fail to update dialog+booth instance");
                    return;
                };

                // mark profile as modified
                dialog.modified = true;

                // update color on avatar
                let target = match category {
                    WearableCategory::EYEBROWS
                    | WearableCategory::FACIAL_HAIR
                    | WearableCategory::HAIR => &mut avatar.shape.hair_color,
                    WearableCategory::EYES => &mut avatar.shape.eye_color,
                    WearableCategory::BODY_SHAPE => &mut avatar.shape.skin_color,
                    _ => panic!(),
                };
                *target = Some(picker.get_linear().into());

                // and photobooth
                booth.update_shape(instance, avatar.clone());
            },
        );

        let components = commands
            .entity(components.named("selected-item"))
            .spawn_template(
                &dui,
                "wearable-selection",
                DuiProps::new()
                    .with_prop("rarity-color", sel.rarity.color())
                    .with_prop("selection-image", data_ref.thumbnail.clone())
                    .with_prop("title", data_ref.name.clone())
                    .with_prop("body", data_ref.description.clone())
                    .with_prop("label", label.to_owned())
                    .with_prop("enabled", enabled)
                    .with_prop("onclick", equip_action)
                    .with_prop("color-picker-display", picker_display)
                    .with_prop("color", color)
                    .with_prop("color-changed", color_picker_changed),
            )
            .unwrap();

        for category in hides {
            let child = commands
                .spawn_template(
                    &dui,
                    "wearable-hides",
                    DuiProps::new().with_prop(
                        "img",
                        format!("images/backpack/wearable_categories/{}.png", category.slot),
                    ),
                )
                .unwrap()
                .root;
            commands
                .entity(components.named("hides"))
                .try_push_children(&[child]);
        }
    }
}

fn target_position(cat: &WearableCategory) -> Transform {
    match *cat {
        WearableCategory::SKIN | WearableCategory::BODY_SHAPE => Transform {
            translation: Vec3::new(1.2844446, 1.1353853, -2.876228),
            rotation: Quat::from_xyzw(0.0, 0.978031, 0.0, 0.20845993),
            scale: Vec3::ONE,
        },
        WearableCategory::HAIR => Transform {
            translation: Vec3::new(0.5859284, 1.7501538, -0.7222105),
            rotation: Quat::from_xyzw(0.0, 0.94248885, 0.0, 0.33423764),
            scale: Vec3::ONE,
        },
        WearableCategory::EYEBROWS
        | WearableCategory::EYES
        | WearableCategory::MOUTH
        | WearableCategory::MASK
        | WearableCategory::HELMET
        | WearableCategory::EYEWEAR
        | WearableCategory::FACIAL_HAIR => Transform {
            translation: Vec3::new(0.04801171, 1.7916923, -0.77852094),
            rotation: Quat::from_xyzw(0.0, 0.9995258, 0.0, 0.030791335),
            scale: Vec3::ONE,
        },
        WearableCategory::UPPER_BODY => Transform {
            translation: Vec3::new(-0.17291786, 1.5203078, -1.7514846),
            rotation: Quat::from_xyzw(0.0, 0.9987898, -0.0, -0.049183927),
            scale: Vec3::ONE,
        },
        WearableCategory::HAND_WEAR => Transform {
            translation: Vec3::new(-2.0522792, 1.2433841, -1.8454696),
            rotation: Quat::from_xyzw(0.0, 0.9134134, -0.0, -0.40703315),
            scale: Vec3::ONE,
        },
        WearableCategory::LOWER_BODY => Transform {
            translation: Vec3::new(1.0300425, 0.5, -1.5004734),
            rotation: Quat::from_xyzw(0.0, 0.9551008, 0.0, 0.29628116),
            scale: Vec3::ONE,
        },
        WearableCategory::FEET => Transform {
            translation: Vec3::new(-0.81119233, 0.1, -1.1897795),
            rotation: Quat::from_xyzw(0.0, 0.95557153, -0.0, -0.29475912),
            scale: Vec3::ONE,
        },
        WearableCategory::TOP_HEAD | WearableCategory::TIARA | WearableCategory::HAT => Transform {
            translation: Vec3::new(-0.554511, 1.9, -1.0188804),
            rotation: Quat::from_xyzw(0.0, 0.96910924, -0.0, -0.24663205),
            scale: Vec3::ONE,
        },
        WearableCategory::EARRING => Transform {
            translation: Vec3::new(-0.8107094, 1.752923, -0.43491435),
            rotation: Quat::from_xyzw(0.0, 0.858118, -0.0, -0.5134526),
            scale: Vec3::ONE,
        },
        _ => panic!(),
    }
}
