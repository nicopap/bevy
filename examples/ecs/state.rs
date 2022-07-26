//! This example illustrates how to use [`States`] to control transitioning from a `Menu` state to
//! an `InGame` state.

use bevy::{prelude::*, ui_navigation::NavRequestSystem};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_state(AppState::Menu)
        .add_startup_system(setup)
        .add_system_set(SystemSet::on_enter(AppState::Menu).with_system(setup_menu))
        .add_system_set(
            SystemSet::on_update(AppState::Menu).with_system(hover_color.after(NavRequestSystem)),
        )
        .add_system_set(
            SystemSet::on_update(AppState::Menu)
                .with_system(activate_button.after(NavRequestSystem)),
        )
        .add_system_set(SystemSet::on_exit(AppState::Menu).with_system(cleanup_menu))
        .add_system_set(SystemSet::on_enter(AppState::InGame).with_system(setup_game))
        .add_system_set(
            SystemSet::on_update(AppState::InGame)
                .with_system(movement)
                .with_system(change_color),
        )
        .run();
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum AppState {
    Menu,
    InGame,
}

#[derive(Resource)]
struct MenuData {
    button_entity: Entity,
}

const NORMAL_BUTTON: Color = Color::rgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::rgb(0.25, 0.25, 0.25);

fn setup(mut commands: Commands) {
    commands.spawn_bundle(Camera2dBundle::default());
}

fn setup_menu(mut commands: Commands, asset_server: Res<AssetServer>) {
    let button_entity = commands
        .spawn_bundle(ButtonBundle {
            style: Style {
                size: Size::new(Val::Px(150.0), Val::Px(65.0)),
                // center button
                margin: UiRect::all(Val::Auto),
                // horizontally center child text
                justify_content: JustifyContent::Center,
                // vertically center child text
                align_items: AlignItems::Center,
                ..default()
            },
            color: NORMAL_BUTTON.into(),
            ..default()
        })
        .insert(Hover::default())
        .with_children(|parent| {
            parent.spawn_bundle(TextBundle::from_section(
                "Play",
                TextStyle {
                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                    font_size: 40.0,
                    color: Color::rgb(0.9, 0.9, 0.9),
                },
            ));
        })
        .id();
    commands.insert_resource(MenuData { button_entity });
}

fn activate_button(
    mut events: EventReader<NavEvent>,
    mut state: ResMut<State<AppState>>,
    data: Res<MenuData>,
) {
    for event in events.iter() {
        if event.is_activated(data.button_entity) {
            state.set(AppState::InGame).unwrap();
        }
    }
}

fn hover_color(mut interaction_query: Query<(&Hover, &mut UiColor), Changed<Hover>>) {
    for (hover, mut color) in &mut interaction_query {
        let updated_color = match hover {
            Hover::Hovered => HOVERED_BUTTON,
            Hover::None => NORMAL_BUTTON,
        };
        *color = updated_color.into();
    }
}

fn cleanup_menu(mut commands: Commands, menu_data: Res<MenuData>) {
    commands.entity(menu_data.button_entity).despawn_recursive();
}

fn setup_game(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn_bundle(SpriteBundle {
        texture: asset_server.load("branding/icon.png"),
        ..default()
    });
}

const SPEED: f32 = 100.0;
fn movement(
    time: Res<Time>,
    input: Res<Input<KeyCode>>,
    mut query: Query<&mut Transform, With<Sprite>>,
) {
    for mut transform in &mut query {
        let mut direction = Vec3::ZERO;
        if input.pressed(KeyCode::Left) {
            direction.x -= 1.0;
        }
        if input.pressed(KeyCode::Right) {
            direction.x += 1.0;
        }
        if input.pressed(KeyCode::Up) {
            direction.y += 1.0;
        }
        if input.pressed(KeyCode::Down) {
            direction.y -= 1.0;
        }

        if direction != Vec3::ZERO {
            transform.translation += direction.normalize() * SPEED * time.delta_seconds();
        }
    }
}

fn change_color(time: Res<Time>, mut query: Query<&mut Sprite>) {
    for mut sprite in &mut query {
        sprite
            .color
            .set_b((time.seconds_since_startup() * 0.5).sin() as f32 + 2.0);
    }
}
