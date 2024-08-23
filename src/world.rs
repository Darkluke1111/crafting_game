use std::{fs::File, io::{Read, Write}, path::Path};

use bevy::{
    asset::AssetPath, color::palettes::css::{RED, YELLOW}, ecs::{reflect, world::CommandQueue}, prelude::*, scene::{ron, serde::SceneDeserializer}, tasks::{block_on, futures_lite::future, IoTaskPool, Task}, utils::dbg
};
use bevy_ecs_tilemap::{
    map::{TilemapId, TilemapSize, TilemapTexture, TilemapTileSize, TilemapType},
    prelude::*,
    tiles::{TileBundle, TilePos, TileStorage},
    FrustumCulling,
};
use bevy_inspector_egui::egui::load;
use bevy_mod_picking::{
    events::{Click, Pointer},
    prelude::On,
};
use bevy_rand::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_snap::NetworkOwner;
use rand_core::RngCore;
use serde::{de::DeserializeSeed, Deserialize, Deserializer, Serialize};

use crate::{chunk::{ComputeTask, LoadChunk, SaveChunk}, player::Player, ActionEvent, ClickTileEvent};

pub const TILES_PER_CHUNK: u32 = 8;
pub const TILE_LENGTH: f32 = 32.0;

pub const MAP_SIZE: TilemapSize = TilemapSize {
    x: TILES_PER_CHUNK,
    y: TILES_PER_CHUNK,
};
pub const TILE_SIZE: TilemapTileSize = TilemapTileSize {
    x: TILE_LENGTH,
    y: TILE_LENGTH,
};
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize {
    x: TILE_LENGTH,
    y: TILE_LENGTH,
};

#[derive(Debug, Resource)]
struct ViewDistance(f32);
impl Default for ViewDistance {
    fn default() -> Self {
        Self(20.0)
    }
}

fn spawn_chunk_stub(commands: &mut Commands, chunk_index: IVec2) {

    let tilemap_entity = commands.spawn_empty().id();
    let mut tile_storage = TileStorage::empty(MAP_SIZE);
    commands
        .entity(tilemap_entity)
        .insert((Replicated, Chunk { chunk_index }))
        .with_children(|parent| {
            for x in 0..MAP_SIZE.x {
                for y in 0..MAP_SIZE.y {
                    let tile_pos = TilePos { x, y };
                    let ground = Ground::Grass;
                    let tile_entity = parent
                        .spawn((tile_pos, Replicated, ground, ParentSync::default()))
                        .id();
                    tile_storage.set(&tile_pos, tile_entity);
                }
            }
        });
    commands.entity(tilemap_entity).insert(tile_storage);
}

fn manage_loaded_chunks(
    mut commands: Commands,
    chunk_query: Query<(Entity, &Chunk)>,
    loading_tasks_query: Query<(&ComputeTask)>,
    player_query: Query<&Transform, With<Player>>,
    view_distance: Res<ViewDistance>,
) {
    let mut allowed_chunk_indices = Vec::new();
    for player_transform in player_query.iter() {
        let view_border = Rect::from_center_size(
            player_transform.translation.xy(),
            Vec2::splat(view_distance.0 * GRID_SIZE.x),
        );
        allowed_chunk_indices.append(&mut chunk_indices_inside(view_border));
    }
    for (entity, chunk) in chunk_query.iter() {
        if !allowed_chunk_indices.contains(&chunk.chunk_index) {
            commands.trigger(SaveChunk {index: chunk.chunk_index});

            commands.entity(entity).despawn_recursive();
        } else {
            let pos = allowed_chunk_indices
                .iter()
                .position(|x| *x == chunk.chunk_index)
                .expect("Couldn't find chunk in allowed indices!");
            allowed_chunk_indices.swap_remove(pos);
        }
    }
    for loading_task in loading_tasks_query.iter() {
        let Some(pos) = allowed_chunk_indices
                .iter()
                .position(|x| *x == loading_task.0) else {continue;};
        allowed_chunk_indices.swap_remove(pos);
    }
    for chunk_to_spawn in allowed_chunk_indices {
        if Path::new(&format!("world/{}_{}.ron", chunk_to_spawn.x, chunk_to_spawn.y)).exists() {
            commands.trigger(LoadChunk {index: chunk_to_spawn});
        } else {
            spawn_chunk_stub(&mut commands, chunk_to_spawn);
        }
        
    }
}

fn update_ground_texture(
    mut tile_query: Query<
        (
            &mut TileTextureIndex,
            &Ground,
            &mut EntropyComponent<WyRand>,
        ),
        Changed<Ground>,
    >,
) {
    for (mut texture_index, ground, mut rnd) in tile_query.iter_mut() {
        match ground {
            Ground::Grass => {
                texture_index.0 = rnd.next_u32() % 32;
            }
            Ground::Dirt => {
                texture_index.0 = 32;
            }
            _ => {
                texture_index.0 = 4;
            }
        }
    }
}

fn init_chunk(
    mut commands: Commands,
    chunks_q: Query<(Entity, &Chunk, &Children), Without<TilemapGridSize>>,
    asset_server: Res<AssetServer>,
    mut glob: ResMut<GlobalEntropy<WyRand>>,
) {
    let texture_handle: Handle<Image> = asset_server.load("TX Tileset Grass.png");
    let map_type = TilemapType::default();
    for (entity, chunk, children) in chunks_q.iter() {
        commands.entity(entity).insert((
            Name::new("Chunk"),
            RenderTilemapBundle {
                grid_size: GRID_SIZE,
                map_type,
                size: MAP_SIZE,
                texture: TilemapTexture::Single(texture_handle.clone()),
                transform: Transform::from_translation(
                    chunk.get_world_coords().extend(0.0)
                        + Vec3::new(TILE_LENGTH, TILE_LENGTH, 0.0) * 0.5,
                ),
                tile_size: TILE_SIZE,

                ..Default::default()
            },
        ));

        for child in children {
            commands.entity(*child).insert((
                Name::new("Tile"),
                TileTextureIndex::default(),
                TilemapId(entity),
                TileVisible::default(),
                TileFlip::default(),
                TileColor::default(),
                TilePosOld::default(),
                glob.fork_rng(),
            ));
        }
    }
}

fn apply_action(
    mut tile_query: Query<(&TilePos, &mut Ground)>,
    player_query: Query<(&NetworkOwner, &Transform)>,
    mut events: EventReader<FromClient<ActionEvent>>,
) -> Option<()> {
    for FromClient { client_id, event } in events.read() {
        if event.action != KeyCode::Space {
            continue;
        }
        if let Some((_, t)) = player_query.iter().find(|p| p.0 .0 == client_id.get()) {
            let tile_pos = TilePos::from_world_pos(
                &t.translation.xy(),
                &MAP_SIZE,
                &GRID_SIZE,
                &TilemapType::Square,
            )?;
            let (_pos, mut ground) = tile_query.iter_mut().find(|(pos, _)| pos == &&tile_pos)?;
            *ground = Ground::Dirt;
        }
    }
    Some(())
}

fn detect_tile_click(
    mut click_events: EventReader<Pointer<Click>>,
    tiles: Query<&TilePos>,
    mut writer: EventWriter<ClickTileEvent>,
) {
    for click in click_events.read() {
        let Some(tile_pos) = tiles.get(click.target).ok() else {
            continue;
        };
        dbg!(tile_pos);
        writer.send(ClickTileEvent { tile: click.target });
    }
}

fn handle_tile_click(
    mut reader: EventReader<FromClient<ClickTileEvent>>,
    mut tiles: Query<(&mut Ground), With<TilePos>>,
) {
    for FromClient {
        client_id,
        event: ClickTileEvent { tile },
    } in reader.read()
    {
        match tiles.get_mut(*tile) {
            Ok(mut ground) => *ground = Ground::Dirt,
            Err(_) => {}
        }
    }
}

fn debug_draw_chunk_borders(chunk_query: Query<&Chunk>, mut gizmos: Gizmos) {
    for chunk in chunk_query.iter() {
        let pos = chunk.get_world_coords();
        gizmos.circle_2d(pos, 1.0, RED);
        gizmos.rect_2d(
            chunk.get_world_coords() + chunk.get_size() * 0.5,
            0.0,
            chunk.get_size(),
            RED,
        );
    }
}

fn debug_draw_tile_borders(
    chunk_query: Query<&Chunk>,
    tile_query: Query<(&TilePos, &Parent)>,
    mut gizmos: Gizmos,
) {
    for (tile, parent) in tile_query.iter() {
        let chunk = chunk_query.get(**parent).unwrap();
        let tile_pos = Vec2::new(tile.x as f32 * TILE_LENGTH, tile.y as f32 * TILE_LENGTH);
        let pos = chunk.get_world_coords() + tile_pos;
        let tile_size: Vec2 = GRID_SIZE.into();
        gizmos.rect_2d(pos + tile_size * 0.5, 0.0, tile_size, YELLOW);
    }
}

#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Chunk {
    pub chunk_index: IVec2,
}

impl Chunk {
    pub fn get_world_coords(&self) -> Vec2 {
        let x = self.chunk_index.x as f32 * TILES_PER_CHUNK as f32 * TILE_LENGTH;
        let y = self.chunk_index.y as f32 * TILES_PER_CHUNK as f32 * TILE_LENGTH;
        Vec2 { x, y }
    }

    pub fn get_size(&self) -> Vec2 {
        Vec2::splat(TILES_PER_CHUNK as f32 * TILE_LENGTH)
    }
}

pub fn chunk_indices_inside(rect: Rect) -> Vec<IVec2> {
    let mut indices = Vec::new();
    let units_per_chunk = TILES_PER_CHUNK as i32 * TILE_LENGTH as i32;
    for x in (rect.min.x as i32) / units_per_chunk..(rect.max.x as i32) / units_per_chunk {
        for y in (rect.min.y as i32) / units_per_chunk..(rect.max.y as i32) / units_per_chunk {
            indices.push(IVec2 { x, y })
        }
    }
    return indices;
}

#[derive(Component, Debug, Reflect, Serialize, Deserialize, Clone)]
#[reflect(Component)]
pub enum Ground {
    Dirt,
    Grass,
    Stone,
    Water,
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin)
            .insert_resource(ViewDistance::default())
            .replicate_mapped::<TilemapId>()
            .replicate_mapped::<TileStorage>()
            .replicate::<TilePos>()
            .replicate::<Ground>()
            .replicate::<Chunk>()
            .replicate::<TileVisible>()
            .replicate::<TileFlip>()
            .replicate::<TileTextureIndex>()
            .replicate::<TileColor>()
            .replicate::<TilePosOld>()
            .register_type::<Chunk>()
            .register_type::<Ground>()
            .add_systems(
                PreUpdate,
                manage_loaded_chunks
                    .run_if(server_running)
                    .after(ClientSet::SyncHierarchy),
            )
            .add_systems(
                PreUpdate,
                init_chunk
                    .after(ClientSet::Receive)
                    .after(manage_loaded_chunks),
            )
            .add_systems(
                Update,
                (
                    debug_draw_chunk_borders,
                    debug_draw_tile_borders,
                    detect_tile_click.run_if(client_connected),
                    handle_tile_click.run_if(has_authority),
                ),
            )
            .add_systems(
                Update,
                (
                    apply_action.map(Option::unwrap).run_if(has_authority),
                    update_ground_texture,
                ),
            );
    }
}

pub trait ChunkPosExt {
    fn from_in_chunk_pos(pos: Vec2) -> Option<Self>
    where
        Self: Sized;
    fn get_in_chunk_pos(&self) -> Vec2;
}

impl ChunkPosExt for TilePos {
    fn from_in_chunk_pos(pos: Vec2) -> Option<Self> {
        let tile_pos = (pos / TILE_LENGTH).trunc();
        let tile_pos = TilePos::from_i32_pair(tile_pos.x as i32, tile_pos.y as i32, &MAP_SIZE);
        //dbg!(tile_pos);
        return tile_pos;
    }

    fn get_in_chunk_pos(&self) -> Vec2 {
        todo!()
    }
}