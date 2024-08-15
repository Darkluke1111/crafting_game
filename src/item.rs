use bevy::{asset::AssetServer, math::Vec2, prelude::*, sprite::{Sprite, SpriteBundle}, utils::HashMap, };
use bevy_replicon::{core::Replicated, prelude::{has_authority, AppRuleExt, FromClient}};
use bevy_replicon_snap::NetworkOwner;
use serde::{Deserialize, Serialize};

use crate::ActionEvent;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {

        let mut item_registry = ItemRegistry::new();
        item_registry.register(
            Item::new("Bread", "bread", 1)
        );
        item_registry.register(
            Item::new("Cheese", "cheese", 2)
        );

        app
            .insert_resource(item_registry)
            .add_systems(PreUpdate, init_grounditems)
            .add_systems(Update, apply_action.map(Option::unwrap).run_if(has_authority))
            .replicate::<Item>();
    }
}

#[derive(Event)]
struct ItemDropEvent {
    position: Vec2,
    item: Item
}


#[derive(Bundle)]
struct GroundItemBundle {
    sprite: SpriteBundle,
    item: Item,
}

#[derive(Component, Serialize, Deserialize, Debug, Clone)]
struct Item {
    name: String,
    id: String,
    texture_index: usize,
}

impl Item {
    fn new(name: &str, id: &str, texture_index: usize) -> Self{
        Self { name: name.to_string(), id: id.to_string(), texture_index: texture_index }
    }
}

#[derive(Resource)]
struct ItemRegistry {
    items: HashMap<String,Item>,
    default_item: Item,
}

impl ItemRegistry {
    fn new() -> Self {
        Self {
            items: HashMap::new(),
            default_item: Item::new("Default", "default", 0),
        }
    }

    fn register(&mut self, item: Item) {
        self.items.insert(item.id.clone(), item);
    }

    fn get(&self, id: &str) -> &Item {
        self.items.get(id).unwrap_or(&self.default_item)
    }
}

fn init_grounditems(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    query: Query<(Entity, &Item), (Without<Sprite>)>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load::<Image>("rpgitems.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(24), 16, 16, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    for (entity, item ) in query.iter() {
        commands.entity(entity).insert((
            Sprite::default(),
            TextureAtlas {
                layout: texture_atlas_layout.clone(),
                index: item.texture_index,
            },
            texture.clone(),
            VisibilityBundle::default(),
            GlobalTransform::default(),
        ));
    }
}

fn spawn_ground_item(
    commands: &mut Commands,
    item: &Item,
    position: Vec2,
) {
    commands.spawn((
        Name::new("Item"),
        Transform::from_translation(position.extend(1.0)),
        item.clone(),
        Replicated,
    ));
}

fn apply_action(
    mut commands: Commands,
    player_query: Query<(&NetworkOwner, &Transform)>,
    mut events: EventReader<FromClient<ActionEvent>>,
    item_registry: Res<ItemRegistry>,
) -> Option<()>{
    for FromClient { client_id, .. } in events.read() {
        if let Some((_,t)) = player_query.iter().find(|p| p.0.0 == client_id.get()) {
            spawn_ground_item(&mut commands, item_registry.get("bread"), t.translation.xy())
        }
    }
    Some(())
}

