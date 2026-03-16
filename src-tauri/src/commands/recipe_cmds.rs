use crate::models::resolve_paths;
use crate::recipe::load_recipes_with_fallback;

#[tauri::command]
pub fn list_recipes(source: Option<String>) -> Result<Vec<crate::recipe::Recipe>, String> {
    let paths = resolve_paths();
    let default_path = paths.clawpal_dir.join("recipes").join("recipes.json");
    Ok(load_recipes_with_fallback(source, &default_path))
}
