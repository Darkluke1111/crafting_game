use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_replicon::{client::ClientSet, core::Replicated, prelude::AppRuleExt};
use serde::{Deserialize, Serialize};





pub struct WorldObjectPlugin;

impl Plugin for WorldObjectPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app
            .replicate::<WorldObject>()
            .add_systems(Update, init_world_objets.after(ClientSet::Receive));
    }
}


#[derive(Debug, Component, Serialize, Deserialize)]
struct WorldObject;


pub fn spawn_world_object(
    commands: &mut Commands,
    position: Vec2,
) {
    commands.spawn((
        Name::new("World_Object"),
        Transform::from_translation(position.extend(1.0)),
        WorldObject,
        Replicated,
    ));
}


fn init_world_objets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    query: Query<(Entity, &WorldObject), Without<Sprite>>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load::<Image>("TX Props.png");
    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(512, 512));
    layout.add_texture(URect::new(162, 16, 190, 64));
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    for (entity, item ) in query.iter() {
        commands.entity(entity).insert((
            Sprite::default(),
            TextureAtlas {
                layout: texture_atlas_layout.clone(),
                index: 0,
            },
            texture.clone(),
            VisibilityBundle::default(),
            GlobalTransform::default(),
            Collider::ball(50.0),
            Restitution::coefficient(0.7),
        ));
    }
}


