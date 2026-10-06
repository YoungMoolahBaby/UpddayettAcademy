//! Ray-traced lighting with Bevy Solari (feature `solari`, on by default).
//! Solari lights the room from emissive meshes (the fluorescent tubes, the
//! LEDs) and directional lights, with ray-traced shadows and bounce light.
//! Off by default (without DLSS denoising, moving things stay grainy): F2
//! toggles it, and `UPD_SOLARI=1` starts with it on.
//! `UPD_BENCH=1` measures both (vsync off) and saves `shots/rt_*.png`.

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::CameraMainTextureUsages;
use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::camera::{MipBias, TemporalJitter};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::solari::prelude::{RaytracingMesh3d, SolariLighting, SolariPlugins};

use super::scene::{BoardCam, MainCam, Tubes};

/// How much brighter the tubes glow under Solari, where they light the room
/// (`UPD_TUBES=<x>` overrides it).
const TUBE_BOOST: f32 = 20_000.0;

pub struct RtPlugin;

impl Plugin for RtPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((SolariPlugins, FrameTimeDiagnosticsPlugin::default()))
            .insert_resource(Rt { on: std::env::var("UPD_SOLARI").as_deref() == Ok("1"), supported: false })
            .add_systems(Update, (prepare_cameras, trace_meshes, toggle));
        if bench_enabled() {
            app.add_systems(Update, bench);
        }
    }
}

#[derive(Resource)]
pub struct Rt {
    /// Solari wanted (F2 flips it).
    pub on: bool,
    /// The GPU can ray trace (checked once the renderer is up).
    supported: bool,
}

pub fn bench_enabled() -> bool {
    std::env::var_os("UPD_BENCH").is_some()
}

/// Cameras that still need the deferred prepass.
type Unprepped = (Or<(With<MainCam>, With<BoardCam>)>, Without<DeferredPrepass>);
/// Meshes not yet sorted into traced or raster-only.
type Untraced = (Without<RaytracingMesh3d>, Without<RasterOnly>);

/// Both cameras render deferred under Solari (it sets the default opaque
/// method), so neither can multisample; the main one also needs its target
/// writable as a storage texture.
fn prepare_cameras(
    mut commands: Commands,
    unprepped: Query<Entity, Unprepped>,
    mut main: Query<(&mut Msaa, &mut CameraMainTextureUsages), With<MainCam>>,
    mut board: Query<&mut Msaa, (With<BoardCam>, Without<MainCam>)>,
    mut rt: ResMut<Rt>,
    device: Option<Res<RenderDevice>>,
    mut checked: Local<bool>,
) {
    if !*checked && let Some(device) = device {
        *checked = true;
        rt.supported = device.features().contains(SolariPlugins::required_wgpu_features());
        if !rt.supported {
            warn!("this GPU can't ray trace for Solari; staying on PBR");
            rt.on = false;
        }
    }
    // Solari makes every opaque material deferred, so PBR needs the deferred
    // prepass too (or it draws nothing).
    for cam in &unprepped {
        commands.entity(cam).insert((DepthPrepass, DeferredPrepass));
    }
    // Camera3d brings both components with defaults, so they're set in place.
    for (mut msaa, mut usages) in &mut main {
        if *msaa != Msaa::Off {
            *msaa = Msaa::Off;
        }
        if !usages.0.contains(TextureUsages::STORAGE_BINDING) {
            usages.0 |= TextureUsages::STORAGE_BINDING;
        }
    }
    for mut msaa in &mut board {
        if *msaa != Msaa::Off {
            *msaa = Msaa::Off;
        }
    }
}

/// A mesh Solari doesn't trace (see-through, or a shape it can't take).
#[derive(Component)]
pub struct RasterOnly;

/// Give every opaque standard-material mesh a ray-tracing twin, fixing up
/// the mesh the way Solari needs it (UVs, tangents, 32-bit indices, one UV set).
fn trace_meshes(
    mut commands: Commands,
    new: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>), Untraced>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<Assets<StandardMaterial>>,
) {
    for (e, mesh3d, mat) in &new {
        // Glass stays raster-only (Solari traces opaque surfaces), and so does
        // the unlit TV picture.
        if mats.get(&mat.0).is_none_or(|m| m.alpha_mode != AlphaMode::Opaque || m.unlit) {
            commands.entity(e).insert(RasterOnly);
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&mesh3d.0) else {
            continue;
        };
        let n = mesh.count_vertices();
        if !mesh.contains_attribute(Mesh::ATTRIBUTE_UV_0) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; n]);
        }
        if !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) && mesh.generate_tangents().is_err() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; n]);
        }
        if mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1) {
            mesh.remove_attribute(Mesh::ATTRIBUTE_UV_1);
        }
        if let Some(indices) = mesh.indices_mut()
            && let Indices::U16(_) = indices
        {
            *indices = Indices::U32(indices.iter().map(|i| i as u32).collect());
        }
        if !matches!(mesh.attribute(Mesh::ATTRIBUTE_UV_0), Some(VertexAttributeValues::Float32x2(_))) {
            commands.entity(e).insert(RasterOnly);
            continue;
        }
        commands.entity(e).insert(RaytracingMesh3d(mesh3d.0.clone()));
    }
}

/// F2 flips Solari; the camera gets or loses its `SolariLighting`.
fn toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut rt: ResMut<Rt>,
    cam: Query<(Entity, Has<SolariLighting>), With<MainCam>>,
    mut lights: Query<&mut PointLight>,
    mut tubes: ResMut<Tubes>,
) {
    if keys.just_pressed(KeyCode::F2) && rt.supported {
        rt.on = !rt.on;
        info!("Solari {}", if rt.on { "on" } else { "off (PBR)" });
    }
    let want = rt.on && rt.supported;
    for (e, has) in &cam {
        if want && !has {
            // TAA averages Solari's per-pixel noise over frames (no DLSS here).
            commands.entity(e).insert((SolariLighting::default(), TemporalAntiAliasing::default()));
        } else if !want && has {
            // TAA's jitter and mip bias stay behind otherwise, and shake the PBR picture.
            commands.entity(e).remove::<(SolariLighting, TemporalAntiAliasing, TemporalJitter, MipBias)>();
        }
    }
    let boost = if want { std::env::var("UPD_TUBES").ok().and_then(|s| s.parse().ok()).unwrap_or(TUBE_BOOST) } else { 1.0 };
    if tubes.boost != boost {
        tubes.boost = boost;
    }
    // Solari traces its own shadows.
    for mut l in &mut lights {
        if l.shadow_maps_enabled == want {
            l.shadow_maps_enabled = !want;
        }
    }
}

/// `UPD_BENCH=1`: for each mode (PBR, then Solari), switch, warm up (Solari's
/// temporal history fills in), time it, shoot it; then log both and exit.
#[allow(clippy::too_many_arguments)]
fn bench(
    mut commands: Commands,
    time: Res<Time>,
    diags: Res<DiagnosticsStore>,
    mut rt: ResMut<Rt>,
    mut at: Local<(usize, f32)>,
    mut samples: Local<Vec<f64>>,
    mut results: Local<Vec<(&'static str, f64)>>,
    mut exit: MessageWriter<AppExit>,
) {
    // PBR again last: toggling back must give the same picture and speed.
    const MODES: [(&str, bool); 3] = [("pbr", false), ("solari", true), ("pbr_again", false)];
    /// Seconds: warm up, measure, then let the screenshot land.
    const WARM: f32 = 4.0;
    const RUN: f32 = 5.0;
    const SHOT: f32 = 1.0;
    let now = time.elapsed_secs();
    let (k, t0) = &mut *at;
    let Some(&(name, on)) = MODES.get(*k) else {
        for (name, ms) in results.iter() {
            info!("bench {name}: {:.0} fps ({ms:.2} ms a frame), vsync off", 1000.0 / ms);
        }
        exit.write(AppExit::Success);
        return;
    };
    if *t0 == 0.0 {
        *t0 = now;
        samples.clear();
    }
    rt.on = on;
    let t = now - *t0;
    if t > WARM && t <= WARM + RUN {
        if let Some(dt) = diags.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.value()) {
            samples.push(dt);
        }
    } else if t > WARM + RUN && results.len() == *k {
        results.push((name, samples.iter().sum::<f64>() / samples.len().max(1) as f64));
        std::fs::create_dir_all("shots").ok();
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("shots/rt_{name}.png")));
    } else if t > WARM + RUN + SHOT {
        *k += 1;
        *t0 = 0.0;
    }
}
