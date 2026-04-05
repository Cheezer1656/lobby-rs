use serde::Deserialize;
use std::collections::HashMap;
use std::time::Instant;
use valence::entity::living::Health;
use valence::entity::{block_display, display, item_display, text_display};
use valence::event_loop::PacketEvent;
use valence::inventory::HeldItem;
use valence::math::{EulerRot, Quat};
use valence::message::ChatMessageEvent;
use valence::nbt::compound;
use valence::prelude::*;
use valence::protocol::packets::play::{
    PlayerInteractBlockC2s, PlayerInteractEntityC2s, PlayerInteractItemC2s,
};
use valence::scoreboard::{Objective, ObjectiveBundle, ObjectiveDisplay, ObjectiveScores};
use valence_anvil::AnvilLevel;
use valence_boss_bar::{BossBarBundle, BossBarColor, BossBarDivision, BossBarHealth, BossBarTitle};

const CONFIG_PATH: &str = "config.toml";
const WORLD_PATH: &str = "world";

#[derive(Debug)]
struct GameModeValue(GameMode);

impl<'de> Deserialize<'de> for GameModeValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let game_mode = match s.to_lowercase().as_str() {
            "survival" => GameMode::Survival,
            "creative" => GameMode::Creative,
            "adventure" => GameMode::Adventure,
            "spectator" => GameMode::Spectator,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid game mode: {}",
                    s
                )));
            }
        };
        Ok(GameModeValue(game_mode))
    }
}

#[derive(Debug)]
struct BossBarColorValue(BossBarColor);

impl<'de> Deserialize<'de> for BossBarColorValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let color = match s.to_lowercase().as_str() {
            "pink" => BossBarColor::Pink,
            "blue" => BossBarColor::Blue,
            "red" => BossBarColor::Red,
            "green" => BossBarColor::Green,
            "yellow" => BossBarColor::Yellow,
            "purple" => BossBarColor::Purple,
            "white" => BossBarColor::White,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid boss bar color: {}",
                    s
                )));
            }
        };
        Ok(BossBarColorValue(color))
    }
}

#[derive(Debug)]
struct BossBarDivisionValue(BossBarDivision);

impl<'de> Deserialize<'de> for BossBarDivisionValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = u8::deserialize(deserializer)?;

        let division = match id {
            0 => BossBarDivision::NoDivision,
            1 => BossBarDivision::SixNotches,
            2 => BossBarDivision::TenNotches,
            3 => BossBarDivision::TwelveNotches,
            4 => BossBarDivision::TwentyNotches,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid boss bar division: {}. Must be between 0 and 4 (inclusive).",
                    id
                )));
            }
        };

        Ok(BossBarDivisionValue(division))
    }
}

#[derive(Debug)]
struct TextValue(Text);

impl<'de> Deserialize<'de> for TextValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(TextValue(Text::from_legacy(&s)))
    }
}

#[derive(Debug)]
struct DBlockPosition(DVec3);

impl<'de> Deserialize<'de> for DBlockPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = <[i64; 3]>::deserialize(deserializer)?;
        Ok(DBlockPosition(DVec3::new(
            arr[0] as f64,
            arr[1] as f64,
            arr[2] as f64,
        )))
    }
}

#[derive(Debug, Deserialize)]
struct ParkourCourse {
    name: TextValue,
    checkpoints: Vec<DBlockPosition>,
}

#[derive(Debug, Clone, Copy)]
struct PositionValue(Position);

impl<'de> Deserialize<'de> for PositionValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = <[f64; 3]>::deserialize(deserializer)?;
        Ok(PositionValue(Position(arr.into())))
    }
}

#[derive(Debug, Deserialize)]
struct TextDisplayEntry {
    text: TextValue,
    position: PositionValue,
    rotation: [f32; 3],
    scale: Option<[f32; 3]>,
}

#[derive(Debug)]
struct ItemStackValue(ItemStack);

impl<'de> Deserialize<'de> for ItemStackValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match ItemKind::from_str(&s) {
            Some(item_kind) => Ok(ItemStackValue(ItemStack::new(item_kind, 1, None))),
            None => Err(serde::de::Error::custom(format!(
                "Invalid item kind: {}",
                s
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ItemDisplayEntry {
    item: ItemStackValue,
    position: PositionValue,
    rotation: [f32; 3],
    scale: Option<[f32; 3]>,
}

#[derive(Debug)]
struct BlockStateValue(BlockState);

impl<'de> Deserialize<'de> for BlockStateValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match BlockKind::from_str(&s) {
            Some(block_kind) => Ok(BlockStateValue(BlockState::from_kind(block_kind))),
            None => Err(serde::de::Error::custom(format!(
                "Invalid block kind: {}",
                s
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BlockDisplayEntry {
    block: BlockStateValue,
    position: PositionValue,
    rotation: [f32; 3],
    scale: Option<[f32; 3]>,
}

#[derive(Resource, Deserialize, Debug)]
struct ServerConfig {
    spawn_chunk_corners: Option<[[i32; 2]; 2]>,
    spawn_position: [f64; 3],
    spawn_rotation: [f32; 2],
    game_mode: GameModeValue,
    kill_oob_players: bool,
    minimum_y_level: i32,
    chat_enabled: bool,
    scoreboard_title: Option<TextValue>,
    scoreboard_text: Option<Vec<String>>,
    boss_bar_text: Option<TextValue>,
    boss_bar_color: Option<BossBarColorValue>,
    boss_bar_division: Option<BossBarDivisionValue>,
    title_text: Option<TextValue>,
    title_subtext: Option<TextValue>,
    title_animation_enabled: bool,
    title_fade_in: Option<i32>,
    title_stay: Option<i32>,
    title_fade_out: Option<i32>,
    parkour: Vec<ParkourCourse>,
    text_displays: Vec<TextDisplayEntry>,
    item_displays: Vec<ItemDisplayEntry>,
    block_displays: Vec<BlockDisplayEntry>,
}

#[derive(Component)]
struct ParkourTracker {
    course_index: usize,
    checkpoint_index: usize,
    start_time: Instant,
    actionbar_value: f32,
}

fn main() {
    let config: ServerConfig = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(config_str) => match toml::from_str(&config_str) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to parse config file: {}", e);
                return;
            }
        },
        Err(e) => {
            eprintln!("Failed to read config file: {}", e);
            return;
        }
    };

    for course in &config.parkour {
        if course.checkpoints.len() < 2 {
            eprintln!(
                "Parkour course '{}' must have at least 2 checkpoints.",
                course.name.0.to_string()
            );
            return;
        }
    }

    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                despawn_disconnected_clients,
                init_clients,
                item_interactions,
                check_for_parkour_start.before(item_interactions),
                update_parkour_tracker,
                update_parkour_actionbar_status,
            ),
        );

    if config.kill_oob_players {
        app.add_systems(Update, reset_oob_players);
    }

    if config.chat_enabled {
        app.add_systems(Update, broadcast_chat_message);
    }

    app.insert_resource(config);
    app.run();
}

fn setup(
    mut commands: Commands,
    dimensions: Res<DimensionTypeRegistry>,
    biomes: Res<BiomeRegistry>,
    server: Res<Server>,
    config: Res<ServerConfig>,
) {
    let layer = LayerBundle::new(ident!("overworld"), &dimensions, &biomes, &server);
    let mut level = AnvilLevel::new(WORLD_PATH, &biomes);

    if let Some(corners) = config.spawn_chunk_corners {
        for x in corners[0][0]..=corners[1][0] {
            for z in corners[0][1]..=corners[1][1] {
                let pos = ChunkPos::new(x, z);

                level.ignored_chunks.insert(pos);
                level.force_chunk_load(pos);
            }
        }
    }

    let layer_id = EntityLayerId(commands.spawn((layer, level)).id());

    if let Some(scoreboard_title) = &config.scoreboard_title {
        let objectives = ObjectiveScores::with_map(
            config
                .scoreboard_text
                .as_ref()
                .unwrap_or(&Vec::new())
                .iter()
                .rev()
                .enumerate()
                .map(|(i, text)| (text.clone(), i as i32))
                .collect::<HashMap<_, _>>(),
        );
        commands.spawn(
            ObjectiveBundle {
                name: Objective::new("sidebar"),
                display: ObjectiveDisplay(scoreboard_title.0.clone()),
                scores: objectives,
                layer: layer_id,
                ..Default::default()
            },
        );
    }

    if let Some(boss_bar_text) = &config.boss_bar_text {
        let mut boss_bar_bundle = BossBarBundle {
            title: BossBarTitle(boss_bar_text.0.clone()),
            health: BossBarHealth(1.0),
            layer: layer_id,
            ..Default::default()
        };

        if let Some(boss_bar_color) = &config.boss_bar_color {
            boss_bar_bundle.style.color = boss_bar_color.0;
        }

        if let Some(boss_bar_division) = &config.boss_bar_division {
            boss_bar_bundle.style.division = boss_bar_division.0;
        }

        commands.spawn(boss_bar_bundle);
    }

    for text_display in &config.text_displays {
        commands.spawn(text_display::TextDisplayEntityBundle {
            text_display_text: text_display::Text(text_display.text.0.clone()),
            position: text_display.position.0,
            display_right_rotation: display::RightRotation(rotation_to_quat(text_display.rotation)),
            display_scale: display::Scale(text_display.scale.unwrap_or([1.0; 3]).into()),
            layer: layer_id,
            ..Default::default()
        });
    }

    for item_display in &config.item_displays {
        commands.spawn(item_display::ItemDisplayEntityBundle {
            item_display_item: item_display::Item(item_display.item.0.clone()),
            position: item_display.position.0,
            display_right_rotation: display::RightRotation(rotation_to_quat(item_display.rotation)),
            display_scale: display::Scale(item_display.scale.unwrap_or([1.0; 3]).into()),
            layer: layer_id,
            ..Default::default()
        });
    }

    for block_display in &config.block_displays {
        commands.spawn(block_display::BlockDisplayEntityBundle {
            block_display_block_state: block_display::BlockState(block_display.block.0.clone()),
            position: block_display.position.0,
            display_right_rotation: display::RightRotation(rotation_to_quat(
                block_display.rotation,
            )),
            display_scale: display::Scale(block_display.scale.unwrap_or([1.0; 3]).into()),
            layer: layer_id,
            ..Default::default()
        });
    }
}

fn rotation_to_quat(rotation: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        rotation[0].to_radians(),
        rotation[1].to_radians(),
        rotation[2].to_radians(),
    )
}

fn init_clients(
    mut clients: Query<
        (
            &mut Client,
            &mut EntityLayerId,
            &mut VisibleChunkLayer,
            &mut VisibleEntityLayers,
            &mut Position,
            &mut Look,
            &mut HeadYaw,
            &mut GameMode,
            &mut Health,
        ),
        Added<Client>,
    >,
    layers: Query<Entity, With<ChunkLayer>>,
    config: Res<ServerConfig>,
) {
    for (
        mut client,
        mut layer_id,
        mut visible_chunk_layer,
        mut visible_entity_layers,
        mut pos,
        mut look,
        mut head_yaw,
        mut game_mode,
        mut health,
    ) in &mut clients
    {
        let layer = layers.single();

        layer_id.0 = layer;
        visible_chunk_layer.0 = layer;
        visible_entity_layers.0.insert(layer);
        pos.set(config.spawn_position);
        head_yaw.0 = config.spawn_rotation[0];
        look.yaw = config.spawn_rotation[0];
        look.pitch = config.spawn_rotation[1];
        *game_mode = config.game_mode.0;
        health.0 = 20.0;

        if let Some(title_text) = &config.title_text {
            client.set_title(title_text.0.clone());
            if let Some(title_subtext) = &config.title_subtext {
                client.set_subtitle(title_subtext.0.clone());
            }
            if config.title_animation_enabled {
                client.set_title_times(
                    config.title_fade_in.unwrap_or(0),
                    config.title_stay.unwrap_or(0),
                    config.title_fade_out.unwrap_or(0),
                );
            }
        }
    }
}

fn reset_oob_players(
    mut clients: Query<(&mut Position, &mut Look, &mut HeadYaw, &mut Health), Changed<Position>>,
    config: Res<ServerConfig>,
) {
    for (mut pos, mut look, mut head_yaw, mut health) in &mut clients {
        if pos.0.y < config.minimum_y_level as f64 {
            pos.set(config.spawn_position);
            head_yaw.0 = config.spawn_rotation[0];
            look.yaw = config.spawn_rotation[0];
            look.pitch = config.spawn_rotation[1];
            health.0 = 20.0;
        }
    }
}

fn broadcast_chat_message(
    usernames: Query<&Username>,
    mut clients: Query<&mut Client>,
    mut events: EventReader<ChatMessageEvent>,
) {
    for event in events.read() {
        let Ok(username) = usernames.get(event.client) else {
            continue;
        };
        for mut client in clients.iter_mut() {
            client.send_chat_message(format!("<{}> {}", username.as_str(), event.message));
        }
    }
}

fn parkour_prefix() -> Text {
    "[".bold() + "Parkour".color(Color::GREEN) + "] ".color(Color::WHITE).not_bold()
}

fn item_interactions(
    mut clients: Query<
        (
            Entity,
            &mut Client,
            &mut Inventory,
            &Position,
            &HeldItem,
            Option<&ParkourTracker>,
        ),
        With<Client>,
    >,
    mut packets: EventReader<PacketEvent>,
    mut commands: Commands,
    config: Res<ServerConfig>,
) {
    for packet in packets.read() {
        let is_block_interaction_packet = packet.decode::<PlayerInteractBlockC2s>().is_some();
        if (packet.decode::<PlayerInteractItemC2s>().is_some()
            || is_block_interaction_packet
            || packet.decode::<PlayerInteractEntityC2s>().is_some())
            && let Ok((entity, mut client, mut inv, pos, item, parkour_tracker)) =
                clients.get_mut(packet.client)
        {
            match inv.slot(item.slot()).item {
                ItemKind::Barrier => {
                    if let Some(tracker) = parkour_tracker
                        && pos.floor() == config.parkour[tracker.course_index].checkpoints[0].0
                    {
                        client.send_chat_message("You cannot cancel a parkour course while standing on the starting checkpoint.".color(Color::RED).not_bold());

                        if is_block_interaction_packet {
                            inv.changed |= 1 << 44;
                        }
                    } else {
                        client.send_chat_message(
                            parkour_prefix() + "Course cancelled".color(Color::WHITE).not_bold(),
                        );
                        commands.entity(entity).remove::<ParkourTracker>();
                        inv.set_slot(item.slot(), ItemStack::EMPTY);
                    }
                }
                _ => {}
            }
        }
    }
}

fn check_for_parkour_start(
    mut clients: Query<
        (Entity, &mut Client, &Position, &mut Inventory),
        (Changed<Position>, Without<ParkourTracker>),
    >,
    mut commands: Commands,
    config: Res<ServerConfig>,
) {
    for (entity, mut client, pos, mut inv) in clients.iter_mut() {
        if let Some((course_idx, course)) = config
            .parkour
            .iter()
            .enumerate()
            .filter(|(_, course)| course.checkpoints[0].0 == pos.0.floor())
            .next()
        {
            commands.entity(entity).insert(ParkourTracker {
                course_index: course_idx,
                checkpoint_index: 0,
                start_time: Instant::now(),
                actionbar_value: 0.0,
            });
            inv.set_slot(
                44,
                ItemStack::new(
                    ItemKind::Barrier,
                    1,
                    Some(compound! {
                        "display" => compound! {
                            "Name" => "{\"text\":\"Cancel Parkour\",\"italic\":false}"
                        },
                    }),
                ),
            );
            // TODO - Optimize this by preparing the text in advance instead of cloning it every time a player starts the course.
            client.send_chat_message(
                parkour_prefix()
                    + "Course started: ".color(Color::WHITE).not_bold()
                    + course.name.0.clone(),
            );
        }
    }
}

fn update_parkour_tracker(
    mut clients: Query<
        (
            Entity,
            &mut Client,
            &Position,
            &mut Inventory,
            &mut ParkourTracker,
        ),
        Changed<Position>,
    >,
    mut commands: Commands,
    config: Res<ServerConfig>,
) {
    for (entity, mut client, pos, mut inv, mut tracker) in clients.iter_mut() {
        let course = &config.parkour[tracker.course_index];
        let next_checkpoint = course.checkpoints[tracker.checkpoint_index + 1].0;

        if pos.0.floor() == next_checkpoint {
            tracker.checkpoint_index += 1;

            if tracker.checkpoint_index == course.checkpoints.len() - 1 {
                client.send_chat_message(
                    "Course completed: ".color(Color::WHITE).not_bold() + course.name.0.clone(),
                );
                commands.entity(entity).remove::<ParkourTracker>();
                inv.set_slot(44, ItemStack::EMPTY);
            } else {
                client.send_chat_message(
                    parkour_prefix()
                        + "Checkpoint reached: ".color(Color::WHITE).not_bold()
                        + Text::from(format!(
                            "{}/{}",
                            tracker.checkpoint_index,
                            course.checkpoints.len() - 1
                        )),
                );
            }
        }
    }
}

fn update_parkour_actionbar_status(
    mut clients: Query<(&mut Client, &mut ParkourTracker)>,
    config: Res<ServerConfig>,
) {
    for (mut client, mut tracker) in clients.iter_mut() {
        let elapsed = (tracker.start_time.elapsed().as_secs_f32() * 10.0).round() / 10.0;
        if elapsed != tracker.actionbar_value {
            tracker.actionbar_value = elapsed;
            client.set_action_bar(format!(
                "Parkour - {:.1}s ({}/{})",
                elapsed,
                tracker.checkpoint_index,
                config.parkour[tracker.course_index].checkpoints.len() - 1
            ));
        }
    }
}
