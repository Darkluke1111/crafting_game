use bevy::{asset::AssetServer, math::Vec2, prelude::*, sprite::{Sprite, SpriteBundle}, };
use bevy_replicon::prelude::*;
use bevy_replicon_snap::NetworkOwner;
use serde::{Deserialize, Serialize};

use crate::ActionEvent;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {

        app
            .add_systems(PreUpdate, init_grounditems)
            .add_systems(Update, apply_action.map(Option::unwrap).run_if(has_authority))
            .replicate::<Item>();
    }
}


#[derive(Bundle)]
struct GroundItemBundle {
    sprite: SpriteBundle,
    item: Item,
}

#[derive(Component, Serialize, Deserialize, Debug, Clone, Reflect)]
pub struct Item {
    pub name: String,
    pub id: String,
    pub texture_index: usize,
}

impl Item {
    pub fn new(name: &str, id: &str, texture_index: usize) -> Self{
        Self { name: name.to_string(), id: id.to_string(), texture_index: texture_index }
    }
}

fn init_grounditems(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    query: Query<(Entity, &Item), Without<Sprite>>,
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
) -> Option<()>{
    for FromClient { client_id, event } in events.read() {
        if event.action != KeyCode::Space {continue;}
        if let Some((_,t)) = player_query.iter().find(|p| p.0.0 == client_id.get()) {
            spawn_ground_item(&mut commands, &Item::new("Bread", "bread", 1), t.translation.xy());
        }
    }
    Some(())
}

