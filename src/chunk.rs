use std::{
    fs::{create_dir, File},
    io::{Read, Write},
    path::Path,
};

use anyhow::Context;
use bevy::{
    ecs::world::CommandQueue,
    prelude::*,
    tasks::{
        block_on,
        futures_lite::{future, FutureExt},
        ComputeTaskPool, IoTaskPool, Task,
    },
};
use bevy_ecs_tilemap::{
    map::{
        TilemapGridSize, TilemapId, TilemapRenderSettings, TilemapSize, TilemapSpacing,
        TilemapTexture, TilemapTileSize, TilemapType,
    },
    prelude::StandardTilemapMaterial,
    tiles::{TileColor, TileFlip, TilePos, TilePosOld, TileStorage, TileTextureIndex, TileVisible},
    FrustumCulling,
};
use bevy_rand::prelude::ForkableRng;
use bevy_rand::prelude::{GlobalEntropy, WyRand};
use bevy_replicon::{
    client::ClientSet,
    core::Replicated,
    prelude::{server_running, ParentSync},
};
use serde::{Deserialize, Serialize};

use crate::{player::Player, world::Ground};

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

pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ViewDistance::default())
            .observe(load_chunk_observer)
            .observe(save_chunk_observer)
            .add_systems(Startup, init_save_folder)
            .add_systems(
                PreUpdate,
                task_poll
                    .run_if(server_running)
                    .after(ClientSet::SyncHierarchy),
            )
            .add_systems(
                PreUpdate,
                (
                    load_deload_chunks
                        .run_if(server_running)
                        .after(ClientSet::SyncHierarchy),
                    init_chunk.after(ClientSet::Receive),
                )
                    .chain(),
            )
            .add_event::<LoadChunk>()
            .add_event::<SaveChunk>();
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChunkData {
    chunk_index: IVec2,
    tiles: Vec<TileData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TileData {
    tile_index: UVec2,
    ground: Ground,
}

#[derive(Debug, Resource)]
struct ViewDistance(f32);
impl Default for ViewDistance {
    fn default() -> Self {
        Self(20.0)
    }
}

fn init_save_folder() {
    create_dir("world");
}

fn load_chunk_observer(trigger: Trigger<LoadChunk>, mut commands: Commands) {
    let index = trigger.event().index;
    let task = IoTaskPool::get().spawn(load_chunk(index));
    commands.spawn(ComputeTask(index, task));
}

async fn load_chunk(index: IVec2) -> CommandQueue {
    let mut bytes = Vec::<u8>::new();
    let mut command_queue = CommandQueue::default();

    match File::open(&format!("world/{}_{}.ron", index.x, index.y))
        .and_then(|mut path| path.read_to_end(&mut bytes))
        .context("Failed reading the file")
        .and_then(|_| ron::de::from_bytes::<ChunkData>(&bytes).context("Failed deserialisation"))
    {
        Ok(chunk_data) => {
            command_queue.push(move |world: &mut World| {
                spawn_chunk_stub(&mut world.commands(), chunk_data);
            });
        }
        Err(err) => {
            error!("{err}");
        }
    }
    return command_queue;
}

fn save_chunk_observer(
    trigger: Trigger<SaveChunk>,
    chunks_q: Query<(&Chunk, &Children)>,
    tiles_q: Query<(&TilePos, &Ground)>,
) {
    let index = trigger.event().index;
    let chunk_data = extract_chunk_data(index, chunks_q, tiles_q);
    IoTaskPool::get()
        .spawn(save_chunk(index, chunk_data))
        .detach();
}

async fn save_chunk(index: IVec2, chunk_data: ChunkData) {
    match ron::to_string(&chunk_data)
        .context("Failed serialisation")
        .and_then(|serialized| {
            File::create(&format!("world/{}_{}.ron", index.x, index.y))
                .context("Failed file creation")
                .and_then(|mut path| {
                    path.write(serialized.as_bytes())
                        .context("Error while writing chunk data to file")
                })
        }) {
        Ok(_) => {}
        Err(err) => error!("{err}"),
    }
}

fn extract_chunk_data(
    index: IVec2,
    chunks_q: Query<(&Chunk, &Children)>,
    tiles_q: Query<(&TilePos, &Ground)>,
) -> ChunkData {
    let (_chunk, children) = chunks_q
        .iter()
        .find(|(x, _)| x.chunk_index == index)
        .expect("Chunk to save does not exist!");

    let tile_data: Vec<TileData> = children
        .iter()
        .map(|&e| tiles_q.get(e))
        .filter_map(|x| x.ok())
        .map(|x| TileData {
            tile_index: UVec2::new(x.0.x, x.0.y),
            ground: x.1.clone(),
        })
        .collect();

    ChunkData {
        chunk_index: index,
        tiles: tile_data,
    }
}

fn task_poll(mut commands: Commands, mut tasks_q: Query<(Entity, &mut ComputeTask)>) {
    for (entity, mut task) in &mut tasks_q {
        if let Some(mut commands_queue) = block_on(future::poll_once(&mut task.1)) {
            // append the returned command queue to have it execute later
            commands.append(&mut commands_queue);
            commands.entity(entity).despawn_recursive();
        }
    }
}

#[derive(Event)]
pub struct LoadChunk {
    pub index: IVec2,
}

#[derive(Event)]
pub struct SaveChunk {
    pub index: IVec2,
}

#[derive(Component)]
pub struct ComputeTask(pub IVec2, pub Task<CommandQueue>);

fn spawn_chunk_stub(commands: &mut Commands, chunk_data: ChunkData) {
    let mut tile_storage = TileStorage::empty(MAP_SIZE);
    let tilemap_entity = commands
        .spawn((
            Chunk {
                chunk_index: chunk_data.chunk_index,
            },
            Replicated,
        ))
        .with_children(|parent| {
            for tile_data in chunk_data.tiles {
                let tile_pos: TilePos = tile_data.tile_index.into();
                let ground = tile_data.ground;
                let tile_entity = parent
                    .spawn((tile_pos, ground, Replicated, ParentSync::default()))
                    .id();
                tile_storage.set(&tile_pos, tile_entity);
            }
        })
        .remove::<ComputeTask>()
        .id();
    commands.entity(tilemap_entity).insert(tile_storage);
}

fn load_deload_chunks(
    mut commands: Commands,
    chunk_query: Query<(Entity, &Chunk)>,
    loading_tasks_query: Query<(&ComputeTask)>,
    player_query: Query<&Transform, With<Player>>,
    view_distance: Res<ViewDistance>,
) {
    //collect all chunks that are visible and therefore should be loaded
    let mut visible_chunk_indices: Vec<IVec2> = player_query
        .iter()
        .flat_map(|player_transform| {
            let view_border = Rect::from_center_size(
                player_transform.translation.xy(),
                Vec2::splat(view_distance.0 * GRID_SIZE.x),
            );
            chunk_indices_inside(view_border)
        }).collect();

    for (entity, chunk) in chunk_query.iter() {
        let pos = visible_chunk_indices
            .iter()
            .position(|&x| x == chunk.chunk_index);
        match pos {
            Some(pos) => {
                // remove visible chunks that are already spawned from the list
                visible_chunk_indices.swap_remove(pos);
            }
            None => {
                // save and despawn chunks that are not visible
                commands.trigger(SaveChunk {
                    index: chunk.chunk_index,
                });
                commands.entity(entity).despawn_recursive();
            }
        }
    }

    //spawning chunks that are visible but not yet spawned
    for chunk_to_spawn in visible_chunk_indices {
        if Path::new(&format!(
            "world/{}_{}.ron",
            chunk_to_spawn.x, chunk_to_spawn.y
        ))
        .exists()
        {
            //trigger load if the chunk has a save file
            commands.trigger(LoadChunk {
                index: chunk_to_spawn,
            });
        } else {
            //generate new chunk if it wasn't visited before
            spawn_chunk_stub(&mut commands, gen_chunk(chunk_to_spawn));
        }
    }
}

fn gen_chunk(index: IVec2) -> ChunkData {
    let mut tile_data: Vec<TileData> = Vec::new();
    for x in 0..TILES_PER_CHUNK {
        for y in 0..TILES_PER_CHUNK {
            tile_data.push(TileData {
                tile_index: UVec2::new(x, y),
                ground: Ground::Grass,
            });
        }
    }
    ChunkData {
        chunk_index: index,
        tiles: tile_data,
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
#[derive(Bundle, Debug, Default, Clone)]
pub struct RenderTilemapBundle {
    pub grid_size: TilemapGridSize,
    pub map_type: TilemapType,
    pub size: TilemapSize,
    pub spacing: TilemapSpacing,
    pub texture: TilemapTexture,
    pub tile_size: TilemapTileSize,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub render_settings: TilemapRenderSettings,
    /// User indication of whether an entity is visible
    pub visibility: Visibility,
    /// Algorithmically-computed indication of whether an entity is visible and should be extracted
    /// for rendering
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
    /// User indication of whether tilemap should be frustum culled.
    pub frustum_culling: FrustumCulling,
    pub material: Handle<StandardTilemapMaterial>,
}
