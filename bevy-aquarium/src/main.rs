use aquarium_synth::{PatchUnit, presets as synth_presets};
use bevy::asset::AssetPlugin;
use bevy::audio::{AudioPlayer, PlaybackSettings, SpatialListener, Volume};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::core_pipeline::{FullscreenShader, core_3d::graph::Core3d, core_3d::graph::Node3d};
use bevy::ecs::query::QueryItem;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::{
    RenderApp, RenderStartup,
    diagnostic::{RecordDiagnostics, RenderDiagnosticsPlugin},
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_graph::{
        NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
    },
    render_resource::{
        BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
        BufferDescriptor, BufferUsages, CachedComputePipelineId, CachedRenderPipelineId,
        ColorTargetState, ColorWrites, ComputePassDescriptor, ComputePipelineDescriptor,
        FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
        RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
        ShaderType, TextureSampleType,
        binding_types::{
            sampler, storage_buffer_read_only_sized, storage_buffer_sized, texture_2d,
            uniform_buffer,
        },
    },
    renderer::{RenderContext, RenderDevice},
    view::{Hdr, ViewTarget},
};
use bevy_procedural_audio::prelude::{
    DspAppExt, DspGraph, DspManager, DspPlugin, DspSource, SourceType,
};
use cultcache_rs::{CultCache, DatabaseEntry, SingleFileMessagePackBackingStore};
use cultnet_rs::{
    CultNetDocumentBinding, CultNetDocumentRegistry, CultNetMessage, CultNetWireContract,
    encode_cultnet_message_to_vec,
};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const GRID_BASE_HALF_EXTENT: f32 = 42.0;
const GRID_MIN_HALF_EXTENT: f32 = 12.0;
const GRID_MAX_HALF_EXTENT: f32 = 720.0;
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
const LIGHT_GRID_WIDTH: usize = 32;
const LIGHT_GRID_HEIGHT: usize = 32;
const LIGHT_GRID_DEPTH: usize = 12;
const LIGHT_GRID_COUNT: usize = LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT * LIGHT_GRID_DEPTH;
const LIGHT_COEFFICIENT_COUNT: usize = LIGHT_GRID_COUNT * 4;
const LIGHT_BRICK_WIDTH: usize = 8;
const LIGHT_BRICK_HEIGHT: usize = 8;
const LIGHT_BRICK_DEPTH: usize = 4;
const LIGHT_BRICK_COUNT: usize = LIGHT_BRICK_WIDTH * LIGHT_BRICK_HEIGHT * LIGHT_BRICK_DEPTH;
const GRID_FIELD_SIZE: usize = 128;
const GRID_FIELD_COUNT: usize = GRID_FIELD_SIZE * GRID_FIELD_SIZE;

fn main() {
    let runtime_bridge = CultRuntimeBridge::load().unwrap_or_else(|err| {
        eprintln!("failed to initialize cultnet/cultcache bridge: {err}");
        CultRuntimeBridge::fallback()
    });
    let settings = runtime_bridge.settings.clone();
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.008, 0.011, 0.014)))
        .insert_resource(CameraRig::from_settings(&settings))
        .insert_resource(RendererDebugState::from_settings(
            &runtime_bridge.renderer_settings,
        ))
        .insert_resource(GridFrame::from_camera_settings(&settings))
        .insert_resource(LightingGridHistory::default())
        .insert_resource(PointerWorld::default())
        .insert_resource(GridDirty(true))
        .insert_resource(LiveStateAutosave(Timer::from_seconds(
            1.0,
            TimerMode::Repeating,
        )))
        .insert_resource(AquariumAudioState::default())
        .insert_resource(DebugUiState::default())
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
        .add_plugins(RenderDiagnosticsPlugin)
        .add_plugins(AquariumRaymarchPlugin)
        .add_plugins(DspPlugin::default())
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
                debug_ui_buttons,
                debug_terminal_input,
                sync_debug_ui,
                reload_domain_input,
                renderer_debug_input,
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

#[derive(Clone, Debug, PartialEq, DatabaseEntry)]
#[cultcache(type = "epiphany.aquarium.renderer-settings")]
struct AquariumRendererSettings {
    #[cultcache(key = 0)]
    schema_version: String,
    #[cultcache(key = 1)]
    debug_mode: String,
}

#[derive(Resource, Clone, Debug)]
struct DebugUiState {
    open: bool,
    active_tab: DebugTab,
    terminal_focused: bool,
    terminal_input: String,
    terminal_lines: Vec<String>,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            open: false,
            active_tab: DebugTab::Terminal,
            terminal_focused: false,
            terminal_input: String::new(),
            terminal_lines: vec![
                "Epiphany Aquarium debug terminal".to_string(),
                "Type `help`; commands are internal debug verbs.".to_string(),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugTab {
    Terminal,
}

struct DebugCommandSpec {
    name: &'static str,
    usage: &'static str,
    summary: &'static str,
}

const DEBUG_COMMANDS: &[DebugCommandSpec] = &[
    DebugCommandSpec {
        name: "help",
        usage: "help",
        summary: "list registered debug commands",
    },
    DebugCommandSpec {
        name: "clear",
        usage: "clear",
        summary: "clear the terminal output",
    },
    DebugCommandSpec {
        name: "renderer",
        usage: "renderer [mode|next|list]",
        summary: "inspect or change the renderer debug mode",
    },
];

struct DebugCommandResult {
    clear_terminal: bool,
    lines: Vec<String>,
}

impl Default for AquariumRendererSettings {
    fn default() -> Self {
        Self {
            schema_version: "epiphany.aquarium.renderer-settings.v0".to_string(),
            debug_mode: RendererDebugMode::Final.as_key().to_string(),
        }
    }
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
    renderer_settings: AquariumRendererSettings,
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
        let renderer_settings = match cache.get::<AquariumRendererSettings>("renderer")? {
            Some(renderer_settings) => renderer_settings,
            None => cache.put("renderer", &AquariumRendererSettings::default())?,
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
            ))
            .register(
                CultNetDocumentBinding::for_entry::<AquariumRendererSettings>(Some(
                    "epiphany.aquarium.renderer-settings.v0".to_string(),
                )),
            );

        let supported_document_types = vec![
            AquariumClientSettings::TYPE.to_string(),
            AquariumAgentPresence::TYPE.to_string(),
            AquariumDomainState::TYPE.to_string(),
            AquariumBodyState::TYPE.to_string(),
            AquariumRendererSettings::TYPE.to_string(),
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
            renderer_settings,
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
            renderer_settings: AquariumRendererSettings::default(),
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
        cache.register_entry_type::<AquariumRendererSettings>()?;
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

    fn save_renderer_settings(
        &mut self,
        renderer_settings: AquariumRendererSettings,
    ) -> anyhow::Result<()> {
        let mut cache = Self::open_cache(&self.settings_path)?;
        self.renderer_settings = cache.put("renderer", &renderer_settings)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RendererDebugMode {
    Final,
    HitCoverage,
    Depth,
    Normals,
    BrickOccupancy,
    IrradianceLuminance,
}

impl RendererDebugMode {
    const ALL: [Self; 6] = [
        Self::Final,
        Self::HitCoverage,
        Self::Depth,
        Self::Normals,
        Self::BrickOccupancy,
        Self::IrradianceLuminance,
    ];

    fn as_key(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::HitCoverage => "hit-coverage",
            Self::Depth => "depth",
            Self::Normals => "normals",
            Self::BrickOccupancy => "brick-occupancy",
            Self::IrradianceLuminance => "irradiance-luminance",
        }
    }

    fn from_key(value: &str) -> Self {
        Self::from_key_option(value).unwrap_or(Self::Final)
    }

    fn from_key_option(value: &str) -> Option<Self> {
        match value {
            "final" => Some(Self::Final),
            "hit-coverage" => Some(Self::HitCoverage),
            "depth" => Some(Self::Depth),
            "normals" => Some(Self::Normals),
            "brick-occupancy" => Some(Self::BrickOccupancy),
            "irradiance-luminance" | "sh-luminance" => Some(Self::IrradianceLuminance),
            _ => None,
        }
    }

    fn as_shader_value(self) -> f32 {
        match self {
            Self::Final => 0.0,
            Self::HitCoverage => 1.0,
            Self::Depth => 2.0,
            Self::Normals => 3.0,
            Self::BrickOccupancy => 4.0,
            Self::IrradianceLuminance => 5.0,
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Resource, Clone, Copy, Debug)]
struct RendererDebugState {
    mode: RendererDebugMode,
}

impl RendererDebugState {
    fn from_settings(settings: &AquariumRendererSettings) -> Self {
        Self {
            mode: RendererDebugMode::from_key(&settings.debug_mode),
        }
    }

    fn as_settings(self) -> AquariumRendererSettings {
        AquariumRendererSettings {
            schema_version: "epiphany.aquarium.renderer-settings.v0".to_string(),
            debug_mode: self.mode.as_key().to_string(),
        }
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
            settings.camera_pitch,
        )
    }

    fn from_camera(target: Vec3, distance: f32, pitch: f32) -> Self {
        Self {
            center: target.truncate(),
            half_extent: grid_half_extent_for_camera(distance, pitch),
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

fn grid_half_extent_for_camera(distance: f32, _pitch: f32) -> f32 {
    distance.clamp(GRID_MIN_HALF_EXTENT, GRID_MAX_HALF_EXTENT)
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

#[derive(Resource, Clone, Copy)]
struct LightingGridHistory {
    previous_center: Vec2,
    previous_half_extent: f32,
    initialized: bool,
}

impl Default for LightingGridHistory {
    fn default() -> Self {
        Self {
            previous_center: Vec2::ZERO,
            previous_half_extent: GRID_BASE_HALF_EXTENT,
            initialized: false,
        }
    }
}

#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
struct AquariumRaymarch {
    time: f32,
    body_count: f32,
    debug_mode: f32,
    debug_pad: f32,
    grid_center: Vec2,
    grid_half_extent: f32,
    previous_grid_center: Vec2,
    previous_grid_half_extent: f32,
    delta_time: f32,
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
    body_seeds: [Vec4; MAX_RAYMARCH_BODIES],
    froxel_masks: [UVec4; RAYMARCH_FROXEL_MASK_WORDS],
}

impl Default for AquariumRaymarch {
    fn default() -> Self {
        Self {
            time: 0.0,
            body_count: 0.0,
            debug_mode: 0.0,
            debug_pad: 0.0,
            grid_center: Vec2::ZERO,
            grid_half_extent: GRID_BASE_HALF_EXTENT,
            previous_grid_center: Vec2::ZERO,
            previous_grid_half_extent: GRID_BASE_HALF_EXTENT,
            delta_time: 1.0 / 60.0,
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
            body_seeds: [Vec4::ZERO; MAX_RAYMARCH_BODIES],
            froxel_masks: [UVec4::ZERO; RAYMARCH_FROXEL_MASK_WORDS],
        }
    }
}

struct AquariumRaymarchPlugin;

impl Plugin for AquariumRaymarchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<AquariumRaymarch>::default(),
            UniformComponentPlugin::<AquariumRaymarch>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(RenderStartup, init_aquarium_raymarch_pipeline);
        render_app
            .add_render_graph_node::<ViewNodeRunner<AquariumRaymarchNode>>(
                Core3d,
                AquariumRaymarchLabel,
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::EndMainPass,
                    AquariumRaymarchLabel,
                    Node3d::StartMainPassPostProcessing,
                ),
            );
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct AquariumRaymarchLabel;

#[derive(Resource)]
struct AquariumRaymarchPipeline {
    compute_layout: BindGroupLayoutDescriptor,
    render_layout: BindGroupLayoutDescriptor,
    render_sampler: Sampler,
    render_pipeline_id: CachedRenderPipelineId,
    height_pipeline_id: CachedComputePipelineId,
    compute_pipeline_id: CachedComputePipelineId,
    brick_pipeline_id: CachedComputePipelineId,
}

#[derive(Resource)]
struct AquariumLightBuffers {
    buffers: [Buffer; 2],
    brick_occupancy: Buffer,
    grid_height: Buffer,
    frame: AtomicU32,
}

fn init_aquarium_raymarch_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let storage_size = (LIGHT_COEFFICIENT_COUNT * size_of::<Vec4>()) as u64;
    let brick_storage_size = (LIGHT_BRICK_COUNT * size_of::<u32>()) as u64;
    let grid_height_storage_size = (GRID_FIELD_COUNT * size_of::<Vec4>()) as u64;
    let buffers = [
        render_device.create_buffer(&BufferDescriptor {
            label: Some("aquarium_irradiance_volume_a"),
            size: storage_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        render_device.create_buffer(&BufferDescriptor {
            label: Some("aquarium_irradiance_volume_b"),
            size: storage_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
    ];
    let brick_occupancy = render_device.create_buffer(&BufferDescriptor {
        label: Some("aquarium_light_brick_occupancy"),
        size: brick_storage_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let grid_height = render_device.create_buffer(&BufferDescriptor {
        label: Some("aquarium_grid_height_field"),
        size: grid_height_storage_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let compute_layout = BindGroupLayoutDescriptor::new(
        "aquarium_raymarch_compute_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::COMPUTE,
            (
                (2, uniform_buffer::<AquariumRaymarch>(true)),
                (4, storage_buffer_read_only_sized(false, None)),
                (5, storage_buffer_sized(false, None)),
                (6, storage_buffer_sized(false, None)),
                (7, storage_buffer_sized(false, None)),
            ),
        ),
    );
    let render_layout = BindGroupLayoutDescriptor::new(
        "aquarium_raymarch_render_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_2d(TextureSampleType::Float { filterable: true })),
                (1, sampler(SamplerBindingType::Filtering)),
                (2, uniform_buffer::<AquariumRaymarch>(true)),
                (3, storage_buffer_read_only_sized(false, None)),
                (7, storage_buffer_sized(false, None)),
            ),
        ),
    );
    let render_sampler = render_device.create_sampler(&SamplerDescriptor::default());

    let shader = asset_server.load("shaders/aquarium_raymarch.wgsl");
    let height_pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("aquarium_grid_height_compute".into()),
        layout: vec![compute_layout.clone()],
        shader: shader.clone(),
        entry_point: Some("cs_update_grid_height".into()),
        ..default()
    });
    let render_pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("aquarium_raymarch_render_pipeline".into()),
        layout: vec![render_layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: shader.clone(),
            entry_point: Some("fs_main".into()),
            targets: vec![Some(ColorTargetState {
                format: ViewTarget::TEXTURE_FORMAT_HDR,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    });
    let compute_pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("aquarium_irradiance_grid_compute".into()),
        layout: vec![compute_layout.clone()],
        shader,
        entry_point: Some("cs_grid_lighting".into()),
        ..default()
    });
    let shader = asset_server.load("shaders/aquarium_raymarch.wgsl");
    let brick_pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("aquarium_light_brick_occupancy_compute".into()),
        layout: vec![compute_layout.clone()],
        shader,
        entry_point: Some("cs_update_light_bricks".into()),
        ..default()
    });

    commands.insert_resource(AquariumRaymarchPipeline {
        compute_layout,
        render_layout,
        render_sampler,
        render_pipeline_id,
        height_pipeline_id,
        compute_pipeline_id,
        brick_pipeline_id,
    });
    commands.insert_resource(AquariumLightBuffers {
        buffers,
        brick_occupancy,
        grid_height,
        frame: AtomicU32::new(0),
    });
}

#[derive(Default)]
struct AquariumRaymarchNode;

impl ViewNode for AquariumRaymarchNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static AquariumRaymarch,
        &'static DynamicUniformIndex<AquariumRaymarch>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, _settings, settings_index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipeline = world.resource::<AquariumRaymarchPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let Some(compute_pipeline) =
            pipeline_cache.get_compute_pipeline(pipeline.compute_pipeline_id)
        else {
            return Ok(());
        };
        let Some(height_pipeline) =
            pipeline_cache.get_compute_pipeline(pipeline.height_pipeline_id)
        else {
            return Ok(());
        };
        let Some(brick_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.brick_pipeline_id)
        else {
            return Ok(());
        };
        let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline.render_pipeline_id)
        else {
            return Ok(());
        };

        let uniforms = world.resource::<ComponentUniforms<AquariumRaymarch>>();
        let Some(settings_binding) = uniforms.uniforms().binding() else {
            return Ok(());
        };

        let buffers = world.resource::<AquariumLightBuffers>();
        let frame = buffers.frame.fetch_add(1, Ordering::Relaxed);
        let read_index = (frame & 1) as usize;
        let write_index = 1usize - read_index;

        let compute_bind_group = render_context.render_device().create_bind_group(
            "aquarium_irradiance_compute_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline.compute_layout),
            &BindGroupEntries::with_indices((
                (2, settings_binding.clone()),
                (4, buffers.buffers[read_index].as_entire_binding()),
                (5, buffers.buffers[write_index].as_entire_binding()),
                (6, buffers.brick_occupancy.as_entire_binding()),
                (7, buffers.grid_height.as_entire_binding()),
            )),
        );
        {
            let diagnostics = render_context.diagnostic_recorder();
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("aquarium_light_compute"),
                        ..default()
                    });
            let height_span = diagnostics.pass_span(&mut pass, "aquarium_grid_height_compute");
            pass.set_pipeline(height_pipeline);
            pass.set_bind_group(0, &compute_bind_group, &[settings_index.index()]);
            pass.dispatch_workgroups(GRID_FIELD_COUNT.div_ceil(64) as u32, 1, 1);
            height_span.end(&mut pass);
            let brick_span = diagnostics.pass_span(&mut pass, "aquarium_light_brick_occupancy");
            pass.set_pipeline(brick_pipeline);
            pass.set_bind_group(0, &compute_bind_group, &[settings_index.index()]);
            pass.dispatch_workgroups(LIGHT_BRICK_COUNT.div_ceil(64) as u32, 1, 1);
            brick_span.end(&mut pass);
            let sh_span = diagnostics.pass_span(&mut pass, "aquarium_irradiance_grid_lighting");
            pass.set_pipeline(compute_pipeline);
            pass.set_bind_group(0, &compute_bind_group, &[settings_index.index()]);
            pass.dispatch_workgroups(LIGHT_GRID_COUNT.div_ceil(64) as u32, 1, 1);
            sh_span.end(&mut pass);
        }

        let post_process = view_target.post_process_write();
        let bind_group = render_context.render_device().create_bind_group(
            "aquarium_raymarch_render_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline.render_layout),
            &BindGroupEntries::with_indices((
                (0, post_process.source),
                (1, &pipeline.render_sampler),
                (2, settings_binding.clone()),
                (3, buffers.buffers[write_index].as_entire_binding()),
                (7, buffers.grid_height.as_entire_binding()),
            )),
        );

        {
            let diagnostics = render_context.diagnostic_recorder();
            let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("aquarium_raymarch_render"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let raymarch_span = diagnostics.pass_span(&mut render_pass, "aquarium_raymarch");
            render_pass.set_render_pipeline(render_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
            render_pass.draw(0..3, 0..1);
            raymarch_span.end(&mut render_pass);
        }

        Ok(())
    }
}

#[derive(Component)]
struct AquariumCamera;

#[derive(Component)]
struct AquariumDomainRoot;

#[derive(Component)]
struct BodyLabel;

#[derive(Component)]
struct DebugUiRoot;

#[derive(Component)]
struct DebugUiPanel;

#[derive(Component)]
struct DebugUiTabRail;

#[derive(Component)]
struct DebugTriggerButton;

#[derive(Component)]
struct DebugTerminalTabButton;

#[derive(Component)]
struct DebugTerminalPanel;

#[derive(Component)]
struct DebugTerminalOutput;

#[derive(Component)]
struct DebugTerminalInput;

#[derive(Component)]
struct DebugTerminalInputText;

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
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 46.0_f32.to_radians(),
            near: 0.5,
            far: 240.0,
            ..default()
        }),
        Transform::default(),
        Msaa::Off,
        Hdr,
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.08,
            low_frequency_boost: 0.92,
            low_frequency_boost_curvature: 0.96,
            high_pass_frequency: 0.38,
            prefilter: BloomPrefilter {
                threshold: 1.15,
                threshold_softness: 0.72,
            },
            composite_mode: BloomCompositeMode::EnergyConserving,
            max_mip_dimension: 1024,
            scale: Vec2::ONE,
        },
        SpatialListener::default(),
        AquariumRaymarch::default(),
        AquariumCamera,
        IsDefaultUiCamera,
    ));
    spawn_debug_ui(&mut commands);

    spawn_domain(
        &mut commands,
        &bridge,
        *grid_frame,
        bridge.load_body_states().ok(),
    );
}

fn spawn_debug_ui(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ZIndex(20),
            DebugUiRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(10.0),
                    top: Val::Px(10.0),
                    width: Val::Px(34.0),
                    height: Val::Px(34.0),
                    border: UiRect::all(Val::Px(1.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.015, 0.022, 0.025, 0.64)),
                BorderColor::all(Color::srgba(0.58, 0.82, 0.76, 0.34)),
                DebugTriggerButton,
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new(">_"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.76, 0.96, 0.88, 0.9)),
                ));
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(54.0),
                    top: Val::Px(10.0),
                    width: Val::Percent(46.0),
                    height: Val::Px(34.0),
                    display: Display::None,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                },
                DebugUiTabRail,
            ))
            .with_children(|tabs| {
                tabs.spawn((
                    Button,
                    Node {
                        width: Val::Px(34.0),
                        height: Val::Px(34.0),
                        border: UiRect::all(Val::Px(1.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.03, 0.05, 0.048, 0.72)),
                    BorderColor::all(Color::srgba(0.58, 0.82, 0.76, 0.38)),
                    DebugTerminalTabButton,
                ))
                .with_children(|tab| {
                    tab.spawn((
                        Text::new(">_"),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.78, 0.98, 0.9, 0.9)),
                    ));
                });
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(54.0),
                    top: Val::Px(54.0),
                    width: Val::Percent(46.0),
                    height: Val::Percent(88.0),
                    border: UiRect::all(Val::Px(1.0)),
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.006, 0.012, 0.013, 0.82)),
                BorderColor::all(Color::srgba(0.58, 0.82, 0.76, 0.25)),
                DebugUiPanel,
                DebugTerminalPanel,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("DEBUG / TERMINAL"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.58, 0.82, 0.76, 0.72)),
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.82, 0.96, 0.88, 0.92)),
                    Node {
                        flex_grow: 1.0,
                        width: Val::Percent(100.0),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    DebugTerminalOutput,
                ));
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(38.0),
                            border: UiRect::all(Val::Px(1.0)),
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(10.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.03, 0.03, 0.92)),
                        BorderColor::all(Color::srgba(0.58, 0.82, 0.76, 0.28)),
                        DebugTerminalInput,
                    ))
                    .with_children(|input| {
                        input.spawn((
                            Text::new("> "),
                            TextFont {
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.82, 1.0, 0.9, 0.95)),
                            DebugTerminalInputText,
                        ));
                    });
            });
        });
}

fn debug_ui_buttons(
    mut debug: ResMut<DebugUiState>,
    trigger: Query<&Interaction, (Changed<Interaction>, With<DebugTriggerButton>)>,
    terminal_tab: Query<&Interaction, (Changed<Interaction>, With<DebugTerminalTabButton>)>,
    terminal_input: Query<&Interaction, (Changed<Interaction>, With<DebugTerminalInput>)>,
) {
    for interaction in &trigger {
        if *interaction == Interaction::Pressed {
            debug.open = !debug.open;
            debug.terminal_focused = debug.open;
        }
    }
    for interaction in &terminal_tab {
        if *interaction == Interaction::Pressed {
            debug.open = true;
            debug.active_tab = DebugTab::Terminal;
            debug.terminal_focused = true;
        }
    }
    for interaction in &terminal_input {
        if *interaction == Interaction::Pressed {
            debug.open = true;
            debug.active_tab = DebugTab::Terminal;
            debug.terminal_focused = true;
        }
    }
}

fn debug_terminal_input(
    mut debug: ResMut<DebugUiState>,
    mut renderer_debug: ResMut<RendererDebugState>,
    mut bridge: ResMut<CultRuntimeBridge>,
    mut keyboard_events: MessageReader<KeyboardInput>,
) {
    if !debug.open || debug.active_tab != DebugTab::Terminal || !debug.terminal_focused {
        return;
    }

    for event in keyboard_events.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        match event.key_code {
            KeyCode::Escape => {
                debug.terminal_focused = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let command = debug.terminal_input.trim().to_string();
                debug.terminal_input.clear();
                if command.is_empty() {
                    continue;
                }
                debug.terminal_lines.push(format!("> {command}"));
                let result = execute_debug_command(&command, &mut renderer_debug, &mut bridge);
                if result.clear_terminal {
                    debug.terminal_lines.clear();
                }
                for line in result.lines {
                    debug.terminal_lines.push(line);
                }
                let keep = 28;
                if debug.terminal_lines.len() > keep {
                    let drain = debug.terminal_lines.len() - keep;
                    debug.terminal_lines.drain(0..drain);
                }
            }
            KeyCode::Backspace => {
                debug.terminal_input.pop();
            }
            _ => {
                if let Some(text) = &event.text {
                    for character in text.chars() {
                        if !character.is_control() {
                            debug.terminal_input.push(character);
                        }
                    }
                }
            }
        }
    }
}

fn execute_debug_command(
    command: &str,
    renderer_debug: &mut RendererDebugState,
    bridge: &mut CultRuntimeBridge,
) -> DebugCommandResult {
    let mut parts = command.split_whitespace();
    let Some(verb) = parts.next() else {
        return DebugCommandResult {
            clear_terminal: false,
            lines: Vec::new(),
        };
    };

    match verb {
        "help" => DebugCommandResult {
            clear_terminal: false,
            lines: DEBUG_COMMANDS
                .iter()
                .map(|spec| format!("{:<10} {:<24} {}", spec.name, spec.usage, spec.summary))
                .collect(),
        },
        "clear" => DebugCommandResult {
            clear_terminal: true,
            lines: Vec::new(),
        },
        "renderer" => debug_renderer_command(parts.collect(), renderer_debug, bridge),
        unknown => DebugCommandResult {
            clear_terminal: false,
            lines: vec![
                format!("unknown debug command `{unknown}`"),
                "type `help` for registered commands".to_string(),
            ],
        },
    }
}

fn debug_renderer_command(
    args: Vec<&str>,
    renderer_debug: &mut RendererDebugState,
    bridge: &mut CultRuntimeBridge,
) -> DebugCommandResult {
    let Some(first) = args.first().copied() else {
        return DebugCommandResult {
            clear_terminal: false,
            lines: vec![format!("renderer mode: {}", renderer_debug.mode.as_key())],
        };
    };

    if first == "list" {
        return DebugCommandResult {
            clear_terminal: false,
            lines: RendererDebugMode::ALL
                .iter()
                .map(|mode| mode.as_key().to_string())
                .collect(),
        };
    }

    let requested = if first == "next" {
        renderer_debug.mode.next()
    } else {
        let Some(mode) = RendererDebugMode::from_key_option(first) else {
            return DebugCommandResult {
                clear_terminal: false,
                lines: vec![
                    format!("unknown renderer mode `{first}`"),
                    "try `renderer list`".to_string(),
                ],
            };
        };
        mode
    };
    renderer_debug.mode = requested;
    let save_result = bridge.save_renderer_settings(renderer_debug.as_settings());
    let mut lines = vec![format!("renderer mode: {}", renderer_debug.mode.as_key())];
    if let Err(err) = save_result {
        lines.push(format!("failed to persist renderer mode: {err}"));
    }

    DebugCommandResult {
        clear_terminal: false,
        lines,
    }
}

fn sync_debug_ui(
    debug: Res<DebugUiState>,
    mut tab_rails: Query<&mut Node, (With<DebugUiTabRail>, Without<DebugUiPanel>)>,
    mut panels: Query<&mut Node, (With<DebugUiPanel>, Without<DebugUiTabRail>)>,
    mut output_text: Query<&mut Text, (With<DebugTerminalOutput>, Without<DebugTerminalInput>)>,
    mut input_text: Query<&mut Text, (With<DebugTerminalInputText>, Without<DebugTerminalOutput>)>,
    mut trigger_color: Query<&mut BackgroundColor, With<DebugTriggerButton>>,
    mut tab_color: Query<
        &mut BackgroundColor,
        (With<DebugTerminalTabButton>, Without<DebugTriggerButton>),
    >,
    mut input_color: Query<
        &mut BackgroundColor,
        (
            With<DebugTerminalInput>,
            Without<DebugTriggerButton>,
            Without<DebugTerminalTabButton>,
        ),
    >,
) {
    let display = if debug.open {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut tab_rails {
        node.display = display;
    }
    for mut node in &mut panels {
        node.display = display;
    }
    for mut text in &mut output_text {
        text.0 = debug.terminal_lines.join("\n");
    }
    for mut text in &mut input_text {
        let caret = if debug.terminal_focused { "_" } else { "" };
        text.0 = format!("> {}{}", debug.terminal_input, caret);
    }
    for mut color in &mut trigger_color {
        color.0 = if debug.open {
            Color::srgba(0.06, 0.14, 0.12, 0.84)
        } else {
            Color::srgba(0.015, 0.022, 0.025, 0.64)
        };
    }
    for mut color in &mut tab_color {
        color.0 = Color::srgba(0.045, 0.10, 0.086, 0.82);
    }
    for mut color in &mut input_color {
        color.0 = if debug.terminal_focused {
            Color::srgba(0.025, 0.07, 0.058, 0.94)
        } else {
            Color::srgba(0.02, 0.03, 0.03, 0.92)
        };
    }
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
        GridFrame::from_camera(rig.target, rig.distance, rig.pitch),
        Some(cached_bodies),
    );
    info!(
        "domain reload complete; generation {}",
        bridge.domain_state.reload_generation
    );
}

fn renderer_debug_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug_state: ResMut<RendererDebugState>,
    mut bridge: ResMut<CultRuntimeBridge>,
) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }

    debug_state.mode = debug_state.mode.next();
    if let Err(err) = bridge.save_renderer_settings(debug_state.as_settings()) {
        warn!("failed to save renderer debug mode: {err}");
    }
    info!("renderer debug mode: {}", debug_state.mode.as_key());
}

fn camera_input(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    debug: Res<DebugUiState>,
    mut rig: ResMut<CameraRig>,
    mut dirty: ResMut<GridDirty>,
    camera: Query<&Transform, With<AquariumCamera>>,
) {
    if debug.open && debug.terminal_focused {
        return;
    }

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
    let next = GridFrame::from_camera(rig.target, rig.distance, rig.pitch);
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
    debug_state: Res<RendererDebugState>,
    mut lighting_history: ResMut<LightingGridHistory>,
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
    raymarch.debug_mode = debug_state.mode.as_shader_value();
    raymarch.debug_pad = 0.0;
    raymarch.grid_center = grid_frame.center;
    raymarch.grid_half_extent = grid_frame.half_extent;
    if lighting_history.initialized {
        raymarch.previous_grid_center = lighting_history.previous_center;
        raymarch.previous_grid_half_extent = lighting_history.previous_half_extent;
    } else {
        raymarch.previous_grid_center = grid_frame.center;
        raymarch.previous_grid_half_extent = grid_frame.half_extent;
    }
    raymarch.delta_time = time.delta_secs().clamp(1.0 / 240.0, 1.0 / 15.0);
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
    raymarch.body_seeds = [Vec4::ZERO; MAX_RAYMARCH_BODIES];
    raymarch.froxel_masks = [UVec4::ZERO; RAYMARCH_FROXEL_MASK_WORDS];
    let camera_position = raymarch.camera_position.truncate();
    let depth_near = raymarch.depth_near;
    let depth_span = raymarch.depth_span;
    let mut count = 0usize;
    let mut body_entries: Vec<_> = bodies.iter().collect();
    body_entries.sort_by(|(_, left), (_, right)| left.body_id.cmp(&right.body_id));
    for (transform, body) in body_entries.into_iter().take(MAX_RAYMARCH_BODIES) {
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
        raymarch.body_seeds[count] = body_seed_uniform(body);
        bin_body_into_froxels(
            &mut raymarch.froxel_masks,
            count,
            radius * (1.20 + self_flag * 0.16),
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
    lighting_history.previous_center = grid_frame.center;
    lighting_history.previous_half_extent = grid_frame.half_extent;
    lighting_history.initialized = true;
}

fn body_seed_uniform(body: &CelestialBody) -> Vec4 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in body.body_id.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let h0 = hash01(hash);
    let h1 = hash01(hash.rotate_left(11) ^ 0x9e37_79b9);
    let h2 = hash01(hash.rotate_left(23) ^ 0x85eb_ca6b);
    Vec4::new(
        12.0 + h0 * 71.0 + body.phase * 0.13,
        27.0 + h1 * 83.0 + body.mass * 0.07,
        43.0 + h2 * 97.0 + body.phase * 0.19,
        0.0,
    )
}

fn hash01(value: u32) -> f32 {
    (value as f32) * (1.0 / u32::MAX as f32)
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

fn aquarium_pluck() -> PatchUnit {
    PatchUnit::new(synth_presets::aquarium_pluck())
}

fn aquarium_heartbeat() -> PatchUnit {
    PatchUnit::new(synth_presets::aquarium_heartbeat())
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
