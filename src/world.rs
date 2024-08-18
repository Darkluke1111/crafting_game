use bevy::{
    color::palettes::css::{RED, YELLOW},
    prelude::*,
};
use bevy_ecs_tilemap::{
    map::{TilemapId, TilemapSize, TilemapTexture, TilemapTileSize, TilemapType}, prelude::*, tiles::{TileBundle, TilePos, TileStorage}, FrustumCulling
};
use bevy_mod_picking::{
    events::{Click, Pointer},
    prelude::On,
};
use bevy_rand::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_snap::NetworkOwner;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};

use crate::{player::Player, ActionEvent};

const TILES_PER_CHUNK: u32 = 4;
const TILE_LENGTH: f32 = 32.0;

const MAP_SIZE: TilemapSize = TilemapSize {
    x: TILES_PER_CHUNK,
    y: TILES_PER_CHUNK,
};
const TILE_SIZE: TilemapTileSize = TilemapTileSize {
    x: TILE_LENGTH,
    y: TILE_LENGTH,
};
const GRID_SIZE: TilemapGridSize = TilemapGridSize {
    x: TILE_LENGTH,
    y: TILE_LENGTH,
};

#[derive(Debug, Resource)]
struct ViewDistance(f32);
impl Default for ViewDistance {
    fn default() -> Self {
        Self(10.0)
    }
}

fn spawn_chunk_stub(commands: &mut Commands, chunk_index: IVec2, glob: &mut GlobalEntropy<WyRand>) {
    let tilemap_entity = commands.spawn_empty().id();
    let mut tile_storage = TileStorage::empty(MAP_SIZE);
    commands
        .entity(tilemap_entity)
        .insert((Replicated, Chunk { chunk_index }, Name::new("Chunk")))
        .with_children(|parent| {
            for x in 0..MAP_SIZE.x {
                for y in 0..MAP_SIZE.y {
                    let tile_pos = TilePos { x, y };
                    let ground = Ground::Grass;
                    let tile_entity = parent
                        .spawn((
                            TileBundle {
                                position: tile_pos,
                                tilemap_id: TilemapId(tilemap_entity),
                                ..Default::default()
                            },
                            Replicated,
                            ground,
                            Name::new("Tile"),
                            ParentSync::default(),
                            glob.fork_rng(),
                            On::<Pointer<Click>>::target_commands_mut(|_click, target_commands| {
                                dbg!("Clicked me!");
                            }),
                        ))
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
    player_query: Query<&Transform, With<Player>>,
    view_distance: Res<ViewDistance>,
    mut glob: ResMut<GlobalEntropy<WyRand>>,
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
            commands.entity(entity).despawn_recursive();
        } else {
            let pos = allowed_chunk_indices
                .iter()
                .position(|x| *x == chunk.chunk_index)
                .expect("Couldn't find chunk in allowed indices!");
            allowed_chunk_indices.swap_remove(pos);
        }
    }
    for chunk_to_spawn in allowed_chunk_indices {
        spawn_chunk_stub(&mut commands, chunk_to_spawn, &mut glob);
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
    query: Query<(Entity, &Chunk), Without<TilemapGridSize>>,
    asset_server: Res<AssetServer>,
) {
    let texture_handle: Handle<Image> = asset_server.load("TX Tileset Grass.png");
    let map_type = TilemapType::default();
    for (entity, chunk) in query.iter() {
        commands.entity(entity).insert(RenderTilemapBundle {
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
    );
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

fn detect_tile_click(mut click_events: EventReader<Pointer<Click>>) {
    for click in click_events.read() {
        dbg!("Click!");
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

#[derive(Component, Serialize, Deserialize)]
pub struct Chunk {
    chunk_index: IVec2,
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

#[derive(Component, Serialize, Deserialize)]
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
            .replicate::<TilePos>()
            .replicate::<Ground>()
            .replicate::<Chunk>()
            .replicate::<TileVisible>()
            .replicate::<TileFlip>()
            .replicate::<TileTextureIndex>()
            .replicate::<TileColor>()
            .replicate::<TilePosOld>()
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
                    detect_tile_click,
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
