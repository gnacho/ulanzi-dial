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
    pub show_hud: bool,
    pub exclusive_grab: bool,
    pub autostart: bool,

    /// Rueda: global por diseño, no se perfilea (el firmware no emite
    /// eventos de rueda mientras hay un botón mantenido).
    pub dial_rotate: Action,
    pub dial_press: Action,

    /// Asignación por botón físico (claves kebab-case: "top-left", …).
    pub buttons: HashMap<ButtonId, ControlBinding>,

    /// Overrides del mapa físico, p. ej. "top-left" = "Ctrl+X".
    /// Úsalos con la salida de `evtest` si el firmware difiere.
    pub keymap: HashMap<ButtonId, String>,

    /// Perfiles por aplicación (conmutación automática: pendiente de
    /// foreign-toplevel en Wayland; por ahora se aplican manualmente).
    pub profiles: HashMap<String, Profile>,
    /// Perfil fijado a mano en la pestaña General; manda sobre la app activa.
    pub pinned_profile: Option<String>,
    /// Perfil impuesto por la ventana activa. No se guarda en el fichero.
    #[serde(skip)]
    pub active_profile: Option<String>,
}

/// Asignaciones propias de una aplicación concreta.
///
/// Solo los botones se perfilean: la rueda es global por diseño (el
/// firmware no emite eventos de rueda mientras hay un botón mantenido).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub buttons: HashMap<ButtonId, ControlBinding>,
}

impl Default for Config {
    fn default() -> Self {
        let mut buttons = HashMap::new();
        buttons.insert(
            ButtonId::TopLeft,
            ControlBinding {
                press: Action::Undo,
            },
        );
        buttons.insert(
            ButtonId::TopCenter,
            ControlBinding {
                press: Action::PlayPause,
            },
        );
        buttons.insert(
            ButtonId::TopRight,
            ControlBinding {
                press: Action::Redo,
            },
        );
        buttons.insert(
            ButtonId::SideLeftUpper,
            ControlBinding {
                press: Action::Copy,
            },
        );
        buttons.insert(
            ButtonId::SideLeftLower,
            ControlBinding {
                press: Action::Paste,
            },
        );
        buttons.insert(
            ButtonId::SideRightUpper,
            ControlBinding {
                press: Action::Screenshot,
            },
        );
        buttons.insert(
            ButtonId::SideRightLower,
            ControlBinding {
                press: Action::Lock,
            },
        );
        Self {
            sensitivity: 5,
            volume_mode: VolumeMode::Keys,
            volume_step: 3,
            invert: false,
            show_hud: true,
            exclusive_grab: true,
            autostart: false,
            dial_rotate: Action::Volume,
            dial_press: Action::Mute,
            buttons,
            keymap: HashMap::new(),
            profiles: HashMap::new(),
            pinned_profile: None,
            active_profile: None,
        }
    }
}

impl Config {
    /// Clave del perfil que manda ahora mismo: el fijado a mano o el de la app activa.
    pub fn current_profile_key(&self) -> Option<&String> {
        self.pinned_profile
            .as_ref()
            .or(self.active_profile.as_ref())
    }

    pub fn current_profile(&self) -> Option<&Profile> {
        self.current_profile_key()
            .and_then(|key| self.profiles.get(key))
    }

    /// Acción efectiva de un botón: la del perfil activo si la tiene, si no la global.
    pub fn press_for(&self, id: ButtonId) -> Action {
        self.current_profile()
            .and_then(|profile| profile.buttons.get(&id))
            .map(|binding| binding.press.clone())
            .unwrap_or_else(|| {
                self.buttons
                    .get(&id)
                    .map(|binding| binding.press.clone())
                    .unwrap_or(Action::None)
            })
    }

    /// Guarda la acción de un botón en el perfil activo, creándolo si hace falta.
    pub fn set_press(&mut self, id: ButtonId, action: Action) {
        match self.current_profile_key().cloned() {
            Some(key) => {
                let profile = self.profiles.entry(key).or_default();
                profile.buttons.insert(id, ControlBinding { press: action });
            }
            None => {
                self.buttons.insert(id, ControlBinding { press: action });
            }
        }
    }

    /// Fija la aplicación activa. Devuelve true si cambió el perfil efectivo.
    pub fn set_active_app(&mut self, app: Option<String>) -> bool {
        let before = self.current_profile_key().cloned();
        self.active_profile = app.filter(|app| self.profiles.contains_key(app));
        self.current_profile_key().cloned() != before
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
                Err(e) => {
                    // Conserva el fichero roto antes de que el siguiente
                    // save lo pise: sin esto, un TOML corrupto acababa
                    // sustituido por valores por defecto sin aviso.
                    let bak = path.with_extension("toml.bak");
                    match std::fs::copy(&path, &bak) {
                        Ok(_) => tracing::warn!(
                            "config.toml inválido ({e}); copia en config.toml.bak; usando valores por defecto"
                        ),
                        Err(_) => {
                            tracing::warn!("config.toml inválido ({e}); usando valores por defecto")
                        }
                    }
                }
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
        let text = match toml::to_string_pretty(self) {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!("no se pudo serializar la config: {e}");
                return;
            }
        };
        // Escritura atómica: temporal con 0600 (los comandos personalizados
        // pueden contener tokens) y rename encima del destino. Un crash a
        // mitad ya no puede dejar un TOML a medias.
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let tmp = path.with_extension("toml.tmp");
        let written = std::fs::OpenOptions::new()
            .mode(0o600)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .and_then(|mut file| file.write_all(text.as_bytes()));
        match written {
            Ok(()) => {
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    tracing::warn!("no se pudo reemplazar config.toml: {e}");
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Err(e) => {
                tracing::warn!("no se pudo guardar config.toml: {e}");
                let _ = std::fs::remove_file(&tmp);
            }
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

    #[test]
    fn profile_overrides_and_falls_back() {
        let mut cfg = Config::default();
        cfg.profiles.insert("firefox".into(), Profile::default());
        assert_eq!(cfg.press_for(ButtonId::TopLeft), Action::Undo);

        assert!(cfg.set_active_app(Some("firefox".into())));
        assert_eq!(cfg.press_for(ButtonId::TopLeft), Action::Undo);

        cfg.set_press(ButtonId::TopLeft, Action::Redo);
        assert_eq!(cfg.press_for(ButtonId::TopLeft), Action::Redo);
        assert_eq!(
            cfg.buttons.get(&ButtonId::TopLeft).map(|b| b.press.clone()),
            Some(Action::Undo),
            "la asignación global no cambia"
        );

        assert!(cfg.set_active_app(None));
        assert_eq!(cfg.press_for(ButtonId::TopLeft), Action::Undo);
    }

    #[test]
    fn unknown_app_is_ignored() {
        let mut cfg = Config::default();
        assert!(!cfg.set_active_app(Some("not-a-profile".into())));
        assert_eq!(cfg.current_profile_key(), None);
        assert_eq!(cfg.press_for(ButtonId::TopLeft), Action::Undo);
    }

    #[test]
    fn pinned_profile_wins_over_active() {
        let mut cfg = Config::default();
        cfg.profiles.insert("a".into(), Profile::default());
        cfg.profiles.insert("b".into(), Profile::default());
        cfg.set_active_app(Some("a".into()));
        cfg.pinned_profile = Some("b".into());
        assert_eq!(cfg.current_profile_key().map(String::as_str), Some("b"));

        cfg.set_press(ButtonId::TopCenter, Action::Mute);
        assert_eq!(
            cfg.profiles
                .get("b")
                .and_then(|p| p.buttons.get(&ButtonId::TopCenter))
                .map(|b| b.press.clone()),
            Some(Action::Mute)
        );
        assert!(cfg
            .profiles
            .get("a")
            .and_then(|p| p.buttons.get(&ButtonId::TopCenter))
            .is_none());
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_config_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ulanzi-dial-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ulanzi-dial")).unwrap();
        dir
    }

    #[test]
    fn save_writes_atomically_with_private_permissions() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_config_dir("save");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let mut cfg = Config::default();
        cfg.autostart = true;
        cfg.save();

        let path = dir.join("ulanzi-dial/config.toml");
        assert!(path.exists());
        assert!(!dir.join("ulanzi-dial/config.toml.tmp").exists());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(Config::load().autostart);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_keeps_a_backup_of_a_corrupt_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_config_dir("corrupt");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::fs::write(dir.join("ulanzi-dial/config.toml"), "esto no es toml [[[ ").unwrap();

        let cfg = Config::load();
        assert!(!cfg.autostart, "cae a valores por defecto");
        assert!(dir.join("ulanzi-dial/config.toml.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
