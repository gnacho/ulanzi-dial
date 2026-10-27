//! Ejecución de acciones: todo sale por un teclado/ratón virtual uinput.
//!
//! Ventaja: uniforme y sin dependencias de sesión — GNOME/KDE interpretan las
//! teclas multimedia (volumen, brillo, play/pausa…) y los acordes (Ctrl+…) del
//! mismo modo que si vinieran de un teclado físico. Los comandos personalizados
//! se lanzan con el shell del usuario.

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use serde::{Deserialize, Serialize};

use crate::config::VolumeMode;

/// Acción asignable a un gesto. Se serializa en kebab-case en el config.toml.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    None,
    // Acciones de rueda
    Volume,
    Scroll,
    HScroll,
    Brightness,
    Zoom,
    Track,
    // Acciones de pulsación
    Mute,
    PlayPause,
    Undo,
    Redo,
    Copy,
    Paste,
    Screenshot,
    Lock,
    /// Comando de shell arbitrario, p. ej. "flameshot gui".
    Command(String),
    /// Acorde de teclas, p. ej. "Ctrl+Shift+T".
    Chord(String),
}

impl Action {
    /// Opciones mostradas en la UI para gestos de rueda.
    pub const SPIN_CHOICES: &'static [&'static str] = &[
        "volume",
        "scroll",
        "hscroll",
        "brightness",
        "zoom",
        "track",
        "none",
    ];
    /// Opciones para pulsaciones de botones.
    pub const PRESS_CHOICES: &'static [&'static str] = &[
        "undo",
        "redo",
        "copy",
        "paste",
        "playpause",
        "mute",
        "screenshot",
        "lock",
        "none",
    ];

    pub fn label(&self) -> String {
        match self {
            Action::None => "Nada".into(),
            Action::Volume => "Volumen".into(),
            Action::Scroll => "Desplazamiento".into(),
            Action::HScroll => "Horizontal".into(),
            Action::Brightness => "Brillo".into(),
            Action::Zoom => "Zoom".into(),
            Action::Track => "Cambiar pista".into(),
            Action::Mute => "Silenciar".into(),
            Action::PlayPause => "Play / Pausa".into(),
            Action::Undo => "Deshacer".into(),
            Action::Redo => "Rehacer".into(),
            Action::Copy => "Copiar".into(),
            Action::Paste => "Pegar".into(),
            Action::Screenshot => "Captura".into(),
            Action::Lock => "Bloquear".into(),
            Action::Command(c) => format!("Ejecutar: {c}"),
            Action::Chord(c) => format!("Atajo: {c}"),
        }
    }
}

/// Parsea el valor de texto guardado en el config (kebab-case o con payload).
pub fn action_from_str(s: &str) -> Action {
    if let Some(cmd) = s.strip_prefix("command:") {
        return Action::Command(cmd.to_string());
    }
    if let Some(chord) = s.strip_prefix("chord:") {
        return Action::Chord(chord.to_string());
    }
    serde_plain(s).unwrap_or(Action::None)
}

fn serde_plain(s: &str) -> Option<Action> {
    // Serialización kebab-case de las variantes sin payload.
    Some(match s {
        "volume" => Action::Volume,
        "scroll" => Action::Scroll,
        "hscroll" => Action::HScroll,
        "brightness" => Action::Brightness,
        "zoom" => Action::Zoom,
        "track" => Action::Track,
        "mute" => Action::Mute,
        "playpause" => Action::PlayPause,
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "copy" => Action::Copy,
        "paste" => Action::Paste,
        "screenshot" => Action::Screenshot,
        "lock" => Action::Lock,
        "none" => Action::None,
        _ => return None,
    })
}

/// Mezclador disponible para mover el volumen en pasos finos.
///
/// Las teclas multimedia del escritorio dan un salto fijo (6% en GNOME), así
/// que para pasos más pequeños se habla directamente con PipeWire o PulseAudio.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mixer {
    Wpctl,
    Pactl,
    None,
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

fn detect_mixer() -> Mixer {
    if command_exists("wpctl") {
        Mixer::Wpctl
    } else if command_exists("pactl") {
        Mixer::Pactl
    } else {
        Mixer::None
    }
}

/// `wpctl get-volume` imprime "Volume: 0.42" (y a veces " [MUTED]").
fn parse_wpctl_volume(out: &str) -> Option<u8> {
    let raw = out.split_whitespace().nth(1)?;
    let value: f64 = raw.parse().ok()?;
    Some((value * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// `pactl get-sink-volume` imprime "Volume: front-left: 26214 /  40% / …".
fn parse_pactl_volume(out: &str) -> Option<u8> {
    let after = out.split("front-left:").nth(1)?;
    let pct = after.split('/').nth(1)?.trim().trim_end_matches('%');
    let value: u8 = pct.parse().ok()?;
    Some(value.min(150))
}

/// Teclado + rueda virtual por donde salen las acciones.
pub struct ActionExecutor {
    dev: VirtualDevice,
    mixer: Mixer,
}

impl ActionExecutor {
    pub fn new() -> std::io::Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for k in [
            Key::KEY_VOLUMEUP,
            Key::KEY_VOLUMEDOWN,
            Key::KEY_MUTE,
            Key::KEY_PLAYPAUSE,
            Key::KEY_NEXTSONG,
            Key::KEY_PREVIOUSSONG,
            Key::KEY_BRIGHTNESSUP,
            Key::KEY_BRIGHTNESSDOWN,
            Key::KEY_LEFTCTRL,
            Key::KEY_LEFTSHIFT,
            Key::KEY_LEFTALT,
            Key::KEY_MINUS,
            Key::KEY_EQUAL,
            Key::KEY_Z,
            Key::KEY_Y,
            Key::KEY_C,
            Key::KEY_V,
            Key::KEY_T,
            Key::KEY_SYSRQ,
        ] {
            keys.insert(k);
        }
        let mut rel = AttributeSet::<RelativeAxisType>::new();
        rel.insert(RelativeAxisType::REL_WHEEL);
        rel.insert(RelativeAxisType::REL_HWHEEL);

        let dev = evdev::uinput::VirtualDeviceBuilder::new()?
            .name("ulanzi-dial-virtual")
            .with_keys(&keys)?
            .with_relative_axes(&rel)?
            .build()?;
        Ok(Self {
            dev,
            mixer: detect_mixer(),
        })
    }

    fn emit(&mut self, events: &[(EventType, u16, i32)]) {
        let evs: Vec<InputEvent> = events
            .iter()
            .map(|(t, c, v)| InputEvent::new(*t, *c, *v))
            .collect();
        if let Err(e) = self.dev.emit(&evs) {
            tracing::warn!("uinput emit falló: {e}");
        }
    }

    fn tap_key(&mut self, key: Key) {
        self.emit(&[(EventType::KEY, key.0, 1), (EventType::KEY, key.0, 0)]);
    }

    fn tap_chord(&mut self, ctrl: bool, key: Key) {
        let mut evs = Vec::new();
        if ctrl {
            evs.push((EventType::KEY, Key::KEY_LEFTCTRL.0, 1));
        }
        evs.push((EventType::KEY, key.0, 1));
        evs.push((EventType::KEY, key.0, 0));
        if ctrl {
            evs.push((EventType::KEY, Key::KEY_LEFTCTRL.0, 0));
        }
        self.emit(&evs);
    }

    /// Acción de rueda. `steps` es el número de muescas a aplicar (la
    /// sensibilidad ya viene multiplicada por la UI solo en las acciones
    /// continuas). `volume_step` es el salto en tanto por ciento por muesca.
    ///
    /// Devuelve el volumen real resultante (0-100) cuando la acción es
    /// `Volume` y hay mezclador, para que el HUD muestre la verdad.
    pub fn rotate(
        &mut self,
        action: &Action,
        steps: i32,
        volume_mode: VolumeMode,
        volume_step: u8,
    ) -> Option<u8> {
        let dir = steps.signum();
        if dir == 0 {
            return None;
        }
        match action {
            Action::Volume => {
                return match volume_mode {
                    // Teclas multimedia: el escritorio aplica su propio salto y
                    // pinta su OSD nativo. Una pulsación por muesca.
                    VolumeMode::Keys => {
                        let key = if dir > 0 {
                            Key::KEY_VOLUMEUP
                        } else {
                            Key::KEY_VOLUMEDOWN
                        };
                        for _ in 0..steps.abs() {
                            self.tap_key(key);
                        }
                        None
                    }
                    VolumeMode::Mixer => self.change_volume(steps * volume_step as i32),
                };
            }
            // Brillo y pista van de uno en uno: multiplicarlos por la
            // sensibilidad convertía cada muesca en un salto enorme.
            Action::Brightness => {
                self.tap_key(if dir > 0 {
                    Key::KEY_BRIGHTNESSUP
                } else {
                    Key::KEY_BRIGHTNESSDOWN
                });
            }
            Action::Track => {
                self.tap_key(if dir > 0 {
                    Key::KEY_NEXTSONG
                } else {
                    Key::KEY_PREVIOUSSONG
                });
            }
            // Acciones continuas: la sensibilidad sí multiplica muescas.
            Action::Zoom => {
                let key = if dir > 0 {
                    Key::KEY_EQUAL
                } else {
                    Key::KEY_MINUS
                };
                for _ in 0..steps.abs() {
                    self.tap_chord(true, key);
                }
            }
            Action::Scroll => {
                for _ in 0..steps.abs() {
                    self.emit(&[(EventType::RELATIVE, RelativeAxisType::REL_WHEEL.0, dir)]);
                }
            }
            Action::HScroll => {
                for _ in 0..steps.abs() {
                    self.emit(&[(EventType::RELATIVE, RelativeAxisType::REL_HWHEEL.0, dir)]);
                }
            }
            _ => {}
        }
        None
    }

    /// Sube o baja el volumen `delta` puntos porcentuales.
    fn change_volume(&mut self, delta: i32) -> Option<u8> {
        if delta == 0 {
            return None;
        }
        match self.mixer {
            Mixer::Wpctl => {
                let arg = if delta > 0 {
                    format!("{delta}%+")
                } else {
                    format!("{}%-", -delta)
                };
                let ok = std::process::Command::new("wpctl")
                    .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &arg])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok {
                    tracing::warn!("wpctl set-volume falló; usando teclas");
                    self.tap_volume_keys(delta);
                    return None;
                }
                self.read_volume()
            }
            Mixer::Pactl => {
                let arg = if delta > 0 {
                    format!("+{delta}%")
                } else {
                    format!("{delta}%")
                };
                let ok = std::process::Command::new("pactl")
                    .args(["set-sink-volume", "@DEFAULT_SINK@", &arg])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok {
                    tracing::warn!("pactl set-sink-volume falló; usando teclas");
                    self.tap_volume_keys(delta);
                    return None;
                }
                self.read_volume()
            }
            Mixer::None => {
                self.tap_volume_keys(delta);
                None
            }
        }
    }

    fn tap_volume_keys(&mut self, delta: i32) {
        let key = if delta > 0 {
            Key::KEY_VOLUMEUP
        } else {
            Key::KEY_VOLUMEDOWN
        };
        for _ in 0..delta.abs() {
            self.tap_key(key);
        }
    }

    fn read_volume(&self) -> Option<u8> {
        let (program, args): (&str, [&str; 2]) = match self.mixer {
            Mixer::Wpctl | Mixer::None => ("wpctl", ["get-volume", "@DEFAULT_AUDIO_SINK@"]),
            Mixer::Pactl => ("pactl", ["get-sink-volume", "@DEFAULT_SINK@"]),
        };
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        match self.mixer {
            Mixer::Pactl => parse_pactl_volume(&text),
            _ => parse_wpctl_volume(&text),
        }
    }

    /// Acción de pulsación (corta o larga, o botón).
    pub fn press(&mut self, action: &Action) {
        match action {
            Action::Mute => self.tap_key(Key::KEY_MUTE),
            Action::PlayPause => self.tap_key(Key::KEY_PLAYPAUSE),
            Action::Undo => self.tap_chord(true, Key::KEY_Z),
            Action::Redo => self.tap_chord(true, Key::KEY_Y),
            Action::Copy => self.tap_chord(true, Key::KEY_C),
            Action::Paste => self.tap_chord(true, Key::KEY_V),
            Action::Screenshot => self.tap_key(Key::KEY_SYSRQ),
            Action::Lock => run_command("loginctl lock-session"),
            Action::Command(cmd) => run_command(cmd),
            Action::Chord(chord) => {
                if let Some(combo) = super::keymap::parse_combo(chord) {
                    self.tap_chord(combo.ctrl, combo.key);
                }
            }
            _ => {}
        }
    }
}

fn run_command(cmd: &str) {
    if let Err(e) = std::process::Command::new("sh").arg("-c").arg(cmd).spawn() {
        tracing::warn!("no se pudo lanzar '{cmd}': {e}");
    }
}

/// Opciones de los desplegables de la UI: (acción, etiqueta).
/// Los `Command`/`Chord` personalizados solo se editan en el config.toml.
pub fn rotate_options() -> &'static [(Action, &'static str)] {
    &[
        (Action::Volume, "Volumen"),
        (Action::Scroll, "Desplazamiento"),
        (Action::HScroll, "Horizontal"),
        (Action::Brightness, "Brillo"),
        (Action::Zoom, "Zoom"),
        (Action::Track, "Cambiar pista"),
        (Action::None, "Nada"),
    ]
}

pub fn press_options() -> &'static [(Action, &'static str)] {
    &[
        (Action::Undo, "Deshacer"),
        (Action::Redo, "Rehacer"),
        (Action::Copy, "Copiar"),
        (Action::Paste, "Pegar"),
        (Action::PlayPause, "Play / Pausa"),
        (Action::Mute, "Silenciar"),
        (Action::Screenshot, "Captura"),
        (Action::Lock, "Bloquear"),
        (Action::None, "Nada"),
    ]
}

/// Índice de una acción dentro de una lista de opciones (fallback: "Nada").
pub fn index_of(options: &[(Action, &'static str)], action: &Action) -> u32 {
    options
        .iter()
        .position(|(a, _)| a == action)
        .unwrap_or(options.len() - 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_volume() {
        assert_eq!(parse_wpctl_volume("Volume: 0.42\n"), Some(42));
        assert_eq!(parse_wpctl_volume("Volume: 1.00 [MUTED]\n"), Some(100));
        assert_eq!(parse_wpctl_volume("sin datos"), None);
    }

    #[test]
    fn parses_pactl_volume() {
        let out = "Volume: front-left: 26214 /  40% / -23.88 dB,   front-right: 26214 /  40% / -23.88 dB\n";
        assert_eq!(parse_pactl_volume(out), Some(40));
        assert_eq!(parse_pactl_volume("sin datos"), None);
    }

    #[test]
    fn labels_are_user_facing_spanish() {
        assert_eq!(Action::Volume.label(), "Volumen");
        assert_eq!(Action::None.label(), "Nada");
        assert_eq!(
            Action::Command("flameshot gui".into()).label(),
            "Ejecutar: flameshot gui"
        );
    }

    #[test]
    fn roundtrip_plain_and_payload_actions() {
        assert_eq!(action_from_str("volume"), Action::Volume);
        assert_eq!(
            action_from_str("command:flameshot gui"),
            Action::Command("flameshot gui".into())
        );
        assert_eq!(
            action_from_str("chord:Ctrl+Shift+T"),
            Action::Chord("Ctrl+Shift+T".into())
        );
        assert_eq!(action_from_str("bogus"), Action::None);
    }

    #[test]
    fn choices_cover_the_mockup_options() {
        // Las cadenas deben mapear 1:1 con las etiquetas del mockup.
        assert_eq!(Action::SPIN_CHOICES.len(), 7);
        assert!(Action::PRESS_CHOICES.contains(&"playpause"));
    }
}
