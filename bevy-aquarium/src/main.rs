use bevy::asset::AssetPlugin;
use bevy::audio::{AudioPlayer, PlaybackSettings, SpatialListener, Volume};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::core_pipeline::{
    core_3d::graph::Node3d,
    fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin},
};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::render::{
    extract_component::ExtractComponent,
    render_graph::{InternedRenderLabel, RenderLabel},
    render_resource::ShaderType,
};
use bevy::shader::ShaderRef;
use bevy_procedural_audio::prelude::{
    AudioUnit, DspAppExt, DspGraph, DspManager, DspPlugin, DspSource, SourceType, sine_hz,
    triangle_hz,
};
use cultcache_rs::{CultCache, DatabaseEntry, SingleFileMessagePackBackingStore};
use cultnet_rs::{
    CultNetDocumentBinding, CultNetDocumentRegistry, CultNetMessage, CultNetWireContract,
    encode_cultnet_message_to_vec,
};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::path::PathBuf;
use std::time::Duration;

const GRID_BASE_HALF_EXTENT: f32 = 42.0;
const GRID_MIN_HALF_EXTENT: f32 = 12.0;
const GRID_MAX_HALF_EXTENT: f32 = 180.0;
const BODY_RADIUS: f32 = 0.9;
const SELF_RADIUS: f32 = 1.25;
const GRID_Z: f32 = 0.0;
const CURSOR_WELL_RADIUS: f32 = 4.6;
const CURSOR_WELL_MASS: f32 = 2.1;
const MAX_RAYMARCH_BODIES: usize = 8;
const RAYMARCH_FROXEL_WIDTH: usize = 16;
const RAYMARCH_FROXEL_HEIGHT: usize = 9;
const RAYMARCH_FROXEL_DEPTH: usize = 16;
const RAYMARCH_FROXEL_COUNT: usize =
    RAYMARCH_FROXEL_WIDTH * RAYMARCH_FROXEL_HEIGHT * RAYMARCH_FROXEL_DEPTH;
const RAYMARCH_FROXEL_MASK_WORDS: usize = RAYMARCH_FROXEL_COUNT / 4;
const LIGHT_FROXEL_WIDTH: usize = 8;
const LIGHT_FROXEL_HEIGHT: usize = 5;
const LIGHT_FROXEL_DEPTH: usize = 8;
const LIGHT_FROXEL_COUNT: usize = LIGHT_FROXEL_WIDTH * LIGHT_FROXEL_HEIGHT * LIGHT_FROXEL_DEPTH;

fn main() {
    let runtime_bridge = CultRuntimeBridge::load().unwrap_or_else(|err| {
        eprintln!("failed to initialize cultnet/cultcache bridge: {err}");
        CultRuntimeBridge::fallback()
    });
    let settings = runtime_bridge.settings.clone();
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.008, 0.011, 0.014)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.62, 0.72, 0.86),
            brightness: 0.18,
            affects_lightmapped_meshes: true,
        })
        .insert_resource(CameraRig::from_settings(&settings))
        .insert_resource(GridFrame::from_camera_settings(&settings))
        .insert_resource(PointerWorld::default())
        .insert_resource(GridDirty(true))
        .insert_resource(FroxelLightingState::default())
        .insert_resource(LiveStateAutosave(Timer::from_seconds(
            1.0,
            TimerMode::Repeating,
        )))
        .insert_resource(AquariumAudioState::default())
        .insert_resource(runtime_bridge)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Epiphany Aquarium - Bevy Host".to_string(),
                        resolution: (1600, 900).into(),
                        present_mode: bevy::window::PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    watch_for_changes_override: Some(true),
                    ..default()
                }),
        )
        .add_plugins(DspPlugin::default())
        .add_plugins(FullscreenMaterialPlugin::<AquariumRaymarch>::default())
        .add_dsp_source(aquarium_pluck, SourceType::Static { duration: 0.52 })
        .add_dsp_source(aquarium_heartbeat, SourceType::Static { duration: 0.28 })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                camera_input,
                sync_grid_frame,
                update_camera,
                update_raymarch_uniforms,
                project_pointer_to_grid,
                integrate_bodies,
                autosave_live_state,
                billboard_labels,
                aquarium_audio,
                reload_domain_input,
            ),
        )
        .run();
}

#[derive(Clone, Debug, PartialEq, DatabaseEntry)]
#[cultcache(type = "epiphany.aquarium.client-settings")]
struct AquariumClientSettings {
    #[cultcache(key = 0)]
    schema_version: String,
    #[cultcache(key = 1)]
    camera_target: [f32; 3],
    #[cultcache(key = 2)]
    camera_yaw: f32,
    #[cultcache(key = 3)]
    camera_pitch: f32,
    #[cultcache(key = 4)]
    camera_distance: f32,
    #[cultcache(key = 5)]
    active_member_id: String,
}

impl Default for AquariumClientSettings {
    fn default() -> Self {
        Self {
            schema_version: "epiphany.aquarium.client-settings.v0".to_string(),
            camera_target: [0.0, 0.0, GRID_Z],
            camera_yaw: -0.58,
            camera_pitch: 0.88,
            camera_distance: 34.0,
            active_member_id: "epiphany-agent".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DatabaseEntry)]
#[cultcache(type = "epiphany.aquarium.agent-presence")]
struct AquariumAgentPresence {
    #[cultcache(key = 0)]
    schema_version: String,
    #[cultcache(key = 1)]
    member_id: String,
    #[cultcache(key = 2)]
    label: String,
    #[cultcache(key = 3)]
    liveness: String,
}

#[derive(Clone, Debug, PartialEq, DatabaseEntry)]
#[cultcache(type = "epiphany.aquarium.domain-state")]
struct AquariumDomainState {
    #[cultcache(key = 0)]
    schema_version: String,
    #[cultcache(key = 1)]
    reload_generation: u64,
    #[cultcache(key = 2)]
    swarm_label: String,
}

impl Default for AquariumDomainState {
    fn default() -> Self {
        Self {
            schema_version: "epiphany.aquarium.domain-state.v0".to_string(),
            reload_generation: 0,
            swarm_label: "Epiphany".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DatabaseEntry)]
#[cultcache(type = "epiphany.aquarium.body-state")]
struct AquariumBodyState {
    #[cultcache(key = 0)]
    schema_version: String,
    #[cultcache(key = 1)]
    body_id: String,
    #[cultcache(key = 2)]
    label: String,
    #[cultcache(key = 3)]
    class: String,
    #[cultcache(key = 4)]
    anchor: [f32; 3],
    #[cultcache(key = 5)]
    position: [f32; 3],
    #[cultcache(key = 6)]
    velocity: [f32; 3],
    #[cultcache(key = 7)]
    mass: f32,
    #[cultcache(key = 8)]
    phase: f32,
}

impl AquariumBodyState {
    fn new(
        body_id: impl Into<String>,
        label: impl Into<String>,
        class: BodyClass,
        position: Vec3,
        velocity: Vec3,
        mass: f32,
        phase: f32,
        anchor: Vec3,
    ) -> Self {
        Self {
            schema_version: "epiphany.aquarium.body-state.v0".to_string(),
            body_id: body_id.into(),
            label: label.into(),
            class: class.cache_key().to_string(),
            anchor: anchor.to_array(),
            position: position.to_array(),
            velocity: velocity.to_array(),
            mass,
            phase,
        }
    }

    fn class(&self) -> BodyClass {
        BodyClass::from_cache_key(&self.class)
    }
}

#[derive(Resource, Clone, Debug)]
struct CultRuntimeBridge {
    runtime_id: String,
    settings_path: PathBuf,
    settings: AquariumClientSettings,
    domain_state: AquariumDomainState,
    supported_document_types: Vec<String>,
    hello_payload_bytes: usize,
}

impl CultRuntimeBridge {
    fn load() -> anyhow::Result<Self> {
        let settings_path =
            PathBuf::from(".epiphany-aquarium").join("bevy-client-settings.msgpack");
        let mut cache = Self::open_cache(&settings_path)?;

        let settings = match cache.get::<AquariumClientSettings>("client")? {
            Some(settings) => settings,
            None => cache.put("client", &AquariumClientSettings::default())?,
        };
        let domain_state = match cache.get::<AquariumDomainState>("domain")? {
            Some(domain_state) => domain_state,
            None => cache.put("domain", &AquariumDomainState::default())?,
        };

        cache.put(
            "epiphany-agent",
            &AquariumAgentPresence {
                schema_version: "epiphany.aquarium.agent-presence.v0".to_string(),
                member_id: "epiphany-agent".to_string(),
                label: "Epiphany".to_string(),
                liveness: "sleeping".to_string(),
            },
        )?;

        let mut registry = CultNetDocumentRegistry::new();
        registry
            .register(CultNetDocumentBinding::for_entry::<AquariumClientSettings>(
                Some("epiphany.aquarium.client-settings.v0".to_string()),
            ))
            .register(CultNetDocumentBinding::for_entry::<AquariumAgentPresence>(
                Some("epiphany.aquarium.agent-presence.v0".to_string()),
            ));

        let supported_document_types = vec![
            AquariumClientSettings::TYPE.to_string(),
            AquariumAgentPresence::TYPE.to_string(),
            AquariumDomainState::TYPE.to_string(),
            AquariumBodyState::TYPE.to_string(),
        ];
        let hello = CultNetMessage::Hello {
            runtime_id: "epiphany-aquarium-bevy".to_string(),
            runtime_kind: "bevy-client".to_string(),
            agent_id: Some("epiphany-aquarium".to_string()),
            role: Some("aquarium-client".to_string()),
            display_name: Some("Epiphany Aquarium Bevy".to_string()),
            supported_document_types: Some(supported_document_types.clone()),
            supported_message_versions: Some(vec!["cultnet.hello.v0".to_string()]),
            supports_schema_catalog: Some(true),
        };
        let hello_payload_bytes =
            encode_cultnet_message_to_vec(&hello, CultNetWireContract::CultNetSchemaV0)?.len();

        Ok(Self {
            runtime_id: "epiphany-aquarium-bevy".to_string(),
            settings_path,
            settings,
            domain_state,
            supported_document_types,
            hello_payload_bytes,
        })
    }

    fn fallback() -> Self {
        Self {
            runtime_id: "epiphany-aquarium-bevy".to_string(),
            settings_path: PathBuf::from(".epiphany-aquarium/bevy-client-settings.msgpack"),
            settings: AquariumClientSettings::default(),
            domain_state: AquariumDomainState::default(),
            supported_document_types: Vec::new(),
            hello_payload_bytes: 0,
        }
    }

    fn open_cache(settings_path: &PathBuf) -> anyhow::Result<CultCache> {
        let mut cache = CultCache::new();
        cache.register_entry_type::<AquariumClientSettings>()?;
        cache.register_entry_type::<AquariumAgentPresence>()?;
        cache.register_entry_type::<AquariumDomainState>()?;
        cache.register_entry_type::<AquariumBodyState>()?;
        cache.add_generic_backing_store(SingleFileMessagePackBackingStore::new(settings_path));
        cache.pull_all_backing_stores()?;
        Ok(cache)
    }

    fn reload_domain(
        &mut self,
        settings: AquariumClientSettings,
        body_states: &[AquariumBodyState],
    ) -> anyhow::Result<Vec<AquariumBodyState>> {
        let mut cache = Self::open_cache(&self.settings_path)?;
        self.settings = cache.put("client", &settings)?;
        for body_state in body_states {
            cache.put(body_state.body_id.clone(), body_state)?;
        }
        let mut domain_state = cache
            .get::<AquariumDomainState>("domain")?
            .unwrap_or_default();
        domain_state.reload_generation = domain_state.reload_generation.saturating_add(1);
        self.domain_state = cache.put("domain", &domain_state)?;
        cache.put(
            "epiphany-agent",
            &AquariumAgentPresence {
                schema_version: "epiphany.aquarium.agent-presence.v0".to_string(),
                member_id: "epiphany-agent".to_string(),
                label: self.domain_state.swarm_label.clone(),
                liveness: "sleeping".to_string(),
            },
        )?;
        let mut cached_bodies = cache.get_all::<AquariumBodyState>()?;
        cached_bodies.sort_by(|a, b| a.body_id.cmp(&b.body_id));
        Ok(cached_bodies)
    }

    fn load_body_states(&self) -> anyhow::Result<Vec<AquariumBodyState>> {
        let cache = Self::open_cache(&self.settings_path)?;
        let mut body_states = cache.get_all::<AquariumBodyState>()?;
        body_states.sort_by(|a, b| a.body_id.cmp(&b.body_id));
        Ok(body_states)
    }

    fn save_live_state(
        &mut self,
        settings: AquariumClientSettings,
        body_states: &[AquariumBodyState],
    ) -> anyhow::Result<()> {
        let mut cache = Self::open_cache(&self.settings_path)?;
        self.settings = cache.put("client", &settings)?;
        for body_state in body_states {
            cache.put(body_state.body_id.clone(), body_state)?;
        }
        Ok(())
    }
}

#[derive(Resource)]
struct CameraRig {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 0.0, GRID_Z),
            yaw: -0.58,
            pitch: 0.88,
            distance: 34.0,
        }
    }
}

impl CameraRig {
    fn from_settings(settings: &AquariumClientSettings) -> Self {
        Self {
            target: grid_center_from_array(settings.camera_target),
            yaw: settings.camera_yaw,
            pitch: settings.camera_pitch,
            distance: settings.camera_distance,
        }
    }

    fn constrain_to_grid_plane(&mut self) {
        self.target.z = GRID_Z;
        self.distance = self.distance.clamp(8.0, 120.0);
        self.pitch = self.pitch.clamp(0.18, FRAC_PI_2 - 0.04);
    }
}

#[derive(Resource, Clone, Copy, Debug)]
struct GridFrame {
    center: Vec2,
    half_extent: f32,
}

impl GridFrame {
    fn from_camera_settings(settings: &AquariumClientSettings) -> Self {
        Self::from_camera(
            grid_center_from_array(settings.camera_target),
            settings.camera_distance,
        )
    }

    fn from_camera(target: Vec3, distance: f32) -> Self {
        Self {
            center: target.truncate(),
            half_extent: grid_half_extent_for_distance(distance),
        }
    }

    fn contains(self, point: Vec2) -> bool {
        let delta = point - self.center;
        delta.x.abs() <= self.half_extent && delta.y.abs() <= self.half_extent
    }
}

fn grid_center_from_array(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], GRID_Z)
}

fn grid_half_extent_for_distance(distance: f32) -> f32 {
    (GRID_BASE_HALF_EXTENT * (distance / 34.0)).clamp(GRID_MIN_HALF_EXTENT, GRID_MAX_HALF_EXTENT)
}

#[derive(Resource, Default)]
struct PointerWorld {
    plane_position: Option<Vec3>,
    grid_position: Option<Vec3>,
}

#[derive(Resource)]
struct GridDirty(bool);

#[derive(Resource)]
struct LiveStateAutosave(Timer);

#[derive(Resource)]
struct AquariumAudioState {
    next_heartbeat: f32,
    next_chirp: f32,
    next_touch: f32,
    touched_body_id: Option<String>,
}

#[derive(Resource, Clone, Copy)]
struct FroxelLightingState {
    sh_l0: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1x: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1y: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1z: [Vec4; LIGHT_FROXEL_COUNT],
}

impl Default for FroxelLightingState {
    fn default() -> Self {
        Self {
            sh_l0: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1x: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1y: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1z: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
        }
    }
}

impl Default for AquariumAudioState {
    fn default() -> Self {
        Self {
            next_heartbeat: 0.45,
            next_chirp: 1.2,
            next_touch: 0.0,
            touched_body_id: None,
        }
    }
}

#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
struct AquariumRaymarch {
    time: f32,
    body_count: f32,
    grid_center: Vec2,
    grid_half_extent: f32,
    depth_near: f32,
    depth_far: f32,
    depth_span: f32,
    camera_position: Vec4,
    ray00: Vec4,
    ray10: Vec4,
    ray01: Vec4,
    ray11: Vec4,
    bodies: [Vec4; MAX_RAYMARCH_BODIES],
    colors: [Vec4; MAX_RAYMARCH_BODIES],
    froxel_masks: [UVec4; RAYMARCH_FROXEL_MASK_WORDS],
    sh_l0: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1x: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1y: [Vec4; LIGHT_FROXEL_COUNT],
    sh_l1z: [Vec4; LIGHT_FROXEL_COUNT],
}

impl Default for AquariumRaymarch {
    fn default() -> Self {
        Self {
            time: 0.0,
            body_count: 0.0,
            grid_center: Vec2::ZERO,
            grid_half_extent: GRID_BASE_HALF_EXTENT,
            depth_near: 1.0,
            depth_far: 80.0,
            depth_span: 79.0,
            camera_position: Vec4::ZERO,
            ray00: Vec4::new(-0.5, 0.5, -0.7, 0.0),
            ray10: Vec4::new(0.5, 0.5, -0.7, 0.0),
            ray01: Vec4::new(-0.5, 0.8, -0.28, 0.0),
            ray11: Vec4::new(0.5, 0.8, -0.28, 0.0),
            bodies: [Vec4::ZERO; MAX_RAYMARCH_BODIES],
            colors: [Vec4::ZERO; MAX_RAYMARCH_BODIES],
            froxel_masks: [UVec4::ZERO; RAYMARCH_FROXEL_MASK_WORDS],
            sh_l0: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1x: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1y: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
            sh_l1z: [Vec4::ZERO; LIGHT_FROXEL_COUNT],
        }
    }
}

impl FullscreenMaterial for AquariumRaymarch {
    fn fragment_shader() -> ShaderRef {
        "shaders/aquarium_raymarch.wgsl".into()
    }

    fn node_edges() -> Vec<InternedRenderLabel> {
        vec![
            Node3d::Tonemapping.intern(),
            Self::node_label().intern(),
            Node3d::EndMainPassPostProcessing.intern(),
        ]
    }
}

#[derive(Component)]
struct AquariumCamera;

#[derive(Component)]
struct AquariumDomainRoot;

#[derive(Component)]
struct BodyLabel;

#[derive(Component)]
#[allow(dead_code)]
struct OrbitGuide {
    center: Vec3,
    radius: f32,
}

#[derive(Component)]
#[allow(dead_code)]
struct CelestialBody {
    body_id: String,
    label: String,
    class: BodyClass,
    anchor: Vec3,
    velocity: Vec3,
    mass: f32,
    phase: f32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BodyClass {
    SleepingEpiphany,
    LivingSelf,
    Agent,
}

impl BodyClass {
    fn cache_key(self) -> &'static str {
        match self {
            BodyClass::SleepingEpiphany => "sleeping-epiphany",
            BodyClass::LivingSelf => "living-self",
            BodyClass::Agent => "agent",
        }
    }

    fn from_cache_key(value: &str) -> Self {
        match value {
            "living-self" => BodyClass::LivingSelf,
            "agent" => BodyClass::Agent,
            _ => BodyClass::SleepingEpiphany,
        }
    }
}

fn body_color(class: BodyClass) -> Vec3 {
    match class {
        BodyClass::LivingSelf => Vec3::new(4.2, 2.1, 0.55),
        BodyClass::SleepingEpiphany => Vec3::new(0.68, 0.82, 1.0),
        BodyClass::Agent => Vec3::new(0.48, 0.86, 0.78),
    }
}

fn setup(mut commands: Commands, bridge: Res<CultRuntimeBridge>, grid_frame: Res<GridFrame>) {
    info!(
        "{} bridge: {} docs, hello {} bytes, settings {}",
        bridge.runtime_id,
        bridge.supported_document_types.len(),
        bridge.hello_payload_bytes,
        bridge.settings_path.display()
    );
    info!(
        "aquarium domain generation {}",
        bridge.domain_state.reload_generation
    );
    commands.spawn((
        DirectionalLight {
            illuminance: 4800.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(-18.0, -12.0, 32.0).looking_at(Vec3::ZERO, Vec3::Z),
    ));

    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 46.0_f32.to_radians(),
            near: 0.5,
            far: 240.0,
            ..default()
        }),
        Transform::default(),
        Msaa::Off,
        Tonemapping::AcesFitted,
        SpatialListener::default(),
        AquariumRaymarch::default(),
        AquariumCamera,
    ));

    spawn_domain(
        &mut commands,
        &bridge,
        *grid_frame,
        bridge.load_body_states().ok(),
    );
}

fn spawn_domain(
    commands: &mut Commands,
    bridge: &CultRuntimeBridge,
    _grid_frame: GridFrame,
    cached_bodies: Option<Vec<AquariumBodyState>>,
) {
    let domain_root = commands
        .spawn((
            Transform::default(),
            Visibility::default(),
            AquariumDomainRoot,
            Name::new("Aquarium Domain"),
        ))
        .id();

    let body_states = cached_bodies.unwrap_or_else(default_body_states);
    let body_states = if body_states.is_empty() {
        default_body_states()
    } else {
        body_states
    };
    for state in body_states {
        spawn_body_from_state(commands, domain_root, state);
    }

    info!(
        "rehydrated aquarium domain '{}' generation {}",
        bridge.domain_state.swarm_label, bridge.domain_state.reload_generation
    );
}

fn default_body_states() -> Vec<AquariumBodyState> {
    let mut states = Vec::new();
    states.push(AquariumBodyState::new(
        "epiphany",
        "Epiphany",
        BodyClass::LivingSelf,
        Vec3::new(0.0, 0.0, 4.2),
        Vec3::ZERO,
        5.6,
        0.0,
        Vec3::new(0.0, 0.0, 4.2),
    ));

    let agents = [
        ("face", "Face", 0.0),
        ("eyes", "Eyes", 0.8),
        ("hands", "Hands", 1.6),
        ("soul", "Soul", 2.4),
        ("life", "Life", 3.2),
        ("body", "Body", 4.0),
        ("imagination", "Imagination", 4.8),
    ];
    for (body_id, label, phase) in agents {
        let anchor = orbit_anchor(Vec3::ZERO, 7.0, phase) + Vec3::Z * 2.6;
        states.push(AquariumBodyState::new(
            body_id,
            label,
            BodyClass::Agent,
            anchor,
            Vec3::ZERO,
            1.0,
            phase,
            anchor,
        ));
    }

    states.push(AquariumBodyState::new(
        "aetheria-lore",
        "Aetheria Lore",
        BodyClass::SleepingEpiphany,
        Vec3::new(-13.0, -9.0, 2.6),
        Vec3::ZERO,
        2.4,
        0.33,
        Vec3::new(-13.0, -9.0, 2.6),
    ));
    states
}

fn spawn_body_from_state(commands: &mut Commands, domain_root: Entity, state: AquariumBodyState) {
    let label = state.label.clone();
    let label_for_body = label.clone();
    let class = state.class();
    let position = Vec3::from_array(state.position);
    let anchor = Vec3::from_array(state.anchor);
    let velocity = Vec3::from_array(state.velocity);
    let mass = state.mass;
    let phase = state.phase;
    let radius = if class == BodyClass::LivingSelf {
        SELF_RADIUS
    } else {
        BODY_RADIUS
    };

    let body = commands
        .spawn((
            Transform::from_translation(position),
            Visibility::default(),
            CelestialBody {
                body_id: state.body_id,
                label: label_for_body,
                class,
                anchor,
                velocity,
                mass,
                phase,
            },
            ChildOf(domain_root),
        ))
        .id();

    commands.entity(body).with_children(|parent| {
        parent.spawn((
            Text2d::new(label),
            TextFont {
                font_size: 22.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.96, 0.98)),
            Transform::from_xyz(0.0, 0.0, radius + 0.62),
            BodyLabel,
        ));
    });

    if class == BodyClass::Agent {
        commands.spawn((
            Transform::default(),
            OrbitGuide {
                center: Vec3::ZERO,
                radius: (position.truncate()).length(),
            },
            ChildOf(domain_root),
        ));
    }
}

fn live_settings_from_rig(rig: &CameraRig, active_member_id: &str) -> AquariumClientSettings {
    AquariumClientSettings {
        schema_version: "epiphany.aquarium.client-settings.v0".to_string(),
        camera_target: Vec3::new(rig.target.x, rig.target.y, GRID_Z).to_array(),
        camera_yaw: rig.yaw,
        camera_pitch: rig.pitch,
        camera_distance: rig.distance,
        active_member_id: active_member_id.to_string(),
    }
}

fn snapshot_body_states(bodies: &Query<(&Transform, &CelestialBody)>) -> Vec<AquariumBodyState> {
    let mut body_states: Vec<_> = bodies
        .iter()
        .map(|(transform, body)| {
            AquariumBodyState::new(
                body.body_id.clone(),
                body.label.clone(),
                body.class,
                transform.translation,
                body.velocity,
                body.mass,
                body.phase,
                body.anchor,
            )
        })
        .collect();
    body_states.sort_by(|a, b| a.body_id.cmp(&b.body_id));
    body_states
}

fn autosave_live_state(
    time: Res<Time>,
    mut timer: ResMut<LiveStateAutosave>,
    mut bridge: ResMut<CultRuntimeBridge>,
    rig: Res<CameraRig>,
    bodies: Query<(&Transform, &CelestialBody)>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }
    let settings = live_settings_from_rig(&rig, &bridge.settings.active_member_id);
    let body_states = snapshot_body_states(&bodies);
    if let Err(err) = bridge.save_live_state(settings, &body_states) {
        warn!("failed to autosave aquarium live state: {err}");
    }
}

fn reload_domain_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut bridge: ResMut<CultRuntimeBridge>,
    rig: Res<CameraRig>,
    mut pointer: ResMut<PointerWorld>,
    mut dirty: ResMut<GridDirty>,
    roots: Query<Entity, With<AquariumDomainRoot>>,
    bodies: Query<(&Transform, &CelestialBody)>,
) {
    if !keys.just_pressed(KeyCode::F5) {
        return;
    }

    let settings = live_settings_from_rig(&rig, &bridge.settings.active_member_id);
    let body_states = snapshot_body_states(&bodies);
    let cached_bodies = match bridge.reload_domain(settings, &body_states) {
        Ok(cached_bodies) => cached_bodies,
        Err(err) => {
            warn!("failed to reload aquarium domain from CultCache: {err}");
            return;
        }
    };

    for root in &roots {
        commands.entity(root).despawn();
    }
    pointer.plane_position = None;
    pointer.grid_position = None;
    dirty.0 = true;
    spawn_domain(
        &mut commands,
        &bridge,
        GridFrame::from_camera(rig.target, rig.distance),
        Some(cached_bodies),
    );
    info!(
        "domain reload complete; generation {}",
        bridge.domain_state.reload_generation
    );
}

fn camera_input(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut rig: ResMut<CameraRig>,
    mut dirty: ResMut<GridDirty>,
    camera: Query<&Transform, With<AquariumCamera>>,
) {
    let previous_target = rig.target;
    let previous_distance = rig.distance;
    let drag = motion.delta;

    if buttons.pressed(MouseButton::Middle) {
        rig.yaw -= drag.x * 0.006;
        rig.pitch = (rig.pitch + drag.y * 0.004).clamp(0.18, FRAC_PI_2 - 0.04);
    }

    if buttons.pressed(MouseButton::Right) {
        if let Ok(camera_transform) = camera.single() {
            let right = camera_transform.right().truncate().normalize_or_zero();
            let forward = camera_transform.forward().truncate().normalize_or_zero();
            let scale = rig.distance * 0.0018;
            rig.target -= Vec3::new(right.x, right.y, 0.0) * drag.x * scale;
            rig.target += Vec3::new(forward.x, forward.y, 0.0) * drag.y * scale;
        }
    }

    if scroll.delta.y.abs() > f32::EPSILON {
        rig.distance *= (-scroll.delta.y * 0.085).exp();
        rig.distance = rig.distance.clamp(8.0, 120.0);
    }

    if let Ok(camera_transform) = camera.single() {
        let mut pan = Vec3::ZERO;
        let right = camera_transform.right().truncate().normalize_or_zero();
        let forward = camera_transform.forward().truncate().normalize_or_zero();
        if keys.pressed(KeyCode::KeyA) {
            pan -= Vec3::new(right.x, right.y, 0.0);
        }
        if keys.pressed(KeyCode::KeyD) {
            pan += Vec3::new(right.x, right.y, 0.0);
        }
        if keys.pressed(KeyCode::KeyW) {
            pan += Vec3::new(forward.x, forward.y, 0.0);
        }
        if keys.pressed(KeyCode::KeyS) {
            pan -= Vec3::new(forward.x, forward.y, 0.0);
        }
        let speed = 11.0 * (rig.distance / 34.0).sqrt();
        rig.target += pan.normalize_or_zero() * speed * time.delta_secs();
    }

    rig.constrain_to_grid_plane();
    dirty.0 |= rig.target.distance_squared(previous_target) > 0.0001
        || (rig.distance - previous_distance).abs() > 0.001;
}

fn update_camera(rig: Res<CameraRig>, mut camera: Query<&mut Transform, With<AquariumCamera>>) {
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    let horizontal = rig.distance * rig.pitch.cos();
    let offset = Vec3::new(
        horizontal * rig.yaw.cos(),
        horizontal * rig.yaw.sin(),
        rig.distance * rig.pitch.sin(),
    );
    transform.translation = rig.target + offset;
    transform.look_at(rig.target, Vec3::Z);
}

fn sync_grid_frame(
    rig: Res<CameraRig>,
    mut frame: ResMut<GridFrame>,
    mut dirty: ResMut<GridDirty>,
) {
    let next = GridFrame::from_camera(rig.target, rig.distance);
    let changed = frame.center.distance_squared(next.center) > 0.0001
        || (frame.half_extent - next.half_extent).abs() > 0.001;
    if changed {
        *frame = next;
        dirty.0 = true;
    }
}

fn update_raymarch_uniforms(
    time: Res<Time>,
    grid_frame: Res<GridFrame>,
    mut lighting: ResMut<FroxelLightingState>,
    bodies: Query<(&Transform, &CelestialBody)>,
    mut camera: Query<(&GlobalTransform, &Projection, &mut AquariumRaymarch), With<AquariumCamera>>,
) {
    let Ok((camera_transform, projection, mut raymarch)) = camera.single_mut() else {
        return;
    };
    let transform = camera_transform.compute_transform();
    let aspect = match projection {
        Projection::Perspective(perspective) => perspective.aspect_ratio,
        _ => 16.0 / 9.0,
    };
    let fov = match projection {
        Projection::Perspective(perspective) => perspective.fov,
        _ => 46.0_f32.to_radians(),
    };
    let half_y = (fov * 0.5).tan();
    let half_x = half_y * aspect;
    let forward = *transform.forward();
    let right = *transform.right();
    let up = *transform.up();
    let ray = |x: f32, y: f32| -> Vec4 {
        (forward + right * (x * half_x) + up * (y * half_y))
            .normalize()
            .extend(0.0)
    };

    raymarch.time = time.elapsed_secs();
    raymarch.grid_center = grid_frame.center;
    raymarch.grid_half_extent = grid_frame.half_extent;
    raymarch.depth_near = 1.0;
    raymarch.depth_far = (grid_frame.half_extent * 3.0).clamp(32.0, 260.0);
    raymarch.depth_span = raymarch.depth_far - raymarch.depth_near;
    raymarch.camera_position = transform.translation.extend(1.0);
    raymarch.ray00 = ray(-1.0, 1.0);
    raymarch.ray10 = ray(1.0, 1.0);
    raymarch.ray01 = ray(-1.0, -1.0);
    raymarch.ray11 = ray(1.0, -1.0);

    raymarch.bodies = [Vec4::ZERO; MAX_RAYMARCH_BODIES];
    raymarch.colors = [Vec4::ZERO; MAX_RAYMARCH_BODIES];
    raymarch.froxel_masks = [UVec4::ZERO; RAYMARCH_FROXEL_MASK_WORDS];
    let camera_position = raymarch.camera_position.truncate();
    let depth_near = raymarch.depth_near;
    let depth_span = raymarch.depth_span;
    let mut count = 0usize;
    for (transform, body) in bodies.iter().take(MAX_RAYMARCH_BODIES) {
        let self_flag = if body.class == BodyClass::LivingSelf {
            1.0
        } else {
            0.0
        };
        let radius = if body.class == BodyClass::LivingSelf {
            SELF_RADIUS
        } else {
            BODY_RADIUS
        };
        raymarch.bodies[count] = Vec4::new(
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            radius,
        );
        raymarch.colors[count] = body_color(body.class).extend(self_flag);
        bin_body_into_froxels(
            &mut raymarch.froxel_masks,
            count,
            radius * (1.18 + self_flag * 0.18),
            depth_near,
            depth_span,
            transform.translation - camera_position,
            right,
            up,
            forward,
            half_x,
            half_y,
        );
        count += 1;
    }
    raymarch.body_count = count as f32;

    let sun = bodies
        .iter()
        .filter(|(_, body)| body.class == BodyClass::LivingSelf)
        .max_by(|(_, a), (_, b)| a.mass.total_cmp(&b.mass))
        .map(|(transform, body)| (transform.translation, body.mass));
    update_froxel_lighting(
        &mut lighting,
        sun,
        camera_position,
        [
            raymarch.ray00,
            raymarch.ray10,
            raymarch.ray01,
            raymarch.ray11,
        ],
        depth_near,
        depth_span,
    );
    raymarch.sh_l0 = lighting.sh_l0;
    raymarch.sh_l1x = lighting.sh_l1x;
    raymarch.sh_l1y = lighting.sh_l1y;
    raymarch.sh_l1z = lighting.sh_l1z;
}

fn bin_body_into_froxels(
    masks: &mut [UVec4; RAYMARCH_FROXEL_MASK_WORDS],
    body_index: usize,
    radius: f32,
    depth_near: f32,
    depth_span: f32,
    camera_to_body: Vec3,
    camera_right: Vec3,
    camera_up: Vec3,
    camera_forward: Vec3,
    half_x: f32,
    half_y: f32,
) {
    let depth = camera_to_body.dot(camera_forward);
    if depth <= depth_near || depth > depth_near + depth_span {
        return;
    }
    let ndc_x = camera_to_body.dot(camera_right) / (depth * half_x).max(0.001);
    let ndc_y = camera_to_body.dot(camera_up) / (depth * half_y).max(0.001);
    let radius_x = radius / (depth * half_x).max(0.001);
    let radius_y = radius / (depth * half_y).max(0.001);
    if ndc_x + radius_x < -1.0
        || ndc_x - radius_x > 1.0
        || ndc_y + radius_y < -1.0
        || ndc_y - radius_y > 1.0
    {
        return;
    }

    let u = (ndc_x * 0.5 + 0.5) * RAYMARCH_FROXEL_WIDTH as f32;
    let v = (0.5 - ndc_y * 0.5) * RAYMARCH_FROXEL_HEIGHT as f32;
    let z = ((depth - depth_near) / depth_span).clamp(0.0, 1.0) * RAYMARCH_FROXEL_DEPTH as f32;
    let rx = (radius_x * 0.5 * RAYMARCH_FROXEL_WIDTH as f32).ceil() as isize + 1;
    let ry = (radius_y * 0.5 * RAYMARCH_FROXEL_HEIGHT as f32).ceil() as isize + 1;
    let rz = ((radius / depth_span) * RAYMARCH_FROXEL_DEPTH as f32).ceil() as isize + 1;
    let bit = 1u32 << body_index;

    for zz in (z.floor() as isize - rz)..=(z.ceil() as isize + rz) {
        if !(0..RAYMARCH_FROXEL_DEPTH as isize).contains(&zz) {
            continue;
        }
        for yy in (v.floor() as isize - ry)..=(v.ceil() as isize + ry) {
            if !(0..RAYMARCH_FROXEL_HEIGHT as isize).contains(&yy) {
                continue;
            }
            for xx in (u.floor() as isize - rx)..=(u.ceil() as isize + rx) {
                if !(0..RAYMARCH_FROXEL_WIDTH as isize).contains(&xx) {
                    continue;
                }
                let index = zz as usize * RAYMARCH_FROXEL_WIDTH * RAYMARCH_FROXEL_HEIGHT
                    + yy as usize * RAYMARCH_FROXEL_WIDTH
                    + xx as usize;
                let word = index / 4;
                match index % 4 {
                    0 => masks[word].x |= bit,
                    1 => masks[word].y |= bit,
                    2 => masks[word].z |= bit,
                    _ => masks[word].w |= bit,
                }
            }
        }
    }
}

fn update_froxel_lighting(
    lighting: &mut FroxelLightingState,
    sun: Option<(Vec3, f32)>,
    camera_position: Vec3,
    rays: [Vec4; 4],
    depth_near: f32,
    depth_span: f32,
) {
    let previous = *lighting;
    let mut next = FroxelLightingState::default();
    for z in 0..LIGHT_FROXEL_DEPTH {
        for y in 0..LIGHT_FROXEL_HEIGHT {
            for x in 0..LIGHT_FROXEL_WIDTH {
                let index = light_froxel_index(x, y, z);
                let mut l0 = previous.sh_l0[index] * 0.58;
                let mut l1x = previous.sh_l1x[index] * 0.58;
                let mut l1y = previous.sh_l1y[index] * 0.58;
                let mut l1z = previous.sh_l1z[index] * 0.58;

                let neighbors = [
                    (x as isize - 1, y as isize, z as isize),
                    (x as isize + 1, y as isize, z as isize),
                    (x as isize, y as isize - 1, z as isize),
                    (x as isize, y as isize + 1, z as isize),
                    (x as isize, y as isize, z as isize - 1),
                    (x as isize, y as isize, z as isize + 1),
                ];
                for (nx, ny, nz) in neighbors {
                    let Some(neighbor) = light_froxel_index_checked(nx, ny, nz) else {
                        continue;
                    };
                    l0 += previous.sh_l0[neighbor] * 0.045;
                    l1x += previous.sh_l1x[neighbor] * 0.045;
                    l1y += previous.sh_l1y[neighbor] * 0.045;
                    l1z += previous.sh_l1z[neighbor] * 0.045;
                }

                if let Some((sun_position, sun_mass)) = sun {
                    let uv = Vec2::new(
                        (x as f32 + 0.5) / LIGHT_FROXEL_WIDTH as f32,
                        (y as f32 + 0.5) / LIGHT_FROXEL_HEIGHT as f32,
                    );
                    let progress = (z as f32 + 0.5) / LIGHT_FROXEL_DEPTH as f32;
                    let ray = camera_ray_from_corners(rays, uv);
                    let point = camera_position + ray * (depth_near + progress * depth_span);
                    let to_sun = sun_position - point;
                    let distance = to_sun.length().max(0.001);
                    let direction = to_sun / distance;
                    let strength = (sun_mass * 1.55 + 5.0) * (-distance * 0.055).exp();
                    let radiance = Vec3::new(4.4, 2.35, 0.72) * strength.min(9.0);
                    inject_sh(&mut l0, &mut l1x, &mut l1y, &mut l1z, radiance, direction);
                }

                next.sh_l0[index] = clamp_vec4(l0, 18.0);
                next.sh_l1x[index] = clamp_vec4(l1x, 18.0);
                next.sh_l1y[index] = clamp_vec4(l1y, 18.0);
                next.sh_l1z[index] = clamp_vec4(l1z, 18.0);
            }
        }
    }
    *lighting = next;
}

fn light_froxel_index(x: usize, y: usize, z: usize) -> usize {
    z * LIGHT_FROXEL_WIDTH * LIGHT_FROXEL_HEIGHT + y * LIGHT_FROXEL_WIDTH + x
}

fn light_froxel_index_checked(x: isize, y: isize, z: isize) -> Option<usize> {
    if x < 0
        || y < 0
        || z < 0
        || x >= LIGHT_FROXEL_WIDTH as isize
        || y >= LIGHT_FROXEL_HEIGHT as isize
        || z >= LIGHT_FROXEL_DEPTH as isize
    {
        return None;
    }
    Some(light_froxel_index(x as usize, y as usize, z as usize))
}

fn camera_ray_from_corners(rays: [Vec4; 4], uv: Vec2) -> Vec3 {
    let top = rays[0].truncate().lerp(rays[1].truncate(), uv.x);
    let bottom = rays[2].truncate().lerp(rays[3].truncate(), uv.x);
    top.lerp(bottom, uv.y).normalize()
}

fn inject_sh(
    l0: &mut Vec4,
    l1x: &mut Vec4,
    l1y: &mut Vec4,
    l1z: &mut Vec4,
    radiance: Vec3,
    dir: Vec3,
) {
    let rgb = radiance.extend(0.0);
    *l0 += rgb * 0.282095;
    *l1x += rgb * (0.488603 * dir.x);
    *l1y += rgb * (0.488603 * dir.y);
    *l1z += rgb * (0.488603 * dir.z);
}

fn clamp_vec4(value: Vec4, max_component: f32) -> Vec4 {
    Vec4::new(
        value.x.clamp(0.0, max_component),
        value.y.clamp(0.0, max_component),
        value.z.clamp(0.0, max_component),
        0.0,
    )
}

fn project_pointer_to_grid(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<AquariumCamera>>,
    mut pointer: ResMut<PointerWorld>,
    mut dirty: ResMut<GridDirty>,
    grid_frame: Res<GridFrame>,
    bodies: Query<(&Transform, &CelestialBody)>,
) {
    let Ok(window) = windows.single() else {
        if pointer.plane_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = None;
        pointer.grid_position = None;
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        if pointer.plane_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = None;
        pointer.grid_position = None;
        return;
    };
    let Ok((camera, camera_transform)) = camera.single() else {
        if pointer.plane_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = None;
        pointer.grid_position = None;
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        if pointer.plane_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = None;
        pointer.grid_position = None;
        return;
    };
    let Some(distance) =
        ray.intersect_plane(Vec3::new(0.0, 0.0, GRID_Z), InfinitePlane3d::new(Vec3::Z))
    else {
        if pointer.plane_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = None;
        pointer.grid_position = None;
        return;
    };
    let plane_position = ray.get_point(distance);
    if !grid_frame.contains(plane_position.truncate()) {
        if pointer.grid_position.is_some() {
            dirty.0 = true;
        }
        pointer.plane_position = Some(plane_position);
        pointer.grid_position = None;
        return;
    }
    let mut wells = body_wells(&bodies);
    wells.push(cursor_well(plane_position.truncate()));
    let grid_z = gravity_height(plane_position.truncate(), &wells);
    let grid_position = Vec3::new(plane_position.x, plane_position.y, grid_z);
    let moved = pointer
        .plane_position
        .map(|previous| previous.distance_squared(plane_position) > 0.0004)
        .unwrap_or(true);
    pointer.plane_position = Some(plane_position);
    pointer.grid_position = Some(grid_position);
    dirty.0 |= moved;
}

fn integrate_bodies(
    time: Res<Time>,
    pointer: Res<PointerWorld>,
    mut dirty: ResMut<GridDirty>,
    mut bodies: Query<(&mut Transform, &mut CelestialBody)>,
) {
    let dt = time.delta_secs().min(0.033);
    let mut moved = false;
    for (mut transform, mut body) in &mut bodies {
        let anchor = if body.class == BodyClass::Agent {
            let t = time.elapsed_secs() * 0.07 + body.phase;
            orbit_anchor(Vec3::ZERO, 7.0 + body.phase.sin() * 0.35, t) + Vec3::Z * 2.6
        } else {
            body.anchor
        };

        let mut acceleration = (anchor - transform.translation) * 3.4;
        acceleration -= body.velocity * 1.45;

        if let Some(pointer_position) = pointer.plane_position {
            let to_pointer =
                pointer_position + Vec3::Z * transform.translation.z - transform.translation;
            let distance = to_pointer.length().max(0.001);
            let near_attraction = 7.5 * smooth_well(distance, 0.0, 5.0);
            let far_pull = match body.class {
                BodyClass::LivingSelf => -0.18 * smooth_well(distance, 8.0, 28.0),
                _ => 0.34 * smooth_well(distance, 8.0, 34.0),
            };
            acceleration += to_pointer.normalize_or_zero() * (near_attraction + far_pull);
        }

        body.velocity += acceleration * dt;
        transform.translation += body.velocity * dt;
        let min_height = GRID_Z + 1.4 + body.mass.sqrt() * 0.35;
        transform.translation.z = transform.translation.z.max(min_height);
        transform.rotate_z(dt * 0.18 * (1.0 + body.phase));
        moved = true;
    }
    dirty.0 |= moved;
}

fn billboard_labels(
    camera: Query<&GlobalTransform, With<AquariumCamera>>,
    mut labels: Query<&mut Transform, (With<BodyLabel>, Without<AquariumCamera>)>,
) {
    let Ok(camera_transform) = camera.single() else {
        return;
    };
    let rotation = camera_transform.compute_transform().rotation;
    for mut label in &mut labels {
        label.rotation = rotation;
    }
}

fn aquarium_audio(
    time: Res<Time>,
    pointer: Res<PointerWorld>,
    mut audio_state: ResMut<AquariumAudioState>,
    mut commands: Commands,
    mut clips: ResMut<Assets<DspSource>>,
    dsp_manager: Res<DspManager>,
    bodies: Query<(&Transform, &CelestialBody)>,
) {
    let now = time.elapsed_secs();

    if now >= audio_state.next_heartbeat {
        if let Some((transform, body)) = bodies
            .iter()
            .filter(|(_, body)| body.class == BodyClass::LivingSelf)
            .max_by(|(_, a), (_, b)| a.mass.total_cmp(&b.mass))
        {
            spawn_sound(
                &mut commands,
                &mut clips,
                &dsp_manager,
                aquarium_heartbeat,
                0.85 + body.mass * 0.04,
                0.42,
                0.18,
                transform.translation,
            );
        }
        audio_state.next_heartbeat = now + 2.15;
    }

    if now >= audio_state.next_chirp {
        let agent = bodies
            .iter()
            .filter(|(_, body)| body.class == BodyClass::Agent)
            .min_by(|(_, a), (_, b)| {
                let da = (now * 0.41 + a.phase).sin().abs();
                let db = (now * 0.41 + b.phase).sin().abs();
                da.total_cmp(&db)
            });
        if let Some((transform, body)) = agent {
            spawn_sound(
                &mut commands,
                &mut clips,
                &dsp_manager,
                aquarium_pluck,
                0.82 + body.phase.sin().abs() * 0.95,
                0.24,
                0.24,
                transform.translation,
            );
        }
        audio_state.next_chirp = now + 3.4;
    }

    let Some(pointer_position) = pointer.plane_position else {
        audio_state.touched_body_id = None;
        return;
    };

    let nearest = bodies
        .iter()
        .map(|(transform, body)| {
            (
                transform,
                body,
                transform
                    .translation
                    .truncate()
                    .distance(pointer_position.truncate()),
            )
        })
        .filter(|(_, _, distance)| *distance < 1.8)
        .min_by(|(_, _, a), (_, _, b)| a.total_cmp(b));

    let Some((transform, body, _)) = nearest else {
        audio_state.touched_body_id = None;
        return;
    };

    let already_touching = audio_state
        .touched_body_id
        .as_ref()
        .is_some_and(|id| id == &body.body_id);
    if !already_touching && now >= audio_state.next_touch {
        let frequency = match body.class {
            BodyClass::LivingSelf => 185.0,
            BodyClass::SleepingEpiphany => 240.0,
            BodyClass::Agent => 420.0 + body.phase.cos().abs() * 220.0,
        };
        let amplitude = match body.class {
            BodyClass::LivingSelf => 0.09,
            BodyClass::SleepingEpiphany => 0.04,
            BodyClass::Agent => 0.055,
        };
        spawn_sound(
            &mut commands,
            &mut clips,
            &dsp_manager,
            aquarium_pluck,
            frequency / 440.0,
            amplitude * 4.8,
            0.2,
            transform.translation,
        );
        audio_state.next_touch = now + 0.22;
    }
    audio_state.touched_body_id = Some(body.body_id.clone());
}

fn spawn_sound<D: DspGraph>(
    commands: &mut Commands,
    clips: &mut Assets<DspSource>,
    dsp_manager: &DspManager,
    graph: D,
    speed: f32,
    volume: f32,
    duration_secs: f32,
    position: Vec3,
) {
    let Some(source) = dsp_manager.get_graph(graph) else {
        return;
    };
    let handle = clips.add(source);
    commands.spawn((
        AudioPlayer::<DspSource>(handle),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(volume.clamp(0.0, 1.0)))
            .with_speed(speed.clamp(0.35, 2.6))
            .with_duration(Duration::from_secs_f32(duration_secs.max(0.03)))
            .with_spatial(true),
        Transform::from_translation(position),
    ));
}

fn aquarium_pluck() -> impl AudioUnit {
    (sine_hz(440.0) + triangle_hz(880.0) * 0.28 + sine_hz(1760.0) * 0.08) * 0.18
}

fn aquarium_heartbeat() -> impl AudioUnit {
    (sine_hz(72.0) + sine_hz(116.0) * 0.42) * 0.22
}

fn orbit_anchor(center: Vec3, radius: f32, phase: f32) -> Vec3 {
    center + Vec3::new(phase.cos() * radius, phase.sin() * radius, 0.0)
}

fn smooth_well(distance: f32, inner: f32, outer: f32) -> f32 {
    let t = ((outer - distance) / (outer - inner).max(0.001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct GravityWell {
    center: Vec2,
    mass: f32,
    radius: f32,
}

fn body_wells(bodies: &Query<(&Transform, &CelestialBody)>) -> Vec<GravityWell> {
    bodies
        .iter()
        .map(|(transform, body)| GravityWell {
            center: transform.translation.truncate(),
            mass: body.mass,
            radius: if body.class == BodyClass::LivingSelf {
                8.5
            } else {
                3.8
            },
        })
        .collect()
}

fn cursor_well(center: Vec2) -> GravityWell {
    GravityWell {
        center,
        mass: CURSOR_WELL_MASS,
        radius: CURSOR_WELL_RADIUS,
    }
}

fn gravity_height(point: Vec2, wells: &[GravityWell]) -> f32 {
    let mut height = GRID_Z;
    for well in wells {
        let distance = point.distance(well.center);
        let pulse = power_pulse(distance / well.radius);
        height -= pulse * well.mass * 0.42;
    }
    let breathing = (point.x * 0.055).sin() * (point.y * 0.041).cos() * 0.08;
    height + breathing
}

fn power_pulse(x: f32) -> f32 {
    if x >= 1.0 {
        return 0.0;
    }
    let core = 1.0 - x * x;
    core * core * (1.0 + 0.34 * (TAU * (1.0 - x)).sin())
}
