//! The recipes folder.
//!
//! ```text
//! Recipes/
//!   categories.txt     category titles and which recipes are in each
//!   Kodak Tri-X.FP1    one file per recipe
//!   ...
//! ```
//!
//! `categories.txt` is plain text and can be edited by hand:
//!
//! ```text
//! [Favorites]
//! Kodak Tri-X.FP1
//!
//! [Warm]
//! Kodak Tri-X.FP1
//! Kodak Portra 400.FP1
//! ```
//!
//! "Uncategorized" is not written to the file: it lists the recipes that are
//! in no category. "All recipes" lists every recipe.

use crate::fp1::Recipe;
use crate::settings::Settings;
use std::path::{Path, PathBuf};

pub const UNCATEGORIZED: &str = "Uncategorized";
pub const ALL_RECIPES: &str = "All recipes";
pub const FAVORITES: &str = "Favorites";
pub const CATEGORIES_FILE: &str = "categories.txt";

pub const DEFAULT_CATEGORIES: &[&str] = &[
    FAVORITES,
    "Nostalgia",
    "Warm",
    "Cool",
    "Monochrome",
    "Portrait",
    "Landscape",
    "Street",
    "Night",
    "Vivid",
    "Muted",
    "Film Emulation",
];

/// Line in categories.txt recording that the starter recipes were offered,
/// so deleting them does not bring them back.
const STARTERS_MARK: &str = "# starter-recipes: installed";

const HEADER: &str = "\
# X Recipe Bench recipe categories.
# A line in [brackets] is a category title. The lines under it are the recipe
# files in that category. A recipe may be listed under several categories.
# \"Uncategorized\" is built in: it shows the recipes that are in no category.
";

#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub name: String,
    /// Recipe file names, e.g. "Kodak Tri-X.FP1".
    pub members: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecipeEntry {
    /// File name within the recipes folder; also how categories refer to it.
    pub file_name: String,
    pub path: PathBuf,
    /// The parsed recipe, or why the file could not be read.
    pub recipe: Result<Recipe, String>,
    pub settings: Settings,
}

impl RecipeEntry {
    /// The recipe's name, falling back to the file name without its extension.
    pub fn display_name(&self) -> String {
        match &self.recipe {
            Ok(r) if !r.name.trim().is_empty() => r.name.clone(),
            _ => Path::new(&self.file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| self.file_name.clone()),
        }
    }
}

pub struct Library {
    pub folder: PathBuf,
    pub recipes: Vec<RecipeEntry>,
    pub categories: Vec<Category>,
    starters_installed: bool,
    fingerprint: u64,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl Library {
    /// Opens the folder, creating it and a starter `categories.txt` if needed.
    pub fn open(folder: &Path) -> Result<Library, String> {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
        let mut library = Library {
            folder: folder.to_path_buf(),
            recipes: Vec::new(),
            categories: Vec::new(),
            starters_installed: false,
            fingerprint: 0,
        };
        if !folder.join(CATEGORIES_FILE).exists() {
            library.categories = DEFAULT_CATEGORIES
                .iter()
                .map(|name| Category { name: name.to_string(), members: Vec::new() })
                .collect();
            library.save_categories()?;
        }
        library.rescan();
        // A library that has never had the starter recipes and holds no
        // recipes of its own gets them once.
        if !library.starters_installed && library.recipes.is_empty() {
            library.install_starters()?;
        }
        Ok(library)
    }

    /// Writes the starter recipes into the folder and files them under their
    /// categories. Existing files with the same names are left alone.
    /// Returns how many files were written.
    pub fn install_starters(&mut self) -> Result<usize, String> {
        let mut written = 0;
        for starter in crate::starters::all() {
            let file_name = starter.file_name();
            let path = self.folder.join(&file_name);
            if !path.exists() {
                write_atomically(&path, &starter.file_text())?;
                written += 1;
            }
            for category in starter.categories {
                if !self.categories.iter().any(|c| same(&c.name, category)) {
                    self.categories.push(Category { name: category.to_string(), members: Vec::new() });
                }
                let cat = self.categories.iter_mut().find(|c| same(&c.name, category)).unwrap();
                if !cat.members.iter().any(|m| m == &file_name) {
                    cat.members.push(file_name.clone());
                }
            }
        }
        self.starters_installed = true;
        self.save_categories()?;
        self.rescan();
        Ok(written)
    }

    /// Makes a copy of a recipe, in the same categories. Returns its file name.
    pub fn duplicate(&mut self, file_name: &str) -> Result<String, String> {
        let entry = self.find(file_name).ok_or("That recipe is no longer in the folder.")?;
        let recipe = entry.recipe.as_ref().map_err(|e| format!("This file {e}."))?;
        let name = format!("{} copy", entry.display_name());
        let text = recipe.renamed(&name);
        let categories: Vec<String> = self.categories_of(file_name).iter().map(|c| c.to_string()).collect();
        let new_file = self.write_new(&name, &text)?;
        for category in categories {
            if let Some(cat) = self.categories.iter_mut().find(|c| same(&c.name, &category)) {
                cat.members.push(new_file.clone());
            }
        }
        self.save_categories()?;
        self.rescan();
        Ok(new_file)
    }

    /// Copies recipe files out of the library into `dest`, for sharing or
    /// backup. Existing files there are not overwritten. Returns how many were copied.
    pub fn export_copies(&self, file_names: &[String], dest: &Path) -> Result<usize, String> {
        std::fs::create_dir_all(dest).map_err(|e| format!("Could not use {}: {e}", dest.display()))?;
        let mut copied = 0;
        for file_name in file_names {
            let Some(entry) = self.find(file_name) else { continue };
            let stem = Path::new(file_name).file_stem().unwrap_or_default().to_string_lossy().to_string();
            let mut target = dest.join(file_name);
            let mut n = 2;
            while target.exists() {
                target = dest.join(format!("{stem} ({n}).FP1"));
                n += 1;
            }
            std::fs::copy(&entry.path, &target).map_err(|e| format!("Could not copy {file_name}: {e}"))?;
            copied += 1;
        }
        Ok(copied)
    }

    /// Copies recipe files into the library. A folder in the list stands for
    /// every recipe file inside it, at any depth. A file whose name is taken
    /// is kept under a numbered name; the originals are never touched.
    /// Returns (file names added, problems).
    pub fn import(&mut self, sources: &[PathBuf]) -> (Vec<String>, Vec<String>) {
        let (mut added, mut problems) = (Vec::new(), Vec::new());
        let mut files = Vec::new();
        for source in sources {
            if source.is_dir() {
                collect_recipe_files(source, 0, &mut files);
            } else {
                files.push(source.clone());
            }
        }
        for source in &files {
            let shown = source.file_name().unwrap_or_default().to_string_lossy().to_string();
            if source.parent() == Some(self.folder.as_path()) {
                continue; // already in the library
            }
            let text = match std::fs::read(source) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(e) => {
                    problems.push(format!("{shown}: could not be read ({e})"));
                    continue;
                }
            };
            if let Err(e) = Recipe::parse(&text) {
                problems.push(format!("{shown}: {e}"));
                continue;
            }
            let stem = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
            match self.write_new(&stem, &text) {
                Ok(file_name) => added.push(file_name),
                Err(e) => problems.push(format!("{shown}: {e}")),
            }
        }
        self.rescan();
        (added, problems)
    }

    /// Saves changed settings into an existing recipe's file.
    pub fn save_settings(&mut self, file_name: &str, settings: &Settings) -> Result<(), String> {
        let entry = self.find(file_name).ok_or("That recipe is no longer in the folder.")?;
        let recipe = entry.recipe.as_ref().map_err(|e| format!("This file {e}."))?;
        let text = recipe.with_fields(&settings.to_fields());
        Recipe::parse(&text).map_err(|e| format!("Could not save the recipe: {e}"))?;
        write_atomically(&entry.path, &text)?;
        self.rescan();
        Ok(())
    }

    /// Writes `text` as a new file named after `name`, numbered if taken.
    fn write_new(&mut self, name: &str, text: &str) -> Result<String, String> {
        write_recipe_file(&self.folder, name, text)
    }
}

/// Writes a new recipe file into `folder` under a name made from `name`,
/// never replacing an existing file. Returns the file name. Usable without
/// an open [`Library`] (the camera thread backs slots up with it).
pub fn write_recipe_file(folder: &Path, name: &str, text: &str) -> Result<String, String> {
    {
        let safe: String = name
            .trim()
            .chars()
            .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
            .collect();
        let safe = safe.trim_matches(['.', ' ']).to_string();
        if safe.is_empty() {
            return Err("Type a name for the recipe.".into());
        }
        let mut file_name = format!("{safe}.FP1");
        let mut n = 2;
        while folder.join(&file_name).exists() {
            file_name = format!("{safe} ({n}).FP1");
            n += 1;
        }
        write_atomically(&folder.join(&file_name), text)?;
        Ok(file_name)
    }
}

impl Library {
    /// Puts a recipe in a category, creating the category if needed.
    pub fn file_under(&mut self, category: &str, file_name: &str) -> Result<(), String> {
        if !self.categories.iter().any(|c| same(&c.name, category)) {
            self.add_category(category)?;
        }
        self.set_membership(category, file_name, true)
    }

    /// Re-reads every recipe file and `categories.txt`.
    pub fn rescan(&mut self) {
        let mut recipes: Vec<RecipeEntry> = list_recipe_files(&self.folder)
            .into_iter()
            .map(|path| {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let recipe = std::fs::read(&path)
                    .map_err(|e| format!("could not be read ({e})"))
                    .and_then(|bytes| Recipe::parse(&String::from_utf8_lossy(&bytes)));
                let settings = recipe.as_ref().map(Settings::from_recipe).unwrap_or_default();
                RecipeEntry { file_name, path, recipe, settings }
            })
            .collect();
        recipes.sort_by_key(|r| r.display_name().to_lowercase());
        self.recipes = recipes;

        let text = std::fs::read_to_string(self.folder.join(CATEGORIES_FILE)).unwrap_or_default();
        self.categories = parse_categories(&text);
        self.starters_installed = text.lines().any(|l| l.trim() == STARTERS_MARK);
        self.fingerprint = fingerprint(&self.folder);
    }

    /// True if files in the folder changed since the last scan (cheap to call).
    pub fn changed_on_disk(&self) -> bool {
        fingerprint(&self.folder) != self.fingerprint
    }

    pub fn find(&self, file_name: &str) -> Option<&RecipeEntry> {
        self.recipes.iter().find(|r| r.file_name == file_name)
    }

    /// Recipes in a category, in list order. "All recipes" gives every
    /// recipe; "Uncategorized" gives those that are in no category.
    pub fn recipes_in(&self, category: &str) -> Vec<&RecipeEntry> {
        if same(category, ALL_RECIPES) {
            return self.recipes.iter().collect();
        }
        if same(category, UNCATEGORIZED) {
            return self.recipes.iter().filter(|r| self.categories_of(&r.file_name).is_empty()).collect();
        }
        let Some(cat) = self.categories.iter().find(|c| same(&c.name, category)) else {
            return Vec::new();
        };
        self.recipes.iter().filter(|r| cat.members.iter().any(|m| m == &r.file_name)).collect()
    }

    /// Names of the categories a recipe is in.
    pub fn categories_of(&self, file_name: &str) -> Vec<&str> {
        self.categories
            .iter()
            .filter(|c| c.members.iter().any(|m| m == file_name))
            .map(|c| c.name.as_str())
            .collect()
    }

    pub fn is_in(&self, category: &str, file_name: &str) -> bool {
        self.categories
            .iter()
            .any(|c| same(&c.name, category) && c.members.iter().any(|m| m == file_name))
    }

    pub fn set_membership(&mut self, category: &str, file_name: &str, member: bool) -> Result<(), String> {
        let cat = self
            .categories
            .iter_mut()
            .find(|c| same(&c.name, category))
            .ok_or_else(|| format!("There is no category called {category}."))?;
        cat.members.retain(|m| m != file_name);
        if member {
            cat.members.push(file_name.to_string());
        }
        self.save_categories()
    }

    pub fn add_category(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Type a name for the category.".into());
        }
        if name.contains(['[', ']', '#', '\n', '\r']) {
            return Err("Category names cannot contain [ ] or #.".into());
        }
        if same(name, UNCATEGORIZED) || same(name, ALL_RECIPES) || self.categories.iter().any(|c| same(&c.name, name)) {
            return Err(format!("There is already a category called {name}."));
        }
        self.categories.push(Category { name: name.to_string(), members: Vec::new() });
        self.save_categories()
    }

    /// Removes a category. Recipe files are never touched.
    pub fn remove_category(&mut self, name: &str) -> Result<(), String> {
        if !Self::can_remove(name) {
            return Err(format!("{name} cannot be deleted."));
        }
        self.categories.retain(|c| !same(&c.name, name));
        self.save_categories()
    }

    pub fn can_remove(name: &str) -> bool {
        !same(name, UNCATEGORIZED) && !same(name, ALL_RECIPES) && !same(name, FAVORITES)
    }

    fn save_categories(&mut self) -> Result<(), String> {
        let mut text = write_categories(&self.categories);
        if self.starters_installed {
            text = text.replacen('\n', &format!("\n{STARTERS_MARK}\n"), 1);
        }
        write_atomically(&self.folder.join(CATEGORIES_FILE), &text)?;
        self.fingerprint = fingerprint(&self.folder);
        Ok(())
    }

    /// Replaces a recipe's description inside its file.
    pub fn set_description(&mut self, file_name: &str, description: &str) -> Result<(), String> {
        let entry = self.find(file_name).ok_or("That recipe is no longer in the folder.")?;
        let recipe = entry.recipe.as_ref().map_err(|e| format!("This file {e}."))?;
        let text = recipe.with_description(description);
        // Make sure the result still reads back before replacing the file.
        Recipe::parse(&text).map_err(|e| format!("Could not store the description: {e}"))?;
        write_atomically(&entry.path, &text)?;
        self.rescan();
        Ok(())
    }

    /// Writes a new recipe file. Returns its file name.
    pub fn create(&mut self, name: &str, text: &str) -> Result<String, String> {
        let file_name = self.write_new(name, text)?;
        self.rescan();
        Ok(file_name)
    }

    /// Forgets a recipe that was removed from the folder: drops it from every category.
    pub fn forget(&mut self, file_name: &str) -> Result<(), String> {
        for c in &mut self.categories {
            c.members.retain(|m| m != file_name);
        }
        self.save_categories()?;
        self.rescan();
        Ok(())
    }
}

/// Every recipe file under `folder`, at any depth (within reason).
fn collect_recipe_files(folder: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 12 {
        return;
    }
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(folder).map(|e| e.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_recipe_files(&path, depth + 1, out);
        } else if path.extension().map(|x| x.to_string_lossy().eq_ignore_ascii_case("fp1")).unwrap_or(false) {
            out.push(path);
        }
    }
}

fn list_recipe_files(folder: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension().map(|x| x.to_string_lossy().eq_ignore_ascii_case("fp1")).unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A number that changes when recipe files or categories.txt change.
fn fingerprint(folder: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut files = list_recipe_files(folder);
    files.push(folder.join(CATEGORIES_FILE));
    files.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for f in files {
        f.hash(&mut hasher);
        if let Ok(meta) = std::fs::metadata(&f) {
            meta.len().hash(&mut hasher);
            meta.modified().ok().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp-xrp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Could not write {}: {e}", path.display()))
}

pub fn parse_categories(text: &str) -> Vec<Category> {
    let mut categories: Vec<Category> = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim();
            // "Uncategorized" is built in; a duplicate title merges into the first.
            skipping = name.is_empty() || same(name, UNCATEGORIZED);
            if !skipping && !categories.iter().any(|c| same(&c.name, name)) {
                categories.push(Category { name: name.to_string(), members: Vec::new() });
            }
            if !skipping {
                let index = categories.iter().position(|c| same(&c.name, name)).unwrap();
                let cat = categories.remove(index);
                categories.push(cat);
            }
        } else if !skipping {
            if let Some(cat) = categories.last_mut() {
                if !cat.members.iter().any(|m| m == line) {
                    cat.members.push(line.to_string());
                }
            }
        }
    }
    categories
}

pub fn write_categories(categories: &[Category]) -> String {
    let mut out = String::from(HEADER);
    for c in categories {
        out.push_str(&format!("\n[{}]\n", c.name));
        for m in &c.members {
            out.push_str(m);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xrp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn fp1(label: &str, film: &str) -> String {
        Recipe::build(label, "X-T3", "", &[("FilmSimulation".into(), film.into())])
    }

    #[test]
    fn categories_text_round_trips() {
        let text = "# note\n[Favorites]\nA.FP1\n\n[Warm]\nA.FP1\nB.FP1\n[Uncategorized]\nignored.FP1\n[Empty]\n";
        let cats = parse_categories(text);
        assert_eq!(cats.len(), 3);
        assert_eq!(cats[1], Category { name: "Warm".into(), members: vec!["A.FP1".into(), "B.FP1".into()] });
        assert!(cats[2].members.is_empty());
        assert_eq!(parse_categories(&write_categories(&cats)), cats);
    }

    #[test]
    fn a_new_folder_gets_starter_categories() {
        let dir = temp_folder("new");
        let lib = Library::open(&dir).unwrap();
        assert!(dir.join(CATEGORIES_FILE).exists());
        let names: Vec<&str> = lib.categories.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Favorites") && names.contains(&"Nostalgia") && names.contains(&"Monochrome"));
        // It starts with the starter recipes, at least two in every category.
        assert_eq!(lib.recipes.len(), crate::starters::all().len());
        for category in DEFAULT_CATEGORIES {
            assert!(lib.recipes_in(category).len() >= 2, "{category}");
        }
        assert!(lib.recipes.iter().all(|r| r.recipe.is_ok() && !r.recipe.as_ref().unwrap().description.is_empty()));
        // Deleting them all does not bring them back on the next start.
        for r in lib.recipes.clone() {
            std::fs::remove_file(&r.path).unwrap();
        }
        let lib = Library::open(&dir).unwrap();
        assert!(lib.recipes.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_files_and_handles_categories() {
        let dir = temp_folder("scan");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Tri-X.FP1"), fp1("Kodak Tri-X", "Acros")).unwrap();
        std::fs::write(dir.join("portra.fp1"), fp1("Kodak Portra 400", "Classic")).unwrap();
        std::fs::write(dir.join("broken.FP1"), "not xml").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let mut lib = Library::open(&dir).unwrap();
        assert_eq!(lib.recipes.len(), 3);
        assert!(lib.find("broken.FP1").unwrap().recipe.is_err());
        assert_eq!(lib.find("broken.FP1").unwrap().display_name(), "broken");
        assert_eq!(lib.recipes_in(ALL_RECIPES).len(), 3);
        assert_eq!(lib.recipes_in(UNCATEGORIZED).len(), 3, "nothing is categorised yet");

        // A recipe can be in several categories.
        lib.set_membership("Favorites", "Tri-X.FP1", true).unwrap();
        lib.set_membership("Monochrome", "Tri-X.FP1", true).unwrap();
        lib.add_category("Holiday").unwrap();
        lib.set_membership("Holiday", "portra.fp1", true).unwrap();
        assert!(lib.add_category("holiday").is_err(), "names are unique whatever the capitals");
        assert!(lib.add_category("  ").is_err());

        // Everything survives a fresh read from disk.
        let mut lib = Library::open(&dir).unwrap();
        assert!(lib.is_in("Favorites", "Tri-X.FP1") && lib.is_in("Monochrome", "Tri-X.FP1"));
        assert_eq!(lib.recipes_in("Holiday")[0].display_name(), "Kodak Portra 400");

        // Deleting a category leaves the recipe files alone.
        lib.remove_category("Holiday").unwrap();
        assert!(lib.recipes_in("Holiday").is_empty());
        assert!(dir.join("portra.fp1").exists());
        assert_eq!(lib.recipes_in(ALL_RECIPES).len(), 3);
        // Uncategorized holds what is in no category: the Tri-X is in two.
        let loose: Vec<String> = lib.recipes_in(UNCATEGORIZED).iter().map(|r| r.file_name.clone()).collect();
        assert_eq!(loose, vec!["broken.FP1".to_string(), "portra.fp1".to_string()]);
        assert_eq!(lib.categories_of("Tri-X.FP1"), vec!["Favorites", "Monochrome"]);
        assert!(lib.remove_category("Favorites").is_err());
        assert!(lib.remove_category("Uncategorized").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn descriptions_are_stored_in_the_recipe_file() {
        let dir = temp_folder("desc");
        std::fs::create_dir_all(&dir).unwrap();
        let original = fp1("Kodak Tri-X", "Acros");
        std::fs::write(dir.join("Tri-X.FP1"), &original).unwrap();
        let mut lib = Library::open(&dir).unwrap();
        lib.set_description("Tri-X.FP1", "Gritty and contrasty.").unwrap();
        let lib = Library::open(&dir).unwrap();
        let recipe = lib.find("Tri-X.FP1").unwrap().recipe.as_ref().unwrap();
        assert_eq!(recipe.description, "Gritty and contrasty.");
        assert_eq!(recipe.field("FilmSimulation"), Some("Acros"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notices_changes_on_disk_and_names_new_files_safely() {
        let dir = temp_folder("change");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CATEGORIES_FILE), format!("{STARTERS_MARK}\n[Favorites]\n")).unwrap();
        let mut lib = Library::open(&dir).unwrap();
        assert!(lib.recipes.is_empty(), "a library marked as started gets no starters");
        assert!(!lib.changed_on_disk());
        std::fs::write(dir.join("New.FP1"), fp1("New", "Provia")).unwrap();
        assert!(lib.changed_on_disk());
        lib.rescan();
        assert!(!lib.changed_on_disk());
        assert_eq!(lib.recipes.len(), 1);

        let a = lib.create("My: look/1", &fp1("My: look/1", "Velvia")).unwrap();
        assert_eq!(a, "My_ look_1.FP1");
        let b = lib.create("My: look/1", &fp1("My: look/1", "Velvia")).unwrap();
        assert_eq!(b, "My_ look_1 (2).FP1");
        assert_eq!(lib.recipes.len(), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod editing {
    use super::*;
    use crate::settings::Level;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xrp-edit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CATEGORIES_FILE), format!("{STARTERS_MARK}\n[Favorites]\n")).unwrap();
        dir
    }

    #[test]
    fn importing_copies_files_and_keeps_both_on_a_name_clash() {
        let lib_dir = temp("import-lib");
        let outside = temp("import-src");
        let source = outside.join("Look.FP1");
        std::fs::write(&source, Recipe::build("Look", "X-T3", "", &[("FilmSimulation".into(), "Acros".into())])).unwrap();
        std::fs::write(outside.join("bad.FP1"), "nope").unwrap();

        let mut lib = Library::open(&lib_dir).unwrap();
        let (added, problems) = lib.import(&[source.clone(), outside.join("bad.FP1")]);
        assert_eq!(added, vec!["Look.FP1".to_string()]);
        assert_eq!(problems.len(), 1);
        let (added, _) = lib.import(std::slice::from_ref(&source));
        assert_eq!(added, vec!["Look (2).FP1".to_string()]);
        assert!(source.exists(), "the original is left where it was");
        assert_eq!(lib.recipes.len(), 2);
        // A folder stands for every recipe inside it, at any depth.
        let deep = outside.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Deep.fp1"), Recipe::build("Deep", "", "", &[])).unwrap();
        let (mut added, problems) = lib.import(std::slice::from_ref(&outside));
        added.sort();
        assert_eq!(added, vec!["Deep.FP1".to_string(), "Look (3).FP1".to_string()]);
        assert_eq!(problems.len(), 1, "the unreadable file is reported, not fatal");
        // Duplicating keeps the settings and the categories.
        lib.set_membership("Favorites", "Deep.FP1", true).unwrap();
        let copy = lib.duplicate("Deep.FP1").unwrap();
        assert_eq!(copy, "Deep copy.FP1");
        assert_eq!(lib.find(&copy).unwrap().display_name(), "Deep copy");
        assert!(lib.is_in("Favorites", &copy));
        // Exporting copies files out without overwriting.
        let out = outside.join("exported");
        assert_eq!(lib.export_copies(&["Deep.FP1".into(), copy.clone()], &out).unwrap(), 2);
        assert_eq!(lib.export_copies(&["Deep.FP1".into()], &out).unwrap(), 1);
        assert!(out.join("Deep.FP1").exists() && out.join("Deep (2).FP1").exists());
        let before = lib.recipes.len();
        // A file already in the library is not imported onto itself.
        let (added, problems) = lib.import(&[lib_dir.join("Look.FP1")]);
        assert!(added.is_empty() && problems.is_empty());
        assert_eq!(lib.recipes.len(), before);
        std::fs::remove_dir_all(lib_dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn saving_edits_changes_settings_and_keeps_everything_else() {
        let dir = temp("save");
        let original = r#"<?xml version="1.0" encoding="utf-8"?>
<ConversionProfile application="XRFC" version="1.11.0.0">
    <PropertyGroup device="X-T3" version="X-T3_0100" label="Mine">
        <IOPCode>FF159501</IOPCode>
        <FilmSimulation>Classic</FilmSimulation>
        <HighlightTone>1</HighlightTone>
        <HDR/>
    </PropertyGroup>
</ConversionProfile>
"#;
        std::fs::write(dir.join("Mine.FP1"), original).unwrap();
        let mut lib = Library::open(&dir).unwrap();
        lib.set_description("Mine.FP1", "Keep me").unwrap();

        let mut settings = lib.find("Mine.FP1").unwrap().settings.clone();
        settings.highlight = Some(-15);
        settings.color_chrome = Some(Level::Strong);
        lib.save_settings("Mine.FP1", &settings).unwrap();

        let entry = lib.find("Mine.FP1").unwrap();
        let recipe = entry.recipe.as_ref().unwrap();
        assert_eq!(entry.settings, settings);
        assert_eq!(recipe.name, "Mine");
        assert_eq!((recipe.device.as_str(), recipe.device_version.as_str()), ("X-T3", "X-T3_0100"));
        assert_eq!(recipe.description, "Keep me");
        assert_eq!(recipe.field("IOPCode"), Some("FF159501"), "fields the editor does not own are kept");
        assert_eq!(recipe.field("FilmSimulation"), Some("Classic"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod real_files {
    use super::*;

    /// Reads real X RAW Studio recipe files when a folder of them is available
    /// (set XRP_SAMPLE_RECIPES), and reports anything not understood.
    #[test]
    fn understands_real_recipe_files_when_available() {
        let Some(dir) = std::env::var_os("XRP_SAMPLE_RECIPES") else { return };
        let files = list_recipe_files(Path::new(&dir));
        assert!(!files.is_empty());
        let mut problems = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            match Recipe::parse(&text) {
                Ok(recipe) => {
                    let s = Settings::from_recipe(&recipe);
                    assert!(!recipe.name.is_empty());
                    if !s.not_understood.is_empty() {
                        problems.push(format!("{}: {:?}", path.display(), s.not_understood));
                    }
                    // The description edit must leave the settings untouched.
                    let again = Recipe::parse(&recipe.with_description("test")).unwrap();
                    assert_eq!(again.fields, recipe.fields);
                    assert_eq!(again.with_description(""), text.trim_start_matches('\u{feff}'));
                }
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
