//! Tabla de códigos físicos del D100H → identidad lógica de cada control.
//!
//! Valores medidos en un D100H real (27-Oct-2026) capturando evdev en
//! exclusiva: la rueda emite volumen, la pulsación emite mute y los tres
//! botones superiores emiten teclas multimedia. El análisis del firmware de
//! AetherSDR (#3232) que traía el mockup no coincidía con el aparato.
//!
//! El firmware suelta Ctrl antes que la letra del combo, y no informa de una
//! pulsación mantenida en la rueda: la pulsación larga no se puede detectar
//! por eventos, solo por tiempo mientras no llegue la liberación.

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
            rotate_cw: Key::KEY_VOLUMEUP,
            rotate_ccw: Key::KEY_VOLUMEDOWN,
            dial_press: Key::KEY_MUTE,
            buttons: vec![
                // Medidos en el aparato: la franja superior emite multimedia.
                (
                    ButtonId::TopLeft,
                    KeyCombo {
                        ctrl: false,
                        key: Key::KEY_PREVIOUSSONG,
                    },
                ),
                (
                    ButtonId::TopCenter,
                    KeyCombo {
                        ctrl: false,
                        key: Key::KEY_PLAYPAUSE,
                    },
                ),
                (
                    ButtonId::TopRight,
                    KeyCombo {
                        ctrl: false,
                        key: Key::KEY_NEXTSONG,
                    },
                ),
                // Los cuatro laterales emiten combos con Ctrl.
                (
                    ButtonId::SideLeftUpper,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_V,
                    },
                ),
                (
                    ButtonId::SideLeftLower,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_C,
                    },
                ),
                (
                    ButtonId::SideRightUpper,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_Y,
                    },
                ),
                (
                    ButtonId::SideRightLower,
                    KeyCombo {
                        ctrl: true,
                        key: Key::KEY_Z,
                    },
                ),
            ],
        }
    }
}

impl KeyMap {
    /// Botón asociado a una tecla, con su combo (para saber si exige Ctrl).
    pub fn combo_for(&self, key: Key) -> Option<(ButtonId, KeyCombo)> {
        self.buttons
            .iter()
            .find(|(_, combo)| combo.key == key)
            .map(|(id, combo)| (*id, *combo))
    }

    /// Botón asociado a una tecla sin mirar el estado de Ctrl.
    pub fn button_for(&self, key: Key) -> Option<ButtonId> {
        self.combo_for(key).map(|(id, _)| id)
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
    fn default_map_matches_measured_device() {
        let map = KeyMap::default();
        assert_eq!(map.rotate_cw, Key::KEY_VOLUMEUP);
        assert_eq!(map.rotate_ccw, Key::KEY_VOLUMEDOWN);
        assert_eq!(map.dial_press, Key::KEY_MUTE);
        assert_eq!(
            map.button_for(Key::KEY_PREVIOUSSONG),
            Some(ButtonId::TopLeft)
        );
        assert_eq!(
            map.button_for(Key::KEY_PLAYPAUSE),
            Some(ButtonId::TopCenter)
        );
        assert_eq!(map.button_for(Key::KEY_NEXTSONG), Some(ButtonId::TopRight));
        assert_eq!(map.button_for(Key::KEY_V), Some(ButtonId::SideLeftUpper));
        assert_eq!(map.button_for(Key::KEY_C), Some(ButtonId::SideLeftLower));
        assert_eq!(map.button_for(Key::KEY_Y), Some(ButtonId::SideRightUpper));
        assert_eq!(map.button_for(Key::KEY_Z), Some(ButtonId::SideRightLower));
    }

    #[test]
    fn media_buttons_do_not_require_ctrl() {
        let map = KeyMap::default();
        let (id, combo) = map.combo_for(Key::KEY_NEXTSONG).unwrap();
        assert_eq!(id, ButtonId::TopRight);
        assert!(!combo.ctrl);
        let (_, combo) = map.combo_for(Key::KEY_Z).unwrap();
        assert!(combo.ctrl);
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
