use bevy::asset::{AssetPlugin, RenderAssetUsages};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::GlobalAmbientLight;
use bevy::math::primitives::{Cuboid, Sphere};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use cultcache_rs::{CultCache, DatabaseEntry, SingleFileMessagePackBackingStore};
use cultnet_rs::{
    CultNetDocumentBinding, CultNetDocumentRegistry, CultNetMessage, CultNetWireContract,
    encode_cultnet_message_to_vec,
};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::path::PathBuf;

const GRID_HALF_EXTENT: f32 = 42.0;
const GRID_RESOLUTION: usize = 128;
const BODY_RADIUS: f32 = 0.9;
const SELF_RADIUS: f32 = 1.25;
const GRID_Z: f32 = 0.0;
const CURSOR_WELL_RADIUS: f32 = 4.6;
const CURSOR_WELL_MASS: f32 = 2.1;

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
        .insert_resource(PointerWorld::default())
        .insert_resource(GridDirty(true))
        .insert_resource(LiveStateAutosave(Timer::from_seconds(
            1.0,
            TimerMode::Repeating,
        )))
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
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                camera_input,
                update_camera,
                project_pointer_to_grid,
                update_cursor_visual,
                integrate_bodies,
                autosave_live_state,
                rebuild_grid,
                billboard_labels,
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
            target: Vec3::from_array(settings.camera_target),
            yaw: settings.camera_yaw,
            pitch: settings.camera_pitch,
            distance: settings.camera_distance,
        }
    }
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

#[derive(Component)]
struct AquariumCamera;

#[derive(Component)]
struct GridSurface;

#[derive(Component)]
struct AquariumDomainRoot;

#[derive(Component)]
struct BodyLabel;

#[derive(Component)]
struct CursorPlaneMarker;

#[derive(Component)]
struct CursorProbe;

#[derive(Component)]
struct CursorTip;

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

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<CultRuntimeBridge>,
) {
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
        AquariumCamera,
    ));

    spawn_domain(
        &mut commands,
        &mut meshes,
        &mut materials,
        &bridge,
        bridge.load_body_states().ok(),
    );
}

fn spawn_domain(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    bridge: &CultRuntimeBridge,
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

    let grid_mesh = meshes.add(build_heightfield(&[]));
    let grid_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.13, 0.34, 0.36, 0.58),
        emissive: LinearRgba::rgb(0.035, 0.14, 0.16),
        perceptual_roughness: 0.66,
        metallic: 0.0,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(grid_mesh),
        MeshMaterial3d(grid_material),
        Transform::default(),
        GridSurface,
        ChildOf(domain_root),
    ));

    let cursor_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.88, 0.98, 1.0, 0.72),
        emissive: LinearRgba::rgb(0.38, 0.88, 1.25),
        perceptual_roughness: 0.18,
        metallic: 0.16,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(
            meshes.add(
                Sphere::new(0.23)
                    .mesh()
                    .ico(2)
                    .expect("valid cursor sphere"),
            ),
        ),
        MeshMaterial3d(cursor_material.clone()),
        Transform::from_xyz(0.0, 0.0, GRID_Z),
        Visibility::Hidden,
        CursorPlaneMarker,
        ChildOf(domain_root),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.055, 0.055, 1.0))),
        MeshMaterial3d(cursor_material.clone()),
        Transform::default(),
        Visibility::Hidden,
        CursorProbe,
        ChildOf(domain_root),
    ));
    commands.spawn((
        Mesh3d(
            meshes.add(
                Sphere::new(0.16)
                    .mesh()
                    .ico(2)
                    .expect("valid cursor tip sphere"),
            ),
        ),
        MeshMaterial3d(cursor_material),
        Transform::from_xyz(0.0, 0.0, GRID_Z),
        Visibility::Hidden,
        CursorTip,
        ChildOf(domain_root),
    ));

    let body_states = cached_bodies.unwrap_or_else(default_body_states);
    let body_states = if body_states.is_empty() {
        default_body_states()
    } else {
        body_states
    };
    for state in body_states {
        spawn_body_from_state(commands, meshes, materials, domain_root, state);
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

fn spawn_body_from_state(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    domain_root: Entity,
    state: AquariumBodyState,
) {
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
    let body_material = match class {
        BodyClass::LivingSelf => StandardMaterial {
            base_color: Color::srgb(1.0, 0.93, 0.36),
            emissive: LinearRgba::rgb(2.4, 1.9, 0.42),
            metallic: 0.42,
            perceptual_roughness: 0.24,
            ..default()
        },
        BodyClass::SleepingEpiphany => StandardMaterial {
            base_color: Color::srgb(0.82, 0.9, 0.98),
            emissive: LinearRgba::rgb(0.10, 0.16, 0.22),
            metallic: 0.78,
            perceptual_roughness: 0.18,
            ..default()
        },
        BodyClass::Agent => StandardMaterial {
            base_color: Color::srgb(0.57, 0.75, 0.79),
            emissive: LinearRgba::rgb(0.04, 0.11, 0.12),
            metallic: 0.64,
            perceptual_roughness: 0.21,
            ..default()
        },
    };

    let body = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(radius).mesh().ico(4).expect("valid ico sphere"))),
            MeshMaterial3d(materials.add(body_material)),
            Transform::from_translation(position),
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
        camera_target: rig.target.to_array(),
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
        &mut meshes,
        &mut materials,
        &bridge,
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
    camera: Query<&Transform, With<AquariumCamera>>,
) {
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

fn project_pointer_to_grid(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<AquariumCamera>>,
    mut pointer: ResMut<PointerWorld>,
    mut dirty: ResMut<GridDirty>,
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

fn update_cursor_visual(
    pointer: Res<PointerWorld>,
    mut plane_marker: Query<(&mut Transform, &mut Visibility), With<CursorPlaneMarker>>,
    mut probe: Query<
        (&mut Transform, &mut Visibility),
        (
            With<CursorProbe>,
            Without<CursorPlaneMarker>,
            Without<CursorTip>,
        ),
    >,
    mut tip: Query<
        (&mut Transform, &mut Visibility),
        (
            With<CursorTip>,
            Without<CursorPlaneMarker>,
            Without<CursorProbe>,
        ),
    >,
) {
    let (Some(plane_position), Some(grid_position)) =
        (pointer.plane_position, pointer.grid_position)
    else {
        if let Ok((_, mut visibility)) = plane_marker.single_mut() {
            *visibility = Visibility::Hidden;
        }
        if let Ok((_, mut visibility)) = probe.single_mut() {
            *visibility = Visibility::Hidden;
        }
        if let Ok((_, mut visibility)) = tip.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    if let Ok((mut transform, mut visibility)) = plane_marker.single_mut() {
        transform.translation = plane_position;
        *visibility = Visibility::Visible;
    }
    if let Ok((mut transform, mut visibility)) = tip.single_mut() {
        transform.translation = grid_position;
        *visibility = Visibility::Visible;
    }
    if let Ok((mut transform, mut visibility)) = probe.single_mut() {
        let delta = plane_position - grid_position;
        let length = delta.length().max(0.001);
        transform.translation = grid_position + delta * 0.5;
        transform.scale = Vec3::new(1.0, 1.0, length);
        *visibility = Visibility::Visible;
    }
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

fn rebuild_grid(
    mut dirty: ResMut<GridDirty>,
    mut meshes: ResMut<Assets<Mesh>>,
    surface: Query<&Mesh3d, With<GridSurface>>,
    bodies: Query<(&Transform, &CelestialBody)>,
    pointer: Res<PointerWorld>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    let mut wells = body_wells(&bodies);
    if let Some(plane_position) = pointer.plane_position {
        wells.push(cursor_well(plane_position.truncate()));
    }

    let Ok(mesh_handle) = surface.single() else {
        return;
    };
    if let Some(mesh) = meshes.get_mut(&mesh_handle.0) {
        *mesh = build_heightfield(&wells);
    }
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

fn build_heightfield(wells: &[GravityWell]) -> Mesh {
    let vertex_count = (GRID_RESOLUTION + 1) * (GRID_RESOLUTION + 1);
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut heights = Vec::with_capacity(vertex_count);

    for y in 0..=GRID_RESOLUTION {
        for x in 0..=GRID_RESOLUTION {
            let uv = Vec2::new(
                x as f32 / GRID_RESOLUTION as f32,
                y as f32 / GRID_RESOLUTION as f32,
            );
            let xy = (uv * 2.0 - Vec2::ONE) * GRID_HALF_EXTENT;
            let height = gravity_height(xy, wells);
            positions.push([xy.x, xy.y, height]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([uv.x, uv.y]);
            heights.push(height);
        }
    }

    for y in 1..GRID_RESOLUTION {
        for x in 1..GRID_RESOLUTION {
            let left = sample_height(&heights, x - 1, y);
            let right = sample_height(&heights, x + 1, y);
            let down = sample_height(&heights, x, y - 1);
            let up = sample_height(&heights, x, y + 1);
            let normal = Vec3::new(left - right, down - up, 2.0).normalize();
            normals[y * (GRID_RESOLUTION + 1) + x] = normal.to_array();
        }
    }

    let mut indices = Vec::with_capacity(GRID_RESOLUTION * GRID_RESOLUTION * 6);
    let stride = GRID_RESOLUTION + 1;
    for y in 0..GRID_RESOLUTION {
        for x in 0..GRID_RESOLUTION {
            let i = (y * stride + x) as u32;
            indices.extend_from_slice(&[
                i,
                i + 1,
                i + stride as u32,
                i + 1,
                i + stride as u32 + 1,
                i + stride as u32,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn sample_height(heights: &[f32], x: usize, y: usize) -> f32 {
    heights[y * (GRID_RESOLUTION + 1) + x]
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
