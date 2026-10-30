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

/// Un atajo: modificadores opcionales más una tecla.
///
/// El firmware del dial solo emite Ctrl (y lo suelta antes que la letra), pero
/// los atajos configurados a mano necesitan Shift, Alt y Super para reproducir
/// los atajos de GNOME (Ctrl+Shift+T, Alt+Print, Super+D...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
    pub key: Key,
}

impl KeyCombo {
    pub fn plain(key: Key) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            logo: false,
            key,
        }
    }

    pub fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::plain(key)
        }
    }

    /// Representación para la UI y el config.toml: "Ctrl+Shift+T".
    pub fn label(&self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
            (self.logo, "Super"),
        ] {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(&key_name(self.key));
        out
    }
}

/// Nombre legible de una tecla ("T", "F11", "Tab").
pub fn key_name(key: Key) -> String {
    let debug = format!("{key:?}");
    let raw = debug.strip_prefix("KEY_").unwrap_or(&debug);
    match raw {
        "LEFTMETA" => "Super".to_string(),
        "LEFTCTRL" => "Ctrl".to_string(),
        "LEFTSHIFT" => "Shift".to_string(),
        "LEFTALT" => "Alt".to_string(),
        "PLAYPAUSE" => "PlayPause".to_string(),
        "NEXTSONG" => "NextSong".to_string(),
        "PREVIOUSSONG" => "PreviousSong".to_string(),
        "VOLUMEUP" => "VolumeUp".to_string(),
        "VOLUMEDOWN" => "VolumeDown".to_string(),
        "UP" => "Up".to_string(),
        "DOWN" => "Down".to_string(),
        "LEFT" => "Left".to_string(),
        "RIGHT" => "Right".to_string(),
        "PAGEUP" => "PageUp".to_string(),
        "PAGEDOWN" => "PageDown".to_string(),
        "BACKSPACE" => "Backspace".to_string(),
        "ENTER" => "Enter".to_string(),
        "ESC" => "Esc".to_string(),
        "TAB" => "Tab".to_string(),
        "SPACE" => "Space".to_string(),
        "DELETE" => "Delete".to_string(),
        "INSERT" => "Insert".to_string(),
        "HOME" => "Home".to_string(),
        "END" => "End".to_string(),
        "PRINT" => "Print".to_string(),
        "SYSRQ" => "Print".to_string(),
        "MENU" => "Menu".to_string(),
        other => other.to_string(),
    }
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
                (ButtonId::TopLeft, KeyCombo::plain(Key::KEY_PREVIOUSSONG)),
                (ButtonId::TopCenter, KeyCombo::plain(Key::KEY_PLAYPAUSE)),
                (ButtonId::TopRight, KeyCombo::plain(Key::KEY_NEXTSONG)),
                // Los cuatro laterales emiten combos con Ctrl.
                (ButtonId::SideLeftUpper, KeyCombo::ctrl(Key::KEY_V)),
                (ButtonId::SideLeftLower, KeyCombo::ctrl(Key::KEY_C)),
                (ButtonId::SideRightUpper, KeyCombo::ctrl(Key::KEY_Y)),
                (ButtonId::SideRightLower, KeyCombo::ctrl(Key::KEY_Z)),
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
}

/// Parsea combos de texto del config.toml: "Ctrl+Z", "Ctrl+Shift+T",
/// "F11", "Super+D", "PlayPause".
///
/// Los modificadores se aceptan en cualquier orden y sin distinguir mayúsculas.
/// Para un "+" literal hay que escribir "Plus".
pub fn parse_combo(s: &str) -> Option<KeyCombo> {
    let tokens: Vec<&str> = s
        .split('+')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let (modifiers, key_token) = tokens.split_at(tokens.len() - 1);
    let mut combo = KeyCombo::plain(Key::KEY_RESERVED);
    for modifier in modifiers {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => combo.ctrl = true,
            "shift" => combo.shift = true,
            "alt" | "option" => combo.alt = true,
            "super" | "meta" | "logo" | "win" => combo.logo = true,
            _ => return None,
        }
    }
    combo.key = named_key(key_token[0])?;
    Some(combo)
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
        "KEY_TAB" => Key::KEY_TAB,
        "KEY_ENTER" => Key::KEY_ENTER,
        "KEY_ESC" | "KEY_ESCAPE" => Key::KEY_ESC,
        "KEY_SPACE" => Key::KEY_SPACE,
        "KEY_BACKSPACE" => Key::KEY_BACKSPACE,
        "KEY_DELETE" => Key::KEY_DELETE,
        "KEY_INSERT" => Key::KEY_INSERT,
        "KEY_HOME" => Key::KEY_HOME,
        "KEY_END" => Key::KEY_END,
        "KEY_PAGEUP" => Key::KEY_PAGEUP,
        "KEY_PAGEDOWN" => Key::KEY_PAGEDOWN,
        "KEY_LEFT" => Key::KEY_LEFT,
        "KEY_RIGHT" => Key::KEY_RIGHT,
        "KEY_UP" => Key::KEY_UP,
        "KEY_DOWN" => Key::KEY_DOWN,
        "KEY_PRINT" | "KEY_SYSRQ" => Key::KEY_SYSRQ,
        "KEY_MENU" => Key::KEY_MENU,
        "KEY_0" | "KEY_DIGIT0" => Key::KEY_0,
        "KEY_1" | "KEY_DIGIT1" => Key::KEY_1,
        "KEY_2" | "KEY_DIGIT2" => Key::KEY_2,
        "KEY_3" | "KEY_DIGIT3" => Key::KEY_3,
        "KEY_4" | "KEY_DIGIT4" => Key::KEY_4,
        "KEY_5" | "KEY_DIGIT5" => Key::KEY_5,
        "KEY_6" | "KEY_DIGIT6" => Key::KEY_6,
        "KEY_7" | "KEY_DIGIT7" => Key::KEY_7,
        "KEY_8" | "KEY_DIGIT8" => Key::KEY_8,
        "KEY_9" | "KEY_DIGIT9" => Key::KEY_9,
        "KEY_F1" => Key::KEY_F1,
        "KEY_F2" => Key::KEY_F2,
        "KEY_F3" => Key::KEY_F3,
        "KEY_F4" => Key::KEY_F4,
        "KEY_F5" => Key::KEY_F5,
        "KEY_F6" => Key::KEY_F6,
        "KEY_F7" => Key::KEY_F7,
        "KEY_F8" => Key::KEY_F8,
        "KEY_F9" => Key::KEY_F9,
        "KEY_F10" => Key::KEY_F10,
        "KEY_F11" => Key::KEY_F11,
        "KEY_F12" => Key::KEY_F12,
        "KEY_MINUS" => Key::KEY_MINUS,
        "KEY_EQUAL" | "KEY_PLUS" => Key::KEY_EQUAL,
        "KEY_COMMA" => Key::KEY_COMMA,
        "KEY_DOT" | "KEY_PERIOD" => Key::KEY_DOT,
        "KEY_SLASH" => Key::KEY_SLASH,
        "KEY_SEMICOLON" => Key::KEY_SEMICOLON,
        "KEY_APOSTROPHE" => Key::KEY_APOSTROPHE,
        "KEY_GRAVE" => Key::KEY_GRAVE,
        "KEY_LEFTBRACE" => Key::KEY_LEFTBRACE,
        "KEY_RIGHTBRACE" => Key::KEY_RIGHTBRACE,
        "KEY_BACKSLASH" => Key::KEY_BACKSLASH,
        "KEY_LEFTMETA" | "KEY_SUPER" => Key::KEY_LEFTMETA,
        "KEY_LEFTCTRL" | "KEY_CTRL" => Key::KEY_LEFTCTRL,
        "KEY_LEFTSHIFT" | "KEY_SHIFT" => Key::KEY_LEFTSHIFT,
        "KEY_LEFTALT" | "KEY_ALT" => Key::KEY_LEFTALT,
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
            map.combo_for(Key::KEY_PREVIOUSSONG).unwrap().0,
            ButtonId::TopLeft
        );
        assert_eq!(
            map.combo_for(Key::KEY_PLAYPAUSE).unwrap().0,
            ButtonId::TopCenter
        );
        assert_eq!(
            map.combo_for(Key::KEY_NEXTSONG).unwrap().0,
            ButtonId::TopRight
        );
        assert_eq!(
            map.combo_for(Key::KEY_V).unwrap().0,
            ButtonId::SideLeftUpper
        );
        assert_eq!(
            map.combo_for(Key::KEY_C).unwrap().0,
            ButtonId::SideLeftLower
        );
        assert_eq!(
            map.combo_for(Key::KEY_Y).unwrap().0,
            ButtonId::SideRightUpper
        );
        assert_eq!(
            map.combo_for(Key::KEY_Z).unwrap().0,
            ButtonId::SideRightLower
        );
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
        assert_eq!(parse_combo("Ctrl+Z"), Some(KeyCombo::ctrl(Key::KEY_Z)));
        assert_eq!(parse_combo("ctrl+y"), Some(KeyCombo::ctrl(Key::KEY_Y)));
        assert_eq!(
            parse_combo("PlayPause"),
            Some(KeyCombo::plain(Key::KEY_PLAYPAUSE))
        );
        assert_eq!(
            parse_combo("KEY_NEXTSONG"),
            Some(KeyCombo::plain(Key::KEY_NEXTSONG))
        );
        assert_eq!(parse_combo("NotAKey"), None);
    }

    #[test]
    fn parse_combos_with_every_modifier() {
        assert_eq!(
            parse_combo("Ctrl+Shift+T"),
            Some(KeyCombo {
                ctrl: true,
                shift: true,
                alt: false,
                logo: false,
                key: Key::KEY_T,
            })
        );
        assert_eq!(
            parse_combo("shift+ctrl+t"),
            Some(KeyCombo {
                ctrl: true,
                shift: true,
                alt: false,
                logo: false,
                key: Key::KEY_T,
            }),
            "el orden y las mayusculas de los modificadores no importan"
        );
        assert_eq!(
            parse_combo("Super+Up"),
            Some(KeyCombo {
                ctrl: false,
                shift: false,
                alt: false,
                logo: true,
                key: Key::KEY_UP,
            })
        );
        assert_eq!(
            parse_combo("Alt+F4"),
            Some(KeyCombo {
                ctrl: false,
                shift: false,
                alt: true,
                logo: false,
                key: Key::KEY_F4,
            })
        );
        assert_eq!(parse_combo("Ctrl+NotAKey"), None);
        assert_eq!(parse_combo("Hyper+T"), None, "modificador desconocido");
        assert_eq!(parse_combo(""), None);
    }

    #[test]
    fn combo_label_round_trips() {
        for spec in [
            "Ctrl+T",
            "Ctrl+Shift+T",
            "Super+Tab",
            "Ctrl+Alt+Left",
            "F11",
            "Shift+F10",
            "Print",
        ] {
            let combo = parse_combo(spec).unwrap_or_else(|| panic!("no parsea: {spec}"));
            assert_eq!(parse_combo(&combo.label()), Some(combo), "{spec}");
        }
        assert_eq!(parse_combo("Ctrl+Shift+T").unwrap().label(), "Ctrl+Shift+T");
        assert_eq!(parse_combo("super+up").unwrap().label(), "Super+Up");
    }

    #[test]
    fn key_names_are_readable() {
        assert_eq!(key_name(Key::KEY_T), "T");
        assert_eq!(key_name(Key::KEY_F11), "F11");
        assert_eq!(key_name(Key::KEY_UP), "Up");
        assert_eq!(key_name(Key::KEY_LEFTMETA), "Super");
        assert_eq!(key_name(Key::KEY_PLAYPAUSE), "PlayPause");
    }

    #[test]
    fn seven_buttons_mapped() {
        let map = KeyMap::default();
        let all = [
            ButtonId::TopLeft,
            ButtonId::TopCenter,
            ButtonId::TopRight,
            ButtonId::SideLeftUpper,
            ButtonId::SideLeftLower,
            ButtonId::SideRightUpper,
            ButtonId::SideRightLower,
        ];
        for id in all {
            assert!(map.buttons.iter().any(|(b, _)| *b == id), "falta {id:?}");
        }
    }
}
