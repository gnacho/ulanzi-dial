//! Configuración persistente en `~/.config/ulanzi-dial/config.toml`.
//!
//! Elegimos TOML en lugar de GSettings para que la app funcione sin instalar
//! esquemas (cargo run directo). Migrar a GSettings es trivial si se empaqueta.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::backend::actions::Action;
use crate::backend::keymap::{parse_combo, ButtonId, KeyMap};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlBinding {
    /// Acción al pulsar el botón.
    pub press: Action,
    /// Acción de la rueda mientras el botón se mantiene pulsado.
    pub spin: Action,
}

/// Cómo se aplica el volumen al girar la rueda.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeMode {
    /// Teclas multimedia: el escritorio pinta su propio OSD y aplica su salto.
    Keys,
    /// Mezclador de audio (wpctl/pactl): salto `volume_step`, sin OSD del escritorio.
    Mixer,
}

impl Default for VolumeMode {
    fn default() -> Self {
        Self::Keys
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Muescas por evento en las acciones continuas (scroll, zoom).
    pub sensitivity: u8,
    /// Cómo se aplica el volumen de la rueda.
    pub volume_mode: VolumeMode,
    /// Salto de volumen en tanto por ciento por muesca (solo `volume_mode = "mixer"`).
    pub volume_step: u8,
    pub invert: bool,
    pub long_press_ms: u64,
    pub show_hud: bool,
    pub exclusive_grab: bool,
    pub autostart: bool,

    pub dial_rotate: Action,
    pub dial_press: Action,
    pub dial_long_press: Action,

    /// Asignación por botón físico (claves kebab-case: "top-left", …).
    pub buttons: HashMap<ButtonId, ControlBinding>,

    /// Overrides del mapa físico, p. ej. "top-left" = "Ctrl+X".
    /// Úsalos con la salida de `evtest` si el firmware difiere.
    pub keymap: HashMap<ButtonId, String>,

    /// Perfiles por aplicación (conmutación automática: pendiente de
    /// foreign-toplevel en Wayland; por ahora se aplican manualmente).
    pub profiles: HashMap<String, Profile>,
}

/// Un perfil completo de asignaciones (futuro: activado por ventana activa).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub dial_rotate: Action,
    pub buttons: HashMap<ButtonId, ControlBinding>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            dial_rotate: Action::Volume,
            buttons: HashMap::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut buttons = HashMap::new();
        buttons.insert(
            ButtonId::TopLeft,
            ControlBinding {
                press: Action::Undo,
                spin: Action::Zoom,
            },
        );
        buttons.insert(
            ButtonId::TopCenter,
            ControlBinding {
                press: Action::PlayPause,
                spin: Action::Volume,
            },
        );
        buttons.insert(
            ButtonId::TopRight,
            ControlBinding {
                press: Action::Redo,
                spin: Action::Brightness,
            },
        );
        buttons.insert(
            ButtonId::SideLeftUpper,
            ControlBinding {
                press: Action::Copy,
                spin: Action::Scroll,
            },
        );
        buttons.insert(
            ButtonId::SideLeftLower,
            ControlBinding {
                press: Action::Paste,
                spin: Action::HScroll,
            },
        );
        buttons.insert(
            ButtonId::SideRightUpper,
            ControlBinding {
                press: Action::Screenshot,
                spin: Action::Track,
            },
        );
        buttons.insert(
            ButtonId::SideRightLower,
            ControlBinding {
                press: Action::Lock,
                spin: Action::None,
            },
        );
        Self {
            sensitivity: 5,
            volume_mode: VolumeMode::Keys,
            volume_step: 3,
            invert: false,
            long_press_ms: 750,
            show_hud: true,
            exclusive_grab: true,
            autostart: false,
            dial_rotate: Action::Volume,
            dial_press: Action::Mute,
            dial_long_press: Action::None,
            buttons,
            keymap: HashMap::new(),
            profiles: HashMap::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ulanzi-dial").join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(cfg) => return cfg,
                Err(e) => tracing::warn!("config.toml inválido ({e}); usando valores por defecto"),
            },
            Err(_) => tracing::info!("sin config.toml; usando valores por defecto"),
        }
        Config::default()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    tracing::warn!("no se pudo guardar config.toml: {e}");
                }
            }
            Err(e) => tracing::warn!("no se pudo serializar la config: {e}"),
        }
    }

    /// KeyMap por defecto con los overrides del config aplicados.
    pub fn keymap(&self) -> KeyMap {
        let mut map = KeyMap::default();
        for (id, combo_str) in &self.keymap {
            if let Some(combo) = parse_combo(combo_str) {
                if let Some(entry) = map.buttons.iter_mut().find(|(b, _)| b == id) {
                    entry.1 = combo;
                }
            } else {
                tracing::warn!("keymap: combo '{combo_str}' no reconocido para {id:?}");
            }
        }
        map
    }
}

// --- Serialización de Action como cadena plana en TOML ---
// Las variantes sin payload se guardan como "volume"; con payload como tabla.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::actions::action_from_str;

    #[test]
    fn default_roundtrips_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).expect("serialize");
        let back: Config = toml::from_str(&text).expect("deserialize");
        assert_eq!(back.sensitivity, 5);
        assert_eq!(back.long_press_ms, 750);
        assert_eq!(back.buttons.len(), 7);
        assert_eq!(back.buttons[&ButtonId::TopLeft].press, Action::Undo);
    }

    #[test]
    fn parses_minimal_toml_with_defaults() {
        let text = r#"
            sensitivity = 8
            invert = true
        "#;
        let cfg: Config = toml::from_str(text).expect("parse");
        assert_eq!(cfg.sensitivity, 8);
        assert!(cfg.invert);
        assert_eq!(cfg.long_press_ms, 750); // default rellenado
        assert_eq!(cfg.buttons.len(), 7); // default rellenado
        assert_eq!(cfg.volume_mode, VolumeMode::Keys);
    }

    #[test]
    fn parses_volume_mode() {
        let cfg: Config = toml::from_str("volume_mode = \"mixer\"").expect("parse");
        assert_eq!(cfg.volume_mode, VolumeMode::Mixer);
        let cfg: Config = toml::from_str("volume_mode = \"keys\"").expect("parse");
        assert_eq!(cfg.volume_mode, VolumeMode::Keys);
    }

    #[test]
    fn keymap_override_applies() {
        let mut cfg = Config::default();
        cfg.keymap.insert(ButtonId::TopLeft, "Ctrl+Q".into());
        let map = cfg.keymap();
        let combo = map
            .buttons
            .iter()
            .find(|(b, _)| *b == ButtonId::TopLeft)
            .unwrap();
        assert_eq!(combo.1.key, evdev::Key::KEY_Q);
        assert!(combo.1.ctrl);
    }

    #[test]
    fn command_action_parses() {
        assert_eq!(
            action_from_str("command:notify-send hola"),
            Action::Command("notify-send hola".into())
        );
    }
}
