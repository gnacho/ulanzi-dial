//! Tabla de códigos físicos del D100H → identidad lógica de cada control.
//!
//! Valores por defecto tomados del análisis de AetherSDR (#3232) del firmware
//! de fábrica del Ulanzi Dial. El D100H tiene 7 botones (3 superiores +
//! 2 por lado); los tres combos documentados son Ctrl+Z / Ctrl+Y / Ctrl+V.
//! Los cuatro restantes son provisionales — confírmalos con `evtest` y
//! ajústalos sin recompilar en `[keymap]` del config.toml.

use evdev::Key;
use serde::{Deserialize, Serialize};

/// Identidad lógica de cada botón físico.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ButtonId {
    TopLeft,
    TopCenter,
    TopRight,
    SideLeftUpper,
    SideLeftLower,
    SideRightUpper,
    SideRightLower,
}

impl ButtonId {
    pub const ALL: [ButtonId; 7] = [
        ButtonId::TopLeft,
        ButtonId::TopCenter,
        ButtonId::TopRight,
        ButtonId::SideLeftUpper,
        ButtonId::SideLeftLower,
        ButtonId::SideRightUpper,
        ButtonId::SideRightLower,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ButtonId::TopLeft => "Superior izquierdo",
            ButtonId::TopCenter => "Superior central",
            ButtonId::TopRight => "Superior derecho",
            ButtonId::SideLeftUpper => "Lateral izq. · arriba",
            ButtonId::SideLeftLower => "Lateral izq. · abajo",
            ButtonId::SideRightUpper => "Lateral der. · arriba",
            ButtonId::SideRightLower => "Lateral der. · abajo",
        }
    }
}

/// Un combo emitido por el firmware: opcionalmente con Ctrl mantenido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub key: Key,
}

/// Mapa completo del dispositivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMap {
    pub rotate_cw: Key,
    pub rotate_ccw: Key,
    pub dial_press: Key,
    pub buttons: Vec<(ButtonId, KeyCombo)>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            rotate_cw: Key::KEY_NEXTSONG,
            rotate_ccw: Key::KEY_PREVIOUSSONG,
            dial_press: Key::KEY_PLAYPAUSE,
            buttons: vec![
                // Confirmados por evtest en AetherSDR #3232
                (
                    ButtonId::SideLeftUpper,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_Z,
                    },
                ),
                (
                    ButtonId::SideLeftLower,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_V,
                    },
                ),
                (
                    ButtonId::SideRightUpper,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_Y,
                    },
                ),
                // Provisionales (TODO: confirmar con evtest en el D100H)
                (
                    ButtonId::TopLeft,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_X,
                    },
                ),
                (
                    ButtonId::TopCenter,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_C,
                    },
                ),
                (
                    ButtonId::TopRight,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_B,
                    },
                ),
                (
                    ButtonId::SideRightLower,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_N,
                    },
                ),
            ],
        }
    }
}

impl KeyMap {
    pub fn button_for(&self, key: Key) -> Option<ButtonId> {
        self.buttons
            .iter()
            .find(|(_, combo)| combo.key == key)
            .map(|(id, _)| *id)
    }
}

/// Parsea combos de texto del config.toml, p. ej. "Ctrl+Z" o "PlayPause".
pub fn parse_combo(s: &str) -> Option<KeyCombo> {
    let mut ctrl = false;
    let mut key_name = s.trim();
    if let Some(rest) = s.strip_prefix("Ctrl+").or_else(|| s.strip_prefix("ctrl+")) {
        ctrl = true;
        key_name = rest.trim();
    }
    named_key(key_name).map(|key| KeyCombo { ctrl, key })
}

/// Nombres de tecla aceptados en el config (letras sueltas o nombres KEY_*).
pub fn named_key(name: &str) -> Option<Key> {
    let upper = name.to_ascii_uppercase();
    let with_prefix = if upper.starts_with("KEY_") {
        upper.clone()
    } else {
        format!("KEY_{upper}")
    };
    Some(match with_prefix.as_str() {
        "KEY_A" => Key::KEY_A,
        "KEY_B" => Key::KEY_B,
        "KEY_C" => Key::KEY_C,
        "KEY_D" => Key::KEY_D,
        "KEY_E" => Key::KEY_E,
        "KEY_F" => Key::KEY_F,
        "KEY_G" => Key::KEY_G,
        "KEY_H" => Key::KEY_H,
        "KEY_I" => Key::KEY_I,
        "KEY_J" => Key::KEY_J,
        "KEY_K" => Key::KEY_K,
        "KEY_L" => Key::KEY_L,
        "KEY_M" => Key::KEY_M,
        "KEY_N" => Key::KEY_N,
        "KEY_O" => Key::KEY_O,
        "KEY_P" => Key::KEY_P,
        "KEY_Q" => Key::KEY_Q,
        "KEY_R" => Key::KEY_R,
        "KEY_S" => Key::KEY_S,
        "KEY_T" => Key::KEY_T,
        "KEY_U" => Key::KEY_U,
        "KEY_V" => Key::KEY_V,
        "KEY_W" => Key::KEY_W,
        "KEY_X" => Key::KEY_X,
        "KEY_Y" => Key::KEY_Y,
        "KEY_Z" => Key::KEY_Z,
        "KEY_PLAYPAUSE" => Key::KEY_PLAYPAUSE,
        "KEY_NEXTSONG" => Key::KEY_NEXTSONG,
        "KEY_PREVIOUSSONG" => Key::KEY_PREVIOUSSONG,
        "KEY_MUTE" => Key::KEY_MUTE,
        "KEY_VOLUMEUP" => Key::KEY_VOLUMEUP,
        "KEY_VOLUMEDOWN" => Key::KEY_VOLUMEDOWN,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_map_resolves_confirmed_buttons() {
        let map = KeyMap::default();
        assert_eq!(map.button_for(Key::KEY_Z), Some(ButtonId::SideLeftUpper));
        assert_eq!(map.button_for(Key::KEY_Y), Some(ButtonId::SideRightUpper));
        assert_eq!(map.button_for(Key::KEY_V), Some(ButtonId::SideLeftLower));
    }

    #[test]
    fn parse_combos() {
        assert_eq!(
            parse_combo("Ctrl+Z"),
            Some(KeyCombo {
                ctrl: true,
                key: Key::KEY_Z
            })
        );
        assert_eq!(
            parse_combo("ctrl+y"),
            Some(KeyCombo {
                ctrl: true,
                key: Key::KEY_Y
            })
        );
        assert_eq!(
            parse_combo("PlayPause"),
            Some(KeyCombo {
                ctrl: false,
                key: Key::KEY_PLAYPAUSE
            })
        );
        assert_eq!(
            parse_combo("KEY_NEXTSONG"),
            Some(KeyCombo {
                ctrl: false,
                key: Key::KEY_NEXTSONG
            })
        );
        assert_eq!(parse_combo("NotAKey"), None);
    }

    #[test]
    fn seven_buttons_mapped() {
        let map = KeyMap::default();
        for id in ButtonId::ALL {
            assert!(map.buttons.iter().any(|(b, _)| *b == id), "falta {id:?}");
        }
    }
}
