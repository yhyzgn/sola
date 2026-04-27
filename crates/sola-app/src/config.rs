use crate::workspace::ThemeMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub theme_mode: ThemeMode,
    pub recent_paths: Vec<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            recent_paths: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        if let Some(path) = Self::path() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = toml::to_string_pretty(self)?;
            fs::write(path, content)?;
        }
        Ok(())
    }

    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("sola").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = AppConfig {
            theme_mode: ThemeMode::Light,
            recent_paths: vec![PathBuf::from("/test/path")],
        };
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("theme_mode = \"Light\""));
        assert!(toml_str.contains("/test/path"));

        let decoded: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded.theme_mode, ThemeMode::Light);
        assert_eq!(decoded.recent_paths.len(), 1);
    }
}
