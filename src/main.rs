use serde::Deserialize;
use valence::entity::living::Health;
use valence::message::ChatMessageEvent;
use valence::prelude::*;
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
                )))
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
                )))
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

#[derive(Resource, Deserialize, Debug)]
struct ServerConfig {
    spawn_position: [f64; 3],
    spawn_rotation: [f32; 2],
    game_mode: GameModeValue,
    chat_enabled: bool,
    boss_bar_text: Option<TextValue>,
    boss_bar_color: Option<BossBarColorValue>,
    boss_bar_division: Option<BossBarDivisionValue>,
    title_text: Option<TextValue>,
    title_subtext: Option<TextValue>,
    title_animation_enabled: bool,
    title_fade_in: Option<i32>,
    title_stay: Option<i32>,
    title_fade_out: Option<i32>,
}

fn main() {
    let config: ServerConfig = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(config_str) => {
            match toml::from_str(&config_str) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Failed to parse config file: {}", e);
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read config file: {}", e);
            return;
        }
    };

    let mut app = App::new();

    app
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, init_clients);

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

    // Force a 16x16 area of chunks around the origin to be loaded at all times.
    // This is similar to "spawn chunks" in vanilla. This isn't necessary for the
    // example to function, but it's done to demonstrate that it's possible.
    // for z in -8..8 {
    //     for x in -8..8 {
    //         let pos = ChunkPos::new(x, z);
    //
    //         level.ignored_chunks.insert(pos);
    //         level.force_chunk_load(pos);
    //     }
    // }

    let layer_id  = commands.spawn((layer, level)).id();

    if let Some(boss_bar_text) = &config.boss_bar_text {
        let mut boss_bar_bundle = BossBarBundle {
            title: BossBarTitle(boss_bar_text.0.clone()),
            health: BossBarHealth(1.0),
            layer: EntityLayerId(layer_id),
            ..Default::default()
        };

        if let Some(boss_bar_color) = &config.boss_bar_color {
            boss_bar_bundle.style.color = boss_bar_color.0;
        }

        if let Some(boss_bar_division) = &config.boss_bar_division {
            boss_bar_bundle.style.division =  boss_bar_division.0;
        }

        commands.spawn((
            boss_bar_bundle,
        ));
    }
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
                client.set_title_times(config.title_fade_in.unwrap_or(0), config.title_stay.unwrap_or(0), config.title_fade_out.unwrap_or(0));
            }
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
            client.send_chat_message(
                format!("<{}> {}", username.as_str(), event.message),
            );
        }
    }
}