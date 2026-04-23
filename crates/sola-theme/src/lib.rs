use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub palette: ThemePalette,
    pub typography: ThemeTypography,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub app_background: String,
    pub panel_background: String,
    pub panel_border: String,
    pub text_primary: String,
    pub text_muted: String,
    pub accent: String,
    pub focused_background: String,
    pub focused_border: String,
    pub code_background: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeTypography {
    pub ui_scale: u16,
    pub body_size: u16,
    pub title_size: u16,
    pub code_size: u16,
}

impl Theme {
    pub fn sola_dark() -> Self {
        Self {
            name: "sola-dark".into(),
            palette: ThemePalette {
                app_background: "#0f1117".into(),
                panel_background: "#171a21".into(),
                panel_border: "#2b3240".into(),
                text_primary: "#f3f4f6".into(),
                text_muted: "#94a3b8".into(),
                accent: "#8b5cf6".into(),
                focused_background: "#1d2330".into(),
                focused_border: "#c084fc".into(),
                code_background: "#111827".into(),
            },
            typography: ThemeTypography {
                ui_scale: 100,
                body_size: 15,
                title_size: 24,
                code_size: 14,
            },
        }
    }

    pub fn from_toml_str(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(input)
    }
}

pub fn parse_hex_color(input: &str) -> Option<u32> {
    let normalized = input.trim().trim_start_matches('#');
    if normalized.len() != 6 {
        return None;
    }

    u32::from_str_radix(normalized, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_required_semantic_fields() {
        let theme = Theme::sola_dark();

        assert_eq!(theme.name, "sola-dark");
        assert!(theme.palette.app_background.starts_with('#'));
        assert!(theme.palette.focused_border.starts_with('#'));
        assert!(theme.typography.title_size > theme.typography.body_size);
    }

    #[test]
    fn theme_can_be_loaded_from_toml() {
        let theme = Theme::from_toml_str(
            r##"
name = "custom"

[palette]
app_background = "#101010"
panel_background = "#1a1a1a"
panel_border = "#2b2b2b"
text_primary = "#efefef"
text_muted = "#a1a1aa"
accent = "#7c3aed"
focused_background = "#18181b"
focused_border = "#c084fc"
code_background = "#09090b"

[typography]
ui_scale = 100
body_size = 15
title_size = 24
code_size = 14
"##,
        )
        .expect("theme should parse from TOML");

        assert_eq!(theme.name, "custom");
        assert_eq!(parse_hex_color(&theme.palette.accent), Some(0x7c3aed));
    }
}
