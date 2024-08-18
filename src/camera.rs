use bevy::{color::palettes::css::GREEN, math::VectorSpace, prelude::*};
use bevy_replicon::{
    core::ClientId,
    prelude::{client_connected, RepliconClient},
};
use bevy_replicon_renet::renet::RenetClient;
use bevy_replicon_snap::NetworkOwner;

pub struct CameraPlugin;

const CAMERA_HEIGHT: f32 = 10.0;


#[derive(Debug, Component)]
struct MainCamera;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .insert_resource(CameraView::default())
            .add_systems(Update, (update_camera.run_if(client_connected), update_camera_view).chain())
            .add_systems(Update, draw_camera_gizmo);
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2dBundle {
        transform: Transform::from_translation(Vec3::new(0.0, 0.0, CAMERA_HEIGHT)),
        projection: OrthographicProjection {
            scale: 0.5,
            near: -1000.0,
            far: 1000.0,
            ..Default::default()
        },

        ..Default::default()
    }, MainCamera));
}

#[derive(Debug, Resource, Default)]
pub struct CameraView(pub Rect);


fn get_camera_view(
    camera_transform: &GlobalTransform,
    window: &Window,
    camera: &Camera,

) -> Option<Rect> {
    let corner1 = camera.viewport_to_world_2d(camera_transform, Vec2 { x: window.size().x, y: 0.0 })?;
    let corner2 = camera.viewport_to_world_2d(camera_transform, Vec2 { x: 0.0 , y: window.size().y})?;
    Some(Rect::from_corners( corner1, corner2))
}

fn draw_camera_gizmo(
    camera_view: Res<CameraView>,
    mut gizmos: Gizmos,
) {
    let view = camera_view.0;
    gizmos.rect_2d(view.center(), 0.0, view.size(), GREEN);
}

fn update_camera(
    player_query: Query<(&Transform, &NetworkOwner)>,
    mut camera: Query<&mut Transform, (With<MainCamera>, Without<NetworkOwner>)>,
    client: Res<RepliconClient>,
) {
    if let Some(client_id) = client.id() {
        if let Some((t, _)) = player_query
            .iter()
            .find(|(_, nw)| ClientId::new(nw.0) == client_id)
        {
            let mut camera_transform = camera.single_mut();
            camera_transform.translation = camera_transform.translation.lerp(t.translation.xy().extend(CAMERA_HEIGHT), 0.01);
        }
    }
}

fn update_camera_view(
    camera_query: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
    window_query: Query<&Window>,
    mut camera_view : ResMut<CameraView>,
) {
    let (cam, proj) = camera_query.single();
    let win = window_query.single();

    match get_camera_view(cam, win, proj) {
        Some(view) => camera_view.0 = view,
        None => {}
    }
    
}
