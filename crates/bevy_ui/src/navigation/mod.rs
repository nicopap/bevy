mod queries;
mod systems;

pub use queries::UiProjectionQuery;
pub use systems::{
    default_gamepad_input, default_keyboard_input, default_mouse_input, InputMapping,
};
