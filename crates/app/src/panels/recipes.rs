//! Recipes: the recipe files in the recipes folder, listed by category.
//!
//! The panel only collects what the user asked for (an [`Action`]) while it
//! draws; the actions are carried out afterwards in [`perform`]. That keeps
//! the drawing code free of side effects and easy to rearrange.
//!
//! Adjusting a recipe's settings and description happens in the Editor panel.

use super::hint;
use crate::paths;
use crate::state::AppState;
use eframe::egui::{self, RichText, Ui};
use xrp_recipes::library::{Library, RecipeEntry, FAVORITES};
use xrp_recipes::{text_import, Recipe, ALL_RECIPES, UNCATEGORIZED};

enum Action {
    /// Apply a recipe (file name) to the photo, or `None` for as shot.
    Use(Option<String>),
    ShowCategory(String),
    AddCategory(String),
    RemoveCategory(String),
    SetMembership { category: String, file_name: String, member: bool },
    MoveToTrash(String),
    Duplicate(String),
    /// Save a copy of a recipe file somewhere else.
    ExportCopy(String),
    /// Give a recipe's sample its own photo (the selected one), or go back
    /// to the shared photo.
    SamplePhoto { file: String, own: bool },
    Import,
    ImportFolder,
    /// Make recipes from the look stored in Fujifilm JPEGs or RAFs.
    FromPhotos,
    /// Make a recipe from the text in the "Recipe from text" window.
    FromText,
    /// Start a new recipe in the Editor.
    Create,
    Rescan,
    OpenFolder,
}

/// How wide the Recipes panel needs to be to show every recipe's name in
/// full: the longest name, plus the star and the scroll bar.
pub fn width_for_names(ui: &Ui, state: &AppState) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let longest = match &state.recipes.library {
        Ok(library) => library
            .recipes
            .iter()
            .map(|entry| ui.painter().layout_no_wrap(entry.display_name(), font.clone(), egui::Color32::WHITE).size().x)
            .fold(0.0, f32::max),
        Err(_) => 0.0,
    };
    (longest + 64.0).clamp(170.0, 320.0)
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let mut actions = Vec::new();

    if let Err(problem) = &state.recipes.library {
        ui.colored_label(ui.visuals().warn_fg_color, problem);
        if ui.button("Try again").clicked() {
            state.recipes.library = Library::open(&paths::recipes_folder());
        }
        return;
    }

    toolbar(ui, state, &mut actions);
    category_row(ui, state, &mut actions);
    ui.separator();
    recipe_list(ui, state, &mut actions);
    text_window(ui, state, &mut actions);

    for action in actions {
        perform(state, action);
    }
}

/// Import and Folder on one row, the search box under them. Kept to
/// short rows so the panel can stay narrow.
fn toolbar(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let library = state.recipes.library.as_ref().unwrap();
    ui.horizontal(|ui| {
        if ui
            .button("Import…")
            .on_hover_text("Copy .FP1 files into the recipes folder. Select as many as you like. You can also drop them onto the window.")
            .clicked()
        {
            actions.push(Action::Import);
        }
        ui.menu_button("Folder", |ui| {
            if ui.button("Import a whole folder…").on_hover_text("Every .FP1 in a folder and its subfolders.").clicked() {
                actions.push(Action::ImportFolder);
                ui.close();
            }
            if ui.button("Recipe from pasted text…").clicked() {
                state.recipes.paste_open = true;
                ui.close();
            }
            if ui
                .button("Recipes from photos…")
                .on_hover_text("Reads the look stored in Fujifilm JPEGs or RAFs and makes a recipe from each.")
                .clicked()
            {
                actions.push(Action::FromPhotos);
                ui.close();
            }
            if ui.button("New empty recipe").on_hover_text("Set it up in the Editor, name it, and press Save as new.").clicked() {
                actions.push(Action::Create);
                ui.close();
            }
            ui.separator();
            if ui.button("Open recipes folder").on_hover_text(library.folder.display().to_string()).clicked() {
                actions.push(Action::OpenFolder);
                ui.close();
            }
            if ui.button("Read folder again").clicked() {
                actions.push(Action::Rescan);
                ui.close();
            }
        });
    });
}

/// The category chooser, and a menu for adding and deleting categories.
fn category_row(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    category_editor(ui, state, actions);
    let library = state.recipes.library.as_ref().unwrap();
    let with_count = |name: &str| format!("{name} ({})", library.recipes_in(name).len());
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("recipe_category")
            .selected_text(with_count(&state.recipes.category))
            .width(ui.available_width() - 8.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                let names = [ALL_RECIPES, UNCATEGORIZED].into_iter().chain(library.categories.iter().map(|c| c.name.as_str()));
                for name in names {
                    let selected = state.recipes.category.eq_ignore_ascii_case(name);
                    if ui.selectable_label(selected, with_count(name)).clicked() {
                        actions.push(Action::ShowCategory(name.to_string()));
                    }
                }
            });

    });
    ui.add(egui::TextEdit::singleline(&mut state.recipes.search).hint_text("Search").desired_width(f32::INFINITY));
}

/// The menu for adding and deleting categories.
fn category_editor(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let library = state.recipes.library.as_ref().unwrap();
    ui.horizontal(|ui| {
        ui.menu_button("Edit categories", |ui| {
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut state.recipes.new_category)
                        .hint_text("New category")
                        .desired_width(130.0),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let add = ui.add_enabled(!state.recipes.new_category.trim().is_empty(), egui::Button::new("Add"));
                if add.clicked() || enter {
                    actions.push(Action::AddCategory(state.recipes.new_category.clone()));
                }
            });
            ui.separator();
            hint(ui, "Deleting a category never deletes recipe files.");
            for category in &library.categories {
                ui.horizontal(|ui| {
                    let deletable = Library::can_remove(&category.name);
                    let delete = ui.add_enabled(deletable, egui::Button::new("Delete"));
                    if delete.on_disabled_hover_text("This category is built in.").clicked() {
                        actions.push(Action::RemoveCategory(category.name.clone()));
                    }
                    ui.label(&category.name);
                });
            }
        });
    });
}

fn recipe_list(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let library = state.recipes.library.as_ref().unwrap();
    let samples = &state.samples;
    let search = state.recipes.search.trim().to_lowercase();
    let listed: Vec<&RecipeEntry> = library
        .recipes_in(&state.recipes.category)
        .into_iter()
        .filter(|r| search.is_empty() || r.display_name().to_lowercase().contains(&search))
        .collect();

    egui::ScrollArea::vertical().id_salt("recipes_scroll").auto_shrink(false).show(ui, |ui| {
        let as_shot = state.recipes.active.is_none() && state.editor.settings.is_empty();
        if ui
            .selectable_label(as_shot, "Original (no recipe)")
            .on_hover_text("The photo as it was shot, with no recipe applied.")
            .clicked()
        {
            actions.push(Action::Use(None));
        }

        if library.recipes.is_empty() {
            ui.add_space(8.0);
            hint(ui, "No recipes yet. Use Import, or put .FP1 files in the recipes folder:");
            ui.label(library.folder.display().to_string());
        } else if listed.is_empty() {
            ui.add_space(8.0);
            hint(ui, "Nothing here. Right-click a recipe to put it in a category.");
        }

        let mut hovered: Option<(&RecipeEntry, egui::Rect)> = None;
        for entry in listed {
            let active = state.recipes.active.as_deref() == Some(entry.file_name.as_str());
            let favourite = library.is_in(FAVORITES, &entry.file_name);
            ui.horizontal(|ui| {
                let star = if favourite { "★" } else { "☆" };
                let star = ui
                    .add(egui::Button::new(star).frame(false))
                    .on_hover_text(if favourite { "Remove from Favorites" } else { "Add to Favorites" });
                if star.clicked() {
                    actions.push(Action::SetMembership {
                        category: FAVORITES.into(),
                        file_name: entry.file_name.clone(),
                        member: !favourite,
                    });
                }

                let mut label = RichText::new(entry.display_name());
                if entry.recipe.is_err() {
                    label = label.color(ui.visuals().warn_fg_color);
                }
                let row = ui.selectable_label(active, label);
                if row.clicked() && entry.recipe.is_ok() {
                    actions.push(Action::Use(Some(entry.file_name.clone())));
                }
                row.context_menu(|ui| recipe_menu(ui, state, library, entry, actions));
                if row.hovered() {
                    hovered = Some((entry, row.rect));
                }
            });
        }

        if let Some(text) = &state.recipes.message {
            ui.add_space(6.0);
            hint(ui, text);
        }

        // The card for the recipe under the mouse opens to the left of the
        // panel, level with the recipe, so it never covers the names.
        if let Some((entry, row)) = hovered {
            let panel_left = ui.max_rect().left();
            let screen = ui.ctx().content_rect();
            let width = CARD_WIDTH + 24.0;
            let x = (panel_left - width - 10.0).max(screen.left() + 4.0);
            let y = row.top().min(screen.bottom() - 470.0).max(screen.top() + 30.0);
            egui::Area::new(egui::Id::unique("recipe_card"))
                .order(egui::Order::Tooltip)
                .fixed_pos(egui::pos2(x, y))
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        hover_card(ui, library, entry, samples.texture(&entry.file_name));
                    });
                });
        }
    });
}

/// Width of the card shown for the recipe under the mouse.
const CARD_WIDTH: f32 = 320.0;

/// The small window for making a recipe from pasted text.
fn text_window(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    if !state.recipes.paste_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Recipe from text")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.add(
                egui::TextEdit::multiline(&mut state.recipes.paste_text)
                    .hint_text(
                        "Paste a Fujifilm recipe here, one setting per line. The first line can be its name.\n\n\
                         Film Simulation: Classic Chrome\nGrain Effect: Weak, Small\n\
                         White Balance: Daylight, +2 Red & -4 Blue\nHighlight: -1",
                    )
                    .desired_rows(9)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.label("Add to category");
                let library = state.recipes.library.as_ref().unwrap();
                let shown = if state.recipes.paste_category.is_empty() { "None" } else { &state.recipes.paste_category };
                let mut choice = state.recipes.paste_category.clone();
                egui::ComboBox::from_id_salt("paste_category").selected_text(shown.to_string()).show_ui(ui, |ui| {
                    ui.selectable_value(&mut choice, String::new(), "None");
                    for category in &library.categories {
                        ui.selectable_value(&mut choice, category.name.clone(), &category.name);
                    }
                });
                state.recipes.paste_category = choice;
                let ready = !state.recipes.paste_text.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new("Make recipe")).clicked() {
                    actions.push(Action::FromText);
                }
            });
            if let Some(text) = &state.recipes.message {
                hint(ui, text);
            }
        });
    if !open {
        state.recipes.paste_open = false;
    }
}

/// The dialog shown while the mouse is over a recipe.
pub(crate) fn hover_card(
    ui: &mut Ui,
    library: &Library,
    entry: &RecipeEntry,
    sample: Option<&egui::TextureHandle>,
) {
    ui.set_max_width(CARD_WIDTH);
    ui.strong(entry.display_name());
    if let Some(sample) = sample {
        ui.add(egui::Image::new(sample).max_width(CARD_WIDTH).max_height(240.0).corner_radius(3.0));
    }
    match &entry.recipe {
        Err(problem) => {
            ui.colored_label(ui.visuals().warn_fg_color, format!("This file is {problem}."));
        }
        Ok(recipe) => {
            if recipe.description.trim().is_empty() {
                hint(ui, "No description yet. Select the recipe and add one in the Editor.");
            } else {
                ui.label(&recipe.description);
            }
            ui.separator();
            let lines = entry.settings.summary();
            if lines.is_empty() {
                hint(ui, "This recipe changes no settings.");
            }
            egui::Grid::new(("hover", &entry.file_name)).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
                for (label, value) in lines {
                    ui.label(RichText::new(label).weak());
                    ui.label(value);
                    ui.end_row();
                }
            });
            ui.separator();
            let categories: Vec<&str> = library
                .categories
                .iter()
                .filter(|c| library.is_in(&c.name, &entry.file_name))
                .map(|c| c.name.as_str())
                .collect();
            if !categories.is_empty() {
                hint(ui, &format!("In: {}", categories.join(", ")));
            }
            if !recipe.device.is_empty() {
                hint(ui, &format!("Made for {}", recipe.device));
            }
        }
    }
    hint(ui, &entry.file_name);
}

fn recipe_menu(ui: &mut Ui, state: &AppState, library: &Library, entry: &RecipeEntry, actions: &mut Vec<Action>) {
    ui.menu_button("Categories", |ui| {
        for category in &library.categories {
            let mut member = library.is_in(&category.name, &entry.file_name);
            if ui.checkbox(&mut member, &category.name).changed() {
                actions.push(Action::SetMembership {
                    category: category.name.clone(),
                    file_name: entry.file_name.clone(),
                    member,
                });
            }
        }
    });
    if ui.button("Duplicate").clicked() {
        actions.push(Action::Duplicate(entry.file_name.clone()));
        ui.close();
    }
    if ui.button("Export a copy…").on_hover_text("Save a copy of this recipe file in another folder.").clicked() {
        actions.push(Action::ExportCopy(entry.file_name.clone()));
        ui.close();
    }
    if state.samples.enabled {
        ui.separator();
        ui.label(RichText::new("Sample picture").weak());
        let photo = state.selected.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        let label = match &photo {
            Some(name) => format!("Use the selected photo ({name})"),
            None => "Use the selected photo".to_string(),
        };
        if ui
            .add_enabled(photo.is_some(), egui::Button::new(label))
            .on_hover_text("This recipe's sample is made from this photo instead of the shared one.")
            .on_disabled_hover_text("Select a photo first.")
            .clicked()
        {
            actions.push(Action::SamplePhoto { file: entry.file_name.clone(), own: true });
            ui.close();
        }
        let has_own = state.samples.own_photos.contains_key(&entry.file_name);
        if ui.add_enabled(has_own, egui::Button::new("Use the shared photo")).clicked() {
            actions.push(Action::SamplePhoto { file: entry.file_name.clone(), own: false });
            ui.close();
        }
    }
    ui.separator();
    if ui.button("Move file to trash").clicked() {
        actions.push(Action::MoveToTrash(entry.file_name.clone()));
        ui.close();
    }
}

fn perform(state: &mut AppState, action: Action) {
    // Actions that do not touch the library itself come first.
    let action = match action {
        Action::SamplePhoto { file, own } => {
            match (own, state.selected.clone()) {
                (true, Some(photo)) => {
                    state.samples.own_photos.insert(file, photo);
                }
                (true, None) => {}
                (false, _) => {
                    state.samples.own_photos.remove(&file);
                }
            }
            return;
        }
        Action::ImportFolder => return state.pick_recipe_folder_to_import(),
        Action::FromPhotos => return state.pick_photos_for_recipes(),
        Action::ExportCopy(file) => {
            // The export asks for a folder, then copies what is marked.
            state.recipes.selected = std::iter::once(file).collect();
            return state.pick_recipe_export_folder();
        }
        Action::Create => {
            state.use_recipe(None);
            state.editor.name.clear();
            let text = "New recipe: set it up in the Editor, name it, and press Save as new.".to_string();
            state.recipes.message = Some(text);
            return;
        }
        other => other,
    };
    let Ok(library) = &mut state.recipes.library else { return };
    let result: Result<Option<String>, String> = match action {
        Action::Duplicate(file) => library.duplicate(&file).map(|made| Some(format!("Made {made}."))),
        Action::FromText => {
            let text = state.recipes.paste_text.clone();
            let imported = text_import::parse(&text);
            if imported.understood == 0 {
                Err("No settings were recognised in that text.".to_string())
            } else {
                let first_line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                let name: String =
                    if first_line.contains(':') { "Pasted recipe".into() } else { first_line.chars().take(60).collect() };
                let file_text = Recipe::build(&name, "", "Imported from text.", &imported.settings.to_fields());
                let category = state.recipes.paste_category.clone();
                library
                    .create(&name, &file_text)
                    .and_then(|file_name| {
                        if category.is_empty() {
                            Ok(file_name)
                        } else {
                            library.set_membership(&category, &file_name, true).map(|()| file_name)
                        }
                    })
                    .map(|file_name| {
                        state.recipes.paste_text.clear();
                        state.recipes.paste_open = false;
                        let mut message = format!("Made {file_name} from {} settings.", imported.understood);
                        if !imported.skipped.is_empty() {
                            message.push_str(&format!(" Not understood: {}.", imported.skipped.join("; ")));
                        }
                        Some(message)
                    })
            }
        }
        Action::SamplePhoto { .. } | Action::ImportFolder | Action::FromPhotos | Action::ExportCopy(_) | Action::Create => return,
        Action::Use(recipe) => {
            state.use_recipe(recipe);
            return;
        }
        Action::ShowCategory(name) => {
            state.recipes.category = name;
            return;
        }
        Action::Import => {
            state.pick_recipes_to_import();
            return;
        }
        Action::OpenFolder => {
            paths::show_folder(&library.folder);
            return;
        }
        Action::Rescan => {
            library.rescan();
            let count = library.recipes.len();
            state.forget_recipe_previews();
            state.refresh_editor_from_library();
            Ok(Some(format!("Found {count} recipes.")))
        }
        Action::AddCategory(name) => library.add_category(&name).map(|()| {
            state.recipes.new_category.clear();
            Some(format!("Added category {}.", name.trim()))
        }),
        Action::RemoveCategory(name) => library.remove_category(&name).map(|()| {
            if state.recipes.category.eq_ignore_ascii_case(&name) {
                state.recipes.category = ALL_RECIPES.to_string();
            }
            Some(format!("Deleted category {name}. No recipe files were deleted."))
        }),
        Action::SetMembership { category, file_name, member } => {
            library.set_membership(&category, &file_name, member).map(|()| None)
        }
        Action::MoveToTrash(file_name) => match library.find(&file_name).map(|e| e.path.clone()) {
            Some(path) => trash::delete(&path)
                .map_err(|e| format!("Could not move {file_name} to the trash: {e}"))
                .and_then(|()| library.forget(&file_name))
                .map(|()| {
                    if state.recipes.active.as_deref() == Some(file_name.as_str()) {
                        state.recipes.active = None;
                    }
                    Some(format!("Moved {file_name} to the trash."))
                }),
            None => Err("That recipe is no longer in the folder.".into()),
        },
    };
    match result {
        Ok(Some(text)) => {
            state.log.push(text.clone());
            state.recipes.message = Some(text);
        }
        Ok(None) => {}
        Err(problem) => {
            state.log.push(problem.clone());
            state.recipes.message = Some(problem);
        }
    }
}
