use std::str::FromStr;

use anyhow::anyhow;
use avatar::{
    avatar_texture::{BoothInstance, PhotoBooth, PROFILE_UI_RENDERLAYER},
    base_wearables::base_wearables,
    AvatarShape, RequestedWearables, WearableCategory, WearableMeta, WearableMetas,
    WearablePointerResult, WearablePointers,
};
use bevy::{
    prelude::*,
    render::render_resource::Extent3d,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bevy_dui::{
    DuiCommandsExt, DuiEntities, DuiEntityCommandsExt, DuiProps, DuiRegistry, DuiWalker,
};
use common::{structs::PrimaryUser, util::TaskExt};
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
use urn::Urn;

use crate::profile2::{SettingsDialog, SettingsTab};

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
pub struct TransferredAt {
    transferred_at: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OwnedWearableData {
    pub urn: String,
    pub name: String,
    pub category: String,
    pub rarity: String,
    pub individual_data: Vec<TransferredAt>,
}

#[derive(Deserialize)]
pub struct OwnedWearableServerResponse {
    elements: Vec<OwnedWearableData>,
}

#[derive(Component, Default, Clone)]
pub struct WearablesSettings {
    pub body_shape: String,
    pub only_collectibles: bool,
    pub category: Option<&'static WearableCategory>,
    pub collection: Option<String>,
    pub sort_by: SortBy,
    pub search_filter: Option<String>,
    // cat -> (urn, hash)
    pub current_wearables: HashMap<WearableCategory, (String, String)>,
    pub owned_wearables: Vec<OwnedWearableData>,
    current_list: Vec<WearableEntry>,
    pub current_wearable_images: HashMap<WearableCategory, Entity>,
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
    mut q: Query<
        (
            Entity,
            &SettingsTab,
            Option<&mut WearablesSettings>,
            Has<SelectItem>,
        ),
        Changed<SettingsTab>,
    >,
    dui: Res<DuiRegistry>,
    mut booth: PhotoBooth,
    player: Query<&AvatarShape, (Without<SettingsDialog>, With<PrimaryUser>)>,
    wearable_metas: ResMut<WearableMetas>,
    mut prev_tab: Local<Option<SettingsTab>>,
    ipfas: IpfsAssetServer,
    wearable_pointers: Res<WearablePointers>,
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
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Wearables {
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
                    width: 1,
                    height: 1,
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
                let player_shape = &player.get_single().unwrap().0;
                let body_shape = player_shape.body_shape.clone().unwrap();
                let body_shape_hash = wearable_pointers
                    .0
                    .get(&Urn::from_str(&body_shape.to_lowercase()).unwrap())
                    .unwrap()
                    .hash()
                    .unwrap()
                    .to_owned();

                new_settings = WearablesSettings {
                    body_shape: body_shape.clone(),
                    current_wearables: player_shape
                        .wearables
                        .iter()
                        .flat_map(|wearable| {
                            Urn::from_str(wearable)
                                .ok()
                                .and_then(|urn| wearable_pointers.0.get(&urn))
                                .and_then(WearablePointerResult::hash)
                                .and_then(|hash| {
                                    wearable_metas.0.get(hash).map(|meta| (meta, hash))
                                })
                                .map(|(meta, hash)| {
                                    (meta.data.category, (meta.id.to_owned(), hash.to_owned()))
                                })
                        })
                        .chain(std::iter::once((
                            WearableCategory::BODY_SHAPE,
                            (body_shape, body_shape_hash),
                        )))
                        .collect(),
                    ..Default::default()
                };
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
                    .and_then(|(_, hash)| wearable_metas.0.get(hash).map(|m| (m, hash)))
                    .map(|(meta, hash)| ipfas.load_content_file(&meta.thumbnail, hash).unwrap())
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
                            background: Some(Color::ORANGE),
                            ..Default::default()
                        }),
                        inactive: Some(InteractStyle {
                            background: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
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
) {
    let ev = e.read().last().is_some();

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(wearable_data)) => {
                if let Ok(mut settings) = q.get_single_mut() {
                    warn!("wearable task ok");
                    settings.owned_wearables = wearable_data.elements;
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
enum WearableEntry {
    Base(Urn, WearableMeta),
    Owned(OwnedWearableData),
}

impl PartialEq for WearableEntry {
    fn eq(&self, other: &Self) -> bool {
        self.id().eq(other.id())
    }
}

impl WearableEntry {
    fn category(&self) -> Option<WearableCategory> {
        match self {
            WearableEntry::Base(_, m) => Some(m.data.category),
            WearableEntry::Owned(o) => WearableCategory::from_str(&o.category).ok(),
        }
    }

    fn id(&self) -> &str {
        match self {
            WearableEntry::Base(_, m) => &m.id,
            WearableEntry::Owned(o) => &o.urn,
        }
    }

    fn name(&self) -> &str {
        match self {
            WearableEntry::Base(_, m) => m.id.rsplit_once(':').map(|(_, end)| end).unwrap_or(&m.id),
            WearableEntry::Owned(o) => &o.name,
        }
    }

    fn urn(&self) -> Urn {
        match self {
            WearableEntry::Base(urn, _) => urn.clone(),
            WearableEntry::Owned(o) => Urn::from_str(&o.urn).unwrap(),
        }
    }

    fn time(&self) -> i64 {
        match self {
            WearableEntry::Base(_, _) => 0,
            WearableEntry::Owned(o) => o
                .individual_data
                .first()
                .and_then(|t| t.transferred_at.parse::<i64>().ok())
                .unwrap_or_default(),
        }
    }

    fn rarity(&self) -> Rarity {
        match self {
            WearableEntry::Base(_, _) => Rarity::Free,
            WearableEntry::Owned(o) => o.rarity.as_str().into(),
        }
    }

    // fn collection(&self) -> &str {
    //     match self {
    //         WearableEntry::Base(..) => "Decentraland",
    //         WearableEntry::Owned(o) => {
    //             println!("{} -> {}", o.urn, o.urn.split(':').nth(4).unwrap_or_default());
    //             o.urn.split(':').nth(4).unwrap_or_default()
    //         },
    //     }
    // }
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
        let color = self.as_rgba_u8();
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color[0], color[1], color[2], color[3]
        )
    }
}

impl Rarity {
    pub fn color(&self) -> Color {
        match self {
            Rarity::Free => Color::rgb(0.9, 0.9, 0.9),
            Rarity::Common => Color::rgb(0.7, 0.7, 0.7),
            Rarity::Uncommon => Color::rgb(1.0, 0.8, 0.4),
            Rarity::Rare => Color::rgb(0.6, 1.0, 0.6),
            Rarity::Epic => Color::rgb(0.6, 0.6, 1.0),
            Rarity::Legendary => Color::rgb(0.8, 0.4, 0.8),
            Rarity::Mythic => Color::rgb(1.0, 0.6, 1.0),
            Rarity::Unique => Color::rgb(1.0, 1.0, 0.4),
        }
    }

    fn hex_color(&self) -> String {
        self.color().to_hex_color()
    }
}

fn update_wearables_list(
    mut commands: Commands,
    dialog: Query<Ref<SettingsDialog>>,
    mut q: Query<(&mut WearablesSettings, &DuiEntities, &SelectItem), Changed<WearablesSettings>>,
    dui: Res<DuiRegistry>,
    wearable_pointers: Res<WearablePointers>,
    wearable_metas: Res<WearableMetas>,
    asset_server: Res<AssetServer>,
    // mut collections_box: Query<&mut ComboBox>,
) {
    let Ok((mut settings, components, selected)) = q.get_single_mut() else {
        return;
    };

    debug!("updating wearables here");

    let mut wearables = if settings.only_collectibles {
        Vec::default()
    } else {
        base_wearables()
            .into_iter()
            .map(|w| Urn::from_str(&w).unwrap())
            .filter_map(|urn| wearable_pointers.0.get(&urn).map(|p| (urn, p)))
            .filter_map(|(urn, p)| p.hash().map(|h| (urn, h)))
            .filter_map(|(urn, h)| wearable_metas.0.get(h).map(|m| (urn, m.clone())))
            .map(|(urn, meta)| WearableEntry::Base(urn, meta))
            .collect()
    };

    wearables.extend(
        settings
            .owned_wearables
            .iter()
            .cloned()
            .map(WearableEntry::Owned),
    );

    if let Some(category) = settings.category {
        wearables.retain(|w| w.category().as_ref() == Some(category));
    }

    if let Some(collection) = &settings.collection {
        wearables.retain(|w| w.id().contains(collection));
    }

    if let Some(search) = &settings.search_filter {
        wearables.retain(|w| w.name().contains(search));
    }

    match settings.sort_by {
        SortBy::Newest => wearables.sort_by_key(|w| -w.time()),
        SortBy::Oldest => wearables.sort_by_key(|w| w.time()),
        SortBy::Alphabetic => wearables.sort_by(|w, w2| w.name().cmp(w2.name())),
        SortBy::ReverseAlphabetic => wearables.sort_by(|w, w2| w2.name().cmp(w.name())),
        SortBy::Rarest => {
            wearables.sort_by_key(|w| w.rarity());
            wearables.reverse();
        }
        SortBy::ReverseRarest => wearables.sort_by_key(|w| w.rarity()),
        SortBy::Equipped => {
            let worn = settings
                .current_wearables
                .values()
                .map(|(urn, _)| urn.as_str())
                .collect::<HashSet<_>>();

            wearables.sort_by_key(|w| (!worn.contains(w.id()), -w.time()))
        }
    }

    let urns = wearables.iter().map(WearableEntry::urn).collect::<Vec<_>>();
    if urns
        == *settings
            .current_list
            .iter()
            .map(WearableEntry::urn)
            .collect::<Vec<_>>()
        && !dialog.get_single().map_or(false, |d| d.is_changed())
    {
        // wearables list matches and dialog has not changed (so current wearables have not changed)
        return;
    }

    settings.current_list = wearables.clone();

    // let collections = wearables.iter().map(WearableEntry::collection).map(ToOwned::to_owned).collect::<HashSet<_>>();
    // if settings.all_collections != collections {
    //     let mut collections_box = collections_box.single_mut();
    //     let current_selection = collections_box.selected().cloned();
    //     collections_box.options = collections.iter().cloned().collect::<Vec<_>>();
    //     collections_box.options.sort();
    //     collections_box.selected = current_selection.and_then(|sel| collections_box.options.iter().position(|i| i == &sel)).map(|ix| ix as isize).unwrap_or(-1);
    //     settings.all_collections = collections;
    // }

    let worn = settings
        .current_wearables
        .values()
        .map(|(urn, _)| urn.as_str())
        .collect::<HashSet<_>>();

    commands
        .entity(components.named("items"))
        .despawn_descendants();

    let mut initial = None;
    let buttons: Vec<_> = wearables
        .into_iter()
        .enumerate()
        .map(|(ix, wearable)| {
            if selected.0.as_ref().map(WearableEntry::urn) == Some(wearable.urn()) {
                initial = Some(ix);
            }
            let inactive_color = if worn.contains(wearable.id()) {
                Color::ORANGE
            } else {
                Color::rgba(0.0, 0.0, 0.0, 0.0)
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
                        background: Some(Color::RED),
                        ..Default::default()
                    }),
                    inactive: Some(InteractStyle {
                        background: Some(inactive_color),
                        ..Default::default()
                    }),
                    disabled: Some(InteractStyle {
                        background: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                image: Some(asset_server.load("images/backpack/wearable_item_bg.png")),
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
    wearable_pointers: Res<WearablePointers>,
    wearable_metas: Res<WearableMetas>,
    ipfas: IpfsAssetServer,
    dui: Res<DuiRegistry>,
    mut request_wearables: ResMut<RequestedWearables>,
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
            let urn = entry.urn();
            match &*state {
                WearableItemState::PendingMeta(ix) => {
                    let ix = *ix;
                    if let Some(pointer) = wearable_pointers.0.get(&urn) {
                        match pointer {
                            WearablePointerResult::Exists(h) => {
                                debug!("found {} -> {h}", entry.urn());
                                let meta = wearable_metas.0.get(h).unwrap();
                                let fits = meta
                                    .data
                                    .representations
                                    .iter()
                                    .any(|rep| rep.body_shapes.contains(&settings.body_shape))
                                    || meta.data.category == WearableCategory::BODY_SHAPE;

                                *state = WearableItemState::PendingImage(
                                    ipfas.load_content_file(&meta.thumbnail, h).unwrap(),
                                );

                                modified = true;

                                let Some(button_bg) = walker.walk(
                                    settings_ent,
                                    format!("items.tab {ix}.button-background"),
                                ) else {
                                    warn!("failed to find bg");
                                    continue;
                                };

                                commands.entity(button_bg).try_insert(Enabled(fits));
                            }
                            WearablePointerResult::Missing => {
                                warn!("failed to load wearable");
                                commands
                                    .entity(ent)
                                    .despawn_descendants()
                                    .remove::<WearableItemState>()
                                    .spawn_template(
                                        &dui,
                                        "wearable-item",
                                        DuiProps::new()
                                            .with_prop(
                                                "image",
                                                ipfas
                                                    .asset_server()
                                                    .load::<Image>("images/backback/empty.png"),
                                            )
                                            .with_prop("rarity-color", entry.rarity().hex_color()),
                                    )
                                    .unwrap();
                            }
                        }
                    } else {
                        request_wearables.0.insert(urn.clone());
                    }
                }
                WearableItemState::PendingImage(handle) => {
                    let Some(WearablePointerResult::Exists(h)) = wearable_pointers.0.get(&urn)
                    else {
                        panic!();
                    };
                    let meta = wearable_metas.0.get(h).unwrap();

                    let fits = meta
                        .data
                        .representations
                        .iter()
                        .any(|rep| rep.body_shapes.contains(&settings.body_shape))
                        || meta.data.category == WearableCategory::BODY_SHAPE;

                    let (image_color, rarity_color) = if fits {
                        (Color::WHITE.to_hex_color(), entry.rarity().hex_color())
                    } else {
                        (Color::BLACK.to_hex_color(), Color::DARK_GRAY.to_hex_color())
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
                                        .with_prop("image", handle.clone())
                                        .with_prop("rarity-color", rarity_color)
                                        .with_prop("image-color", image_color),
                                )
                                .unwrap();
                        }
                        bevy::asset::LoadState::Failed | bevy::asset::LoadState::NotLoaded => {
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
                                            "image",
                                            ipfas
                                                .asset_server()
                                                .load::<Image>("images/backback/empty.png"),
                                        )
                                        .with_prop("rarity-color", rarity_color)
                                        .with_prop("image-color", image_color),
                                )
                                .unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[derive(Event, Component, Clone)]
struct SelectItem(Option<WearableEntry>);

#[allow(clippy::too_many_arguments)]
fn update_selected_item(
    mut commands: Commands,
    mut e: EventReader<SelectItem>,
    settings: Query<(Entity, Ref<WearablesSettings>, &DuiEntities, &SelectItem)>,
    avatar: Query<&AvatarShape, With<SettingsDialog>>,
    dui: Res<DuiRegistry>,
    wearable_pointers: Res<WearablePointers>,
    wearable_metas: Res<WearableMetas>,
    ipfas: IpfsAssetServer,
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
        .map(|(urn, _)| urn.as_str())
        .collect::<HashSet<_>>();

    if let Some(sel) = current_selection {
        let Some(WearablePointerResult::Exists(h)) = wearable_pointers.0.get(&sel.urn()) else {
            *retry = true;
            return;
        };
        let meta = wearable_metas.0.get(h).unwrap();
        let category = meta.data.category;
        let urn = meta.id.clone();
        let hash = h.clone();
        let is_remove = worn.contains(sel.id());

        let label = if worn.contains(sel.id()) {
            "REMOVE"
        } else {
            "EQUIP"
        };

        let enabled = !(matches!(category, WearableCategory::BODY_SHAPE) && is_remove);

        let equip_action = On::<Click>::new(
            move |mut commands: Commands,
                  ipfas: IpfsAssetServer,
                  wearable_metas: Res<WearableMetas>,
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
                        .insert(category, (urn.clone(), hash.clone()))
                };

                let Ok((mut dialog, instance, mut avatar)) = dialog.get_single_mut() else {
                    warn!("fail to update dialog+booth instance");
                    return;
                };

                // mark profile as modified
                dialog.modified = true;

                // update wearables on avatar
                let mut wearables = avatar.0.wearables.drain(..).collect::<HashSet<_>>();
                if let Some((old_urn, _)) = prev {
                    if !wearables.remove(&old_urn) {
                        warn!("failed to remove {old_urn} from {wearables:?}");
                    }
                }
                match category {
                    WearableCategory::BODY_SHAPE => {
                        avatar.0.body_shape = Some(urn.clone());
                        wearable_settings.body_shape = urn.clone();
                    }
                    _ => {
                        if !is_remove {
                            wearables.insert(urn.clone());
                        }
                    }
                }
                avatar.0.wearables = wearables.into_iter().collect();
                // and photobooth
                booth.update_shape(instance, avatar.clone());

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
                    .and_then(|(_, hash)| wearable_metas.0.get(hash).map(|m| (m, hash)))
                    .map(|(meta, hash)| ipfas.load_content_file(&meta.thumbnail, hash).unwrap())
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
                    Color::from(avatar.0.hair_color.unwrap_or_default()),
                )
            }
            WearableCategory::EYES => (
                "flex".to_owned(),
                Color::from(avatar.0.eye_color.unwrap_or_default()),
            ),
            WearableCategory::BODY_SHAPE => (
                "flex".to_owned(),
                Color::from(avatar.0.skin_color.unwrap_or_default()),
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
                    | WearableCategory::HAIR => &mut avatar.0.hair_color,
                    WearableCategory::EYES => &mut avatar.0.eye_color,
                    WearableCategory::BODY_SHAPE => &mut avatar.0.skin_color,
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
                    .with_prop("rarity-color", sel.rarity().hex_color())
                    .with_prop(
                        "selection-image",
                        ipfas
                            .load_content_file::<Image>(&meta.thumbnail, h)
                            .unwrap(),
                    )
                    .with_prop("title", meta.name.clone())
                    .with_prop("body", meta.description.clone())
                    .with_prop("label", label.to_owned())
                    .with_prop("enabled", enabled)
                    .with_prop("onclick", equip_action)
                    .with_prop("color-picker-display", picker_display)
                    .with_prop("color", color)
                    .with_prop("color-changed", color_picker_changed),
            )
            .unwrap();

        let repr = meta
            .data
            .representations
            .iter()
            .find(|repr| repr.body_shapes.contains(&settings.body_shape));
        if let Some(repr) = repr {
            for hides in &repr.override_hides {
                let Ok(category) = WearableCategory::from_str(hides) else {
                    warn!("unrecognised hide category {hides}");
                    continue;
                };
                let child = commands
                    .spawn_template(
                        &dui,
                        "wearable-hides",
                        DuiProps::new().with_prop(
                            "image",
                            format!("images/backpack/wearable_categories/{}.png", category.slot),
                        ),
                    )
                    .unwrap()
                    .root;
                commands
                    .entity(components.named("hides"))
                    .push_children(&[child]);
            }
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
