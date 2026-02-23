use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::models::OpenClawPaths;

pub const DEFAULT_CONFIG: &str = r#"{}"#;

pub fn ensure_dirs(paths: &OpenClawPaths) -> Result<(), String> {
    fs::create_dir_all(&paths.base_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&paths.history_dir).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_text(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(DEFAULT_CONFIG.to_string());
    }

    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut content = String::new();
    file.read_to_string(&mut content).map_err(|e| e.to_string())?;
    Ok(content)
}

pub fn write_text(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_json<T>(path: &Path) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let text = read_text(path)?;
    let parsed = json5::from_str::<T>(&text).map_err(|e| e.to_string())?;
    Ok(parsed)
}

pub fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    write_text(path, &pretty)
}

pub fn read_openclaw_config(paths: &OpenClawPaths) -> Result<Value, String> {
    ensure_dirs(paths)?;
    match read_json::<Value>(&paths.config_path) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Config may be mid-write by another process — retry once after short delay
            std::thread::sleep(std::time::Duration::from_millis(50));
            read_json::<Value>(&paths.config_path)
                .or_else(|_| Ok(Value::Object(Default::default())))
        }
    }
}
