#![allow(dead_code, unused_imports, unused_variables)]

use bevy::{
    app::AppExit,
    asset::AssetServerSettings,
    core_pipeline::ClearColor,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    ecs::{schedule::ReportExecutionOrderAmbiguities, system::EntityCommands},
    gltf::{Gltf, GltfMesh},
    prelude::*,
    render::{
        camera::PerspectiveProjection,
        mesh::Indices,
        render_resource::{Extent3d, PrimitiveTopology, Texture, TextureDimension, TextureFormat},
    },
    sprite::collide_aabb::{collide, Collision},
    ui::widget::ImageMode,
};
use bevy_kira_audio::{Audio, AudioChannel, AudioPlugin};
use bevy_tweening::TweeningPlugin;
//use bevy_prototype_debug_lines::{DebugLines, DebugLinesPlugin};
use chrono::prelude::*;
use serde::Deserialize;
use std::{collections::HashMap, f32::consts::*, fs::File, io::Read};

#[cfg(debug_assertions)]
use bevy_inspector_egui::{WorldInspectorParams, WorldInspectorPlugin};

mod text_asset;

use text_asset::{TextAsset, TextAssetPlugin};

#[derive(Deserialize)]
enum TextAlign {
    Start,
    Center,
    End,
}

#[derive(Deserialize, Clone)]
enum ButtonAction {
    NextPage,
    JumpToPage(String),
    JumpToEnd,
}

#[derive(Deserialize)]
struct Line {
    text: String,
    align: Option<TextAlign>,
    color: Option<Color>,
    size: Option<f32>,
}

#[derive(Deserialize)]
struct Button {
    text: String,
    action: ButtonAction,
}

#[derive(Deserialize)]
struct Page {
    /// Page name, for cross-reference (e.g. [`ButtonAction::JumpToPage`]).
    name: Option<String>,
    /// Is the page the final message before the scoreboard?
    #[serde(default)]
    is_final: bool,
    /// Lines of text to display.
    lines: Vec<Line>,
    /// Buttons to show on page and their action.
    buttons: Option<HashMap<String, Button>>,
    /// Page background color.
    background_color: Option<Color>,
    /// Align of page content.
    align: Option<JustifyContent>,
}

#[derive(Deserialize)]
struct Book {
    pages: Vec<Page>,
    #[serde(default)]
    line_spacing: f32,
    default_buttons: HashMap<String, Button>,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            pages: vec![],
            line_spacing: 30.0,
            default_buttons: HashMap::default(),
        }
    }
}

#[derive(Component, Default)]
struct Background;

#[derive(Copy, Clone, Debug)]
struct Score {
    date: DateTime<Utc>,
    page_read: u32,
}

#[derive(Component)]
struct TextSystem {
    book: Option<Book>,
    content_handle: Handle<TextAsset>,
    font: Handle<Font>,
    default_color: Color,
    default_size: f32,
    default_background_color: Color,
    root_node: Option<Entity>,
    page_index: usize,
    buttons: HashMap<String, Handle<Image>>,
    page_read: u32,
    scores: Vec<Score>,
    is_scoreboard: bool,
}

impl Default for TextSystem {
    fn default() -> Self {
        TextSystem {
            book: None,
            content_handle: Default::default(),
            font: Default::default(),
            default_color: Color::rgb(0.8, 0.8, 0.8),
            default_size: 30.,
            default_background_color: Color::rgb(0.1, 0.1, 0.2),
            root_node: None,
            page_index: 0,
            buttons: HashMap::default(),
            page_read: 0,
            scores: vec![],
            is_scoreboard: false,
        }
    }
}

impl TextSystem {
    /// Initialize a new instance.
    fn new(
        content_handle: Handle<TextAsset>,
        font: Handle<Font>,
        buttons: HashMap<String, Handle<Image>>,
    ) -> Self {
        TextSystem {
            font,
            content_handle,
            buttons,
            ..Default::default()
        }
    }

    /// Handle frame updates
    fn update(
        &mut self,
        commands: &mut Commands,
        text_assets: &Assets<TextAsset>,
        keyboard_input: &mut Input<KeyCode>,
    ) {
        // Setup once the text asset loaded
        if self.book.is_none() {
            if let Some(json) = text_assets.get(self.content_handle.clone()) {
                self.clear(commands);
                let book: Book = serde_json::from_str(&json.value).unwrap();
                let has_page = !book.pages.is_empty();
                self.book = Some(book);
                self.page_index = 0;
                if has_page {
                    self.setup_page(commands);
                }
            }
        };

        // Handle inputs
        if self.is_scoreboard {
            if keyboard_input.just_pressed(KeyCode::Space) {
                trace!("space");
                self.clear(commands);
                self.page_index = 0;
                self.is_scoreboard = false;
                self.page_read = 0;
                self.setup_page(commands);
            }
        } else if let Some(page) = self.current_page() {
            let buttons = if let Some(buttons) = &page.buttons {
                buttons
            } else {
                &self.book.as_ref().unwrap().default_buttons
            };

            let mut action = None;
            for (name, button) in buttons {
                if name == "space" && keyboard_input.just_pressed(KeyCode::Space) {
                    trace!("space");
                    action = Some(button.action.clone());
                } else if name == "y" && keyboard_input.just_pressed(KeyCode::Y) {
                    trace!("y");
                    action = Some(button.action.clone());
                } else if name == "n" && keyboard_input.just_pressed(KeyCode::N) {
                    trace!("n");
                    action = Some(button.action.clone());
                } else if name == "m" && keyboard_input.just_pressed(KeyCode::M) {
                    trace!("m");
                    action = Some(button.action.clone());
                } else if name == "1" && keyboard_input.just_pressed(KeyCode::Key1) {
                    trace!("1");
                    action = Some(button.action.clone());
                } else if name == "2" && keyboard_input.just_pressed(KeyCode::Key2) {
                    trace!("2");
                    action = Some(button.action.clone());
                } else if name == "3" && keyboard_input.just_pressed(KeyCode::Key3) {
                    trace!("3");
                    action = Some(button.action.clone());
                }
            }

            if let Some(mut action) = action {
                if page.is_final {
                    action = ButtonAction::JumpToEnd;
                }

                self.page_read += 1;

                match action {
                    ButtonAction::NextPage => self.move_next(commands),
                    ButtonAction::JumpToPage(page_name) => self.jump_to(commands, &page_name),
                    ButtonAction::JumpToEnd => self.spawn_leaderboard(commands),
                }
            }
        }
    }

    /// Get the current page, if any.
    fn current_page(&self) -> Option<&Page> {
        if let Some(book) = &self.book {
            if self.page_index < book.pages.len() {
                return Some(&book.pages[self.page_index]);
            }
        }
        None
    }

    /// Move to next page.
    fn move_next(&mut self, commands: &mut Commands) {
        self.clear(commands);
        self.page_index = self.page_index + 1;
        self.setup_page(commands);
    }

    /// Move to next page.
    fn jump_to(&mut self, commands: &mut Commands, page_name: &str) {
        self.clear(commands);
        if let Some(page_index) = self.page_by_name(page_name) {
            self.page_index = page_index;
            self.setup_page(commands);
        }
    }

    /// Get the index of a page by page name.
    fn page_by_name(&self, name: &str) -> Option<usize> {
        if let Some(book) = &self.book {
            for i in 0..book.pages.len() {
                if let Some(page_name) = &book.pages[i].name {
                    if page_name == name {
                        return Some(i);
                    }
                }
            }
        }
        return None;
    }

    /// Clear all content.
    fn clear(&mut self, commands: &mut Commands) {
        if let Some(entity) = &self.root_node {
            commands.entity(*entity).despawn_recursive();
        }
        self.root_node = None;
    }

    /// Setup the current page.
    fn setup_page(&mut self, commands: &mut Commands) {
        self.clear(commands);

        let book = self.book.as_ref().unwrap();
        let page = &book.pages[self.page_index];

        let mut root = self.spawn_background(commands, page.background_color, page.align);

        let text_align = TextAlignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };

        root.with_children(|parent| {
            // Spawn all lines
            let margin = Val::Px(book.line_spacing);
            let margin = Rect {
                top: margin,
                bottom: margin,
                ..Default::default()
            };
            for (line_index, line) in page.lines.iter().enumerate() {
                parent
                    .spawn_bundle(NodeBundle {
                        style: Style {
                            margin,
                            ..Default::default()
                        },
                        color: UiColor(Color::NONE),
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        parent.spawn_bundle(TextBundle {
                            text: Text::with_section(
                                line.text.clone(),
                                TextStyle {
                                    font: self.font.clone(),
                                    font_size: line.size.unwrap_or(self.default_size),
                                    color: line.color.unwrap_or(self.default_color),
                                },
                                text_align,
                            ),
                            ..Default::default()
                        });
                    })
                    .insert(Name::new(format!("Line{}", line_index)));
            }

            // Spawn buttons
            let buttons = page.buttons.as_ref().unwrap_or(&book.default_buttons);
            for (color, button) in buttons {
                let image = if let Some(image) = self.buttons.get(color) {
                    image.clone()
                } else {
                    Handle::<Image>::default()
                };
                self.spawn_button(parent, book.line_spacing, &button.text, image);
            }
        });

        self.root_node = Some(root.id());
    }

    fn spawn_button(
        &self,
        parent: &mut ChildBuilder,
        line_spacing: f32,
        text: &str,
        image: Handle<Image>,
    ) {
        let margin = Val::Px(line_spacing);
        let margin = Rect {
            top: margin,
            bottom: margin,
            ..Default::default()
        };

        parent
            .spawn_bundle(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    margin,
                    size: Size {
                        width: Val::Auto,
                        height: Val::Px(64.),
                    },
                    ..Default::default()
                },
                color: UiColor(Color::NONE),
                ..Default::default()
            })
            .insert(Name::new(format!("button:{}", text)))
            .with_children(|parent| {
                parent
                    .spawn_bundle(NodeBundle {
                        style: Style {
                            // Align button image (child) to the right
                            justify_content: JustifyContent::FlexEnd,
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            size: Size {
                                width: Val::Px(350.),
                                height: Val::Px(64.),
                            },
                            ..Default::default()
                        },
                        color: UiColor(Color::NONE),
                        ..Default::default()
                    })
                    .insert(Name::new("image"))
                    .with_children(|parent| {
                        parent.spawn_bundle(ImageBundle {
                            image: UiImage(image),
                            image_mode: ImageMode::KeepAspect,
                            style: Style {
                                size: Size {
                                    width: Val::Auto,
                                    height: Val::Auto,
                                },
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                    });

                parent
                    .spawn_bundle(NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            margin: Rect {
                                left: Val::Px(20.),
                                ..Default::default()
                            },
                            size: Size {
                                width: Val::Px(300.),
                                height: Val::Px(64.),
                            },
                            ..Default::default()
                        },
                        color: UiColor(Color::NONE),
                        ..Default::default()
                    })
                    .insert(Name::new("text"))
                    .with_children(|parent| {
                        parent.spawn_bundle(TextBundle {
                            text: Text::with_section(
                                text,
                                TextStyle {
                                    font: self.font.clone(),
                                    font_size: self.default_size,
                                    color: self.default_color,
                                },
                                TextAlignment {
                                    horizontal: HorizontalAlign::Center,
                                    vertical: VerticalAlign::Center,
                                },
                            ),
                            ..Default::default()
                        });
                    });
            });
    }

    /// Spawn the leaderboard at the end of the game.
    fn spawn_leaderboard(&mut self, commands: &mut Commands) {
        self.clear(commands);

        // Insert new score, retaining only the 10 last ones.
        while self.scores.len() >= 10 {
            self.scores.remove(0);
        }
        self.scores.push(Score {
            page_read: self.page_read,
            date: Utc::now(),
        });

        // Sort score records by actual score value (pages read)
        let mut sorted_scores = self.scores.clone();
        sorted_scores.sort_by(|a, b| b.page_read.partial_cmp(&a.page_read).unwrap());

        self.is_scoreboard = true;

        let mut root = self.spawn_background(commands, None, Some(JustifyContent::FlexStart));

        let now: DateTime<Utc> = Utc::now();

        let text_align = TextAlignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };

        root.with_children(|parent| {
            // Title
            parent
                .spawn_bundle(TextBundle {
                    style: Style {
                        margin: Rect {
                            top: Val::Px(30.),
                            bottom: Val::Px(30.),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    text: Text::with_section(
                        "Score",
                        TextStyle {
                            font: self.font.clone(),
                            font_size: 60.,
                            color: self.default_color,
                        },
                        text_align,
                    ),
                    ..Default::default()
                })
                .insert(Name::new("Score"));

            // Score lines
            let margin = Val::Px(10.);
            let margin = Rect {
                top: margin,
                bottom: margin,
                ..Default::default()
            };
            for score in &sorted_scores {
                parent
                    .spawn_bundle(NodeBundle {
                        style: Style {
                            margin,
                            ..Default::default()
                        },
                        color: UiColor(Color::NONE),
                        ..Default::default()
                    })
                    .insert(Name::new(format!("{:?}", score.date)))
                    .with_children(|parent| {
                        parent
                            .spawn_bundle(NodeBundle {
                                style: Style {
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..Default::default()
                                },
                                color: UiColor(Color::NONE),
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                parent
                                    .spawn_bundle(NodeBundle {
                                        style: Style {
                                            justify_content: JustifyContent::FlexStart,
                                            size: Size {
                                                width: Val::Px(400.),
                                                height: Val::Px(30.),
                                            },
                                            ..Default::default()
                                        },
                                        color: UiColor(Color::NONE),
                                        ..Default::default()
                                    })
                                    .with_children(|parent| {
                                        parent.spawn_bundle(TextBundle {
                                            text: Text::with_section(
                                                score.date.format("%Y-%m-%d %H:%M:%S").to_string(),
                                                TextStyle {
                                                    font: self.font.clone(),
                                                    font_size: self.default_size,
                                                    color: self.default_color,
                                                },
                                                text_align,
                                            ),
                                            ..Default::default()
                                        });
                                    });

                                parent
                                    .spawn_bundle(NodeBundle {
                                        style: Style {
                                            justify_content: JustifyContent::FlexEnd,
                                            size: Size {
                                                width: Val::Px(200.),
                                                height: Val::Px(30.),
                                            },
                                            ..Default::default()
                                        },
                                        color: UiColor(Color::NONE),
                                        ..Default::default()
                                    })
                                    .with_children(|parent| {
                                        parent.spawn_bundle(TextBundle {
                                            text: Text::with_section(
                                                format!("{} pages read", score.page_read),
                                                TextStyle {
                                                    font: self.font.clone(),
                                                    font_size: self.default_size,
                                                    color: self.default_color,
                                                },
                                                text_align,
                                            ),
                                            ..Default::default()
                                        });
                                    });
                            });
                    });
            }

            // Restart button
            self.spawn_button(
                parent,
                30.,
                "Restart",
                self.buttons.get("space").unwrap().clone(),
            );
        });

        self.root_node = Some(root.id());
    }

    /// Spawn a background node of the given color covering the entire screen, and set up to
    /// have children laid out in column from top to bottom, horizontally stretching the
    /// entire screen.
    fn spawn_background<'w, 's, 'a>(
        &'a self,
        commands: &'a mut Commands<'w, 's>,
        color: Option<Color>,
        justify_content: Option<JustifyContent>,
    ) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = commands.spawn_bundle(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                // Cover entire screen
                position: Rect::all(Val::Px(0.0)),
                // Lay out content items from top to bottom (reverse because Bevy)
                flex_direction: FlexDirection::ColumnReverse,
                // Align the entire content group vertically to the top
                justify_content: justify_content.unwrap_or(JustifyContent::FlexStart),
                // Center child items horizontally
                align_items: AlignItems::Center,
                ..Default::default()
            },
            color: UiColor(color.unwrap_or(self.default_background_color)),
            ..Default::default()
        });
        entity_commands
            .insert(Name::new("Background"))
            .insert(Background);
        entity_commands
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn_bundle(UiCameraBundle::default());

    let text_align = TextAlignment {
        horizontal: HorizontalAlign::Center,
        vertical: VerticalAlign::Center,
    };

    let content = asset_server.load("text.json");
    let font = asset_server.load("fonts/mochiy_pop_one/MochiyPopOne-Regular.ttf");
    let mut buttons: HashMap<String, Handle<Image>> = HashMap::new();
    buttons.insert("space".to_string(), asset_server.load("key_space.png"));
    buttons.insert("m".to_string(), asset_server.load("key_m.png"));
    buttons.insert("n".to_string(), asset_server.load("key_n.png"));
    buttons.insert("y".to_string(), asset_server.load("key_y.png"));
    buttons.insert("1".to_string(), asset_server.load("key_1.png"));
    buttons.insert("2".to_string(), asset_server.load("key_2.png"));
    buttons.insert("3".to_string(), asset_server.load("key_3.png"));
    commands
        .spawn()
        .insert(Name::new("TextSystem"))
        .insert(TextSystem::new(content, font, buttons));
}

fn update(
    mut commands: Commands,
    text_assets: Res<Assets<TextAsset>>,
    mut query: Query<&mut TextSystem>,
    mut keyboard_input: ResMut<Input<KeyCode>>,
) {
    let mut text_system = query.single_mut();
    text_system.update(&mut commands, &text_assets, &mut keyboard_input);
}

fn main() {
    let mut diag = LogDiagnosticsPlugin::default();
    diag.debug = true;

    let mut app = App::new();

    app
        // Logging and diagnostics
        .insert_resource(bevy::log::LogSettings {
            level: bevy::log::Level::INFO,
            filter: "wgpu=error,bevy_render=info,ld50=trace".to_string(),
        })
        .add_plugin(diag)
        // Main window
        .insert_resource(WindowDescriptor {
            title: "LD50".to_string(),
            vsync: true,
            ..Default::default()
        });

    app
        // Helper to exit with ESC key
        .add_system(bevy::input::system::exit_on_esc_system)
        // Default plugins
        .add_plugins(DefaultPlugins)
        // Audio (Kira)
        .add_plugin(AudioPlugin);

    // In Debug build only, add egui inspector to help
    #[cfg(debug_assertions)]
    app.add_plugin(WorldInspectorPlugin::new());

    app.add_plugin(TextAssetPlugin)
        .add_startup_system(setup)
        .add_system(update);

    app.run();
}
