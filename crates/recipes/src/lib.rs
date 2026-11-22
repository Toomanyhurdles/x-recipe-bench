//! Recipes for X Recipe Bench.
//!
//! - [`fp1`]: reading and writing `.FP1` recipe files (X RAW Studio's format).
//! - [`settings`]: what a recipe says, in typed form, and the words FP1 uses.
//! - [`apply`]: turning a recipe into changes to the camera's profile, and back.
//! - [`from_photo`]: the look a photo was shot with, read from the photo.
//! - [`library`]: the recipes folder and its `categories.txt`.
//! - [`starters`]: the recipes a new library starts with.
//! - [`text_import`]: reading a recipe written out as plain text.
//!
//! Nothing here talks to a camera or draws anything.

pub mod apply;
pub mod fp1;
pub mod from_photo;
pub mod library;
pub mod settings;
pub mod slot;
pub mod starters;
pub mod text_import;

pub use fp1::Recipe;
pub use library::{Library, RecipeEntry, ALL_RECIPES, UNCATEGORIZED};
pub use settings::Settings;
