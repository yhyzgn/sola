use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub palette: ThemePalette,
    pub typography: ThemeTypography,
    pub syntax: SyntaxTheme,
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
    pub selection: String,
    pub cursor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxTheme {
    pub keyword: String,
    pub string: String,
    pub comment: String,
    pub function: String,
    pub number: String,
    pub constant: String,
    pub type_name: String,
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
                selection: "#3e4451".into(),
                cursor: "#ffffff".into(),
            },
            typography: ThemeTypography {
                ui_scale: 100,
                body_size: 15,
                title_size: 24,
                code_size: 14,
            },
            syntax: SyntaxTheme {
                keyword: "#c678dd".into(),
                string: "#98c379".into(),
                comment: "#5c6370".into(),
                function: "#61afef".into(),
                number: "#d19a66".into(),
                constant: "#e06c75".into(),
                type_name: "#e5c07b".into(),
            },
        }
    }

    pub fn sola_light() -> Self {
        Self {
            name: "sola-light".into(),
            palette: ThemePalette {
                app_background: "#f8fafc".into(),
                panel_background: "#ffffff".into(),
                panel_border: "#dbe4f0".into(),
                text_primary: "#0f172a".into(),
                text_muted: "#64748b".into(),
                accent: "#7c3aed".into(),
                focused_background: "#f3e8ff".into(),
                focused_border: "#8b5cf6".into(),
                code_background: "#eef2ff".into(),
                selection: "#dbeafe".into(),
                cursor: "#000000".into(),
            },
            typography: ThemeTypography {
                ui_scale: 100,
                body_size: 15,
                title_size: 24,
                code_size: 14,
            },
            syntax: SyntaxTheme {
                keyword: "#a626a4".into(),
                string: "#50a14f".into(),
                comment: "#a0a1a7".into(),
                function: "#4078f2".into(),
                number: "#986801".into(),
                constant: "#e45649".into(),
                type_name: "#c18401".into(),
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
selection = "#3e4451"
cursor = "#ffffff"

[typography]
ui_scale = 100
body_size = 15
title_size = 24
code_size = 14

[syntax]
keyword = "#c678dd"
string = "#98c379"
comment = "#5c6370"
function = "#61afef"
number = "#d19a66"
constant = "#e06c75"
type_name = "#e5c07b"
"##,
        )
        .expect("theme should parse from TOML");

        assert_eq!(theme.name, "custom");
        assert_eq!(parse_hex_color(&theme.palette.accent), Some(0x7c3aed));
        assert_eq!(parse_hex_color(&theme.syntax.keyword), Some(0xc678dd));
    }

    #[test]
    fn light_theme_variant_is_available() {
        let theme = Theme::sola_light();

        assert_eq!(theme.name, "sola-light");
        assert_eq!(
            parse_hex_color(&theme.palette.app_background),
            Some(0xf8fafc)
        );
    }
}
