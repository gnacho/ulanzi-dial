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

    /// Botón asociado a una tecla sin mirar el estado de Ctrl.
    pub fn button_for(&self, key: Key) -> Option<ButtonId> {
        self.combo_for(key).map(|(id, _)| id)
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
/// Todas las teclas que la app puede emitir, con alias y sin el prefijo
/// KEY_ (acepta tanto "F11" como "KEY_F11"). Es la única fuente de teclas:
/// de aquí sale tanto el parsing del config como el set declarado en el
/// dispositivo virtual uinput.
static KEY_NAMES: &[(&str, Key)] = &[
    ("A", Key::KEY_A),
    ("B", Key::KEY_B),
    ("C", Key::KEY_C),
    ("D", Key::KEY_D),
    ("E", Key::KEY_E),
    ("F", Key::KEY_F),
    ("G", Key::KEY_G),
    ("H", Key::KEY_H),
    ("I", Key::KEY_I),
    ("J", Key::KEY_J),
    ("K", Key::KEY_K),
    ("L", Key::KEY_L),
    ("M", Key::KEY_M),
    ("N", Key::KEY_N),
    ("O", Key::KEY_O),
    ("P", Key::KEY_P),
    ("Q", Key::KEY_Q),
    ("R", Key::KEY_R),
    ("S", Key::KEY_S),
    ("T", Key::KEY_T),
    ("U", Key::KEY_U),
    ("V", Key::KEY_V),
    ("W", Key::KEY_W),
    ("X", Key::KEY_X),
    ("Y", Key::KEY_Y),
    ("Z", Key::KEY_Z),
    ("PLAYPAUSE", Key::KEY_PLAYPAUSE),
    ("NEXTSONG", Key::KEY_NEXTSONG),
    ("PREVIOUSSONG", Key::KEY_PREVIOUSSONG),
    ("MUTE", Key::KEY_MUTE),
    ("VOLUMEUP", Key::KEY_VOLUMEUP),
    ("VOLUMEDOWN", Key::KEY_VOLUMEDOWN),
    ("BRIGHTNESSUP", Key::KEY_BRIGHTNESSUP),
    ("BRIGHTNESSDOWN", Key::KEY_BRIGHTNESSDOWN),
    ("TAB", Key::KEY_TAB),
    ("ENTER", Key::KEY_ENTER),
    ("ESC", Key::KEY_ESC),
    ("ESCAPE", Key::KEY_ESC),
    ("SPACE", Key::KEY_SPACE),
    ("BACKSPACE", Key::KEY_BACKSPACE),
    ("DELETE", Key::KEY_DELETE),
    ("INSERT", Key::KEY_INSERT),
    ("HOME", Key::KEY_HOME),
    ("END", Key::KEY_END),
    ("PAGEUP", Key::KEY_PAGEUP),
    ("PAGEDOWN", Key::KEY_PAGEDOWN),
    ("LEFT", Key::KEY_LEFT),
    ("RIGHT", Key::KEY_RIGHT),
    ("UP", Key::KEY_UP),
    ("DOWN", Key::KEY_DOWN),
    ("PRINT", Key::KEY_SYSRQ),
    ("SYSRQ", Key::KEY_SYSRQ),
    ("MENU", Key::KEY_MENU),
    ("0", Key::KEY_0),
    ("1", Key::KEY_1),
    ("2", Key::KEY_2),
    ("3", Key::KEY_3),
    ("4", Key::KEY_4),
    ("5", Key::KEY_5),
    ("6", Key::KEY_6),
    ("7", Key::KEY_7),
    ("8", Key::KEY_8),
    ("9", Key::KEY_9),
    ("DIGIT0", Key::KEY_0),
    ("DIGIT1", Key::KEY_1),
    ("DIGIT2", Key::KEY_2),
    ("DIGIT3", Key::KEY_3),
    ("DIGIT4", Key::KEY_4),
    ("DIGIT5", Key::KEY_5),
    ("DIGIT6", Key::KEY_6),
    ("DIGIT7", Key::KEY_7),
    ("DIGIT8", Key::KEY_8),
    ("DIGIT9", Key::KEY_9),
    ("F1", Key::KEY_F1),
    ("F2", Key::KEY_F2),
    ("F3", Key::KEY_F3),
    ("F4", Key::KEY_F4),
    ("F5", Key::KEY_F5),
    ("F6", Key::KEY_F6),
    ("F7", Key::KEY_F7),
    ("F8", Key::KEY_F8),
    ("F9", Key::KEY_F9),
    ("F10", Key::KEY_F10),
    ("F11", Key::KEY_F11),
    ("F12", Key::KEY_F12),
    ("MINUS", Key::KEY_MINUS),
    ("EQUAL", Key::KEY_EQUAL),
    ("PLUS", Key::KEY_EQUAL),
    ("COMMA", Key::KEY_COMMA),
    ("DOT", Key::KEY_DOT),
    ("PERIOD", Key::KEY_DOT),
    ("SLASH", Key::KEY_SLASH),
    ("SEMICOLON", Key::KEY_SEMICOLON),
    ("APOSTROPHE", Key::KEY_APOSTROPHE),
    ("GRAVE", Key::KEY_GRAVE),
    ("LEFTBRACE", Key::KEY_LEFTBRACE),
    ("RIGHTBRACE", Key::KEY_RIGHTBRACE),
    ("BACKSLASH", Key::KEY_BACKSLASH),
    ("LEFTMETA", Key::KEY_LEFTMETA),
    ("SUPER", Key::KEY_LEFTMETA),
    ("LEFTCTRL", Key::KEY_LEFTCTRL),
    ("CTRL", Key::KEY_LEFTCTRL),
    ("LEFTSHIFT", Key::KEY_LEFTSHIFT),
    ("SHIFT", Key::KEY_LEFTSHIFT),
    ("LEFTALT", Key::KEY_LEFTALT),
    ("ALT", Key::KEY_LEFTALT),
];

pub fn named_key(name: &str) -> Option<Key> {
    let upper = name.to_ascii_uppercase();
    let bare = upper.strip_prefix("KEY_").unwrap_or(&upper);
    KEY_NAMES
        .iter()
        .find(|(candidate, _)| *candidate == bare)
        .map(|(_, key)| *key)
}

/// Teclas distintas de la tabla, para declarar en el dispositivo virtual
/// todas las que la app puede llegar a emitir (atajos y acciones de medios).
pub fn all_named_keys() -> impl Iterator<Item = Key> {
    KEY_NAMES.iter().map(|(_, key)| *key)
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
        for id in ButtonId::ALL {
            assert!(map.buttons.iter().any(|(b, _)| *b == id), "falta {id:?}");
        }
    }

    #[test]
    fn named_key_accepts_prefix_and_aliases() {
        assert_eq!(named_key("f11"), named_key("KEY_F11"));
        assert_eq!(named_key("PRINT"), named_key("SYSRQ"));
        assert_eq!(named_key("DIGIT3"), named_key("3"));
        assert_eq!(named_key("PLUS"), named_key("EQUAL"));
        assert_eq!(named_key("SUPER"), Some(Key::KEY_LEFTMETA));
        assert!(named_key("no-existe").is_none());
    }

    #[test]
    fn table_covers_the_keys_a_chord_can_ask_for() {
        let declared: std::collections::HashSet<_> = all_named_keys().collect();
        for name in [
            "LEFTMETA",
            "F11",
            "F12",
            "1",
            "UP",
            "TAB",
            "N",
            "W",
            "BRIGHTNESSUP",
        ] {
            let key = named_key(name).unwrap_or_else(|| panic!("falta {name} en la tabla"));
            assert!(declared.contains(&key), "{name} no queda declarada");
        }
        for modifier in ["LEFTCTRL", "LEFTSHIFT", "LEFTALT"] {
            let key = named_key(modifier).unwrap();
            assert!(declared.contains(&key));
        }
    }

    #[test]
    fn key_names_have_no_duplicates() {
        let mut names: Vec<_> = KEY_NAMES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "nombres duplicados en KEY_NAMES");
    }
}
