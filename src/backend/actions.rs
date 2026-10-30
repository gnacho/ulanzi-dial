//! Ejecución de acciones: todo sale por un teclado/ratón virtual uinput.
//!
//! Ventaja: uniforme y sin dependencias de sesión — GNOME/KDE interpretan las
//! teclas multimedia (volumen, brillo, play/pausa…) y los acordes (Ctrl+…) del
//! mismo modo que si vinieran de un teclado físico. Los comandos personalizados
//! se lanzan con el shell del usuario.

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use serde::{Deserialize, Serialize};

use crate::backend::keymap::{all_named_keys, parse_combo, KeyCombo};
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
    /// Atajo que emite la acción cuando es una combinación de teclas.
    /// Lo usan la UI (para enseñarlo) y el emisor (para mandarlo).
    pub fn shortcut(&self) -> Option<KeyCombo> {
        match self {
            Action::Undo => Some(KeyCombo::ctrl(Key::KEY_Z)),
            Action::Redo => Some(KeyCombo::ctrl(Key::KEY_Y)),
            Action::Copy => Some(KeyCombo::ctrl(Key::KEY_C)),
            Action::Paste => Some(KeyCombo::ctrl(Key::KEY_V)),
            Action::PlayPause => Some(KeyCombo::plain(Key::KEY_PLAYPAUSE)),
            Action::Mute => Some(KeyCombo::plain(Key::KEY_MUTE)),
            Action::Screenshot => Some(KeyCombo::plain(Key::KEY_PRINT)),
            Action::Lock => Some(KeyCombo {
                ctrl: false,
                shift: false,
                alt: false,
                logo: true,
                key: Key::KEY_L,
            }),
            Action::Chord(chord) => parse_combo(chord),
            _ => None,
        }
    }

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

    /// Icono simbólico de Adwaita que representa la acción en los selectores.
    pub fn icon(&self) -> &'static str {
        match self {
            Action::None => "action-unavailable-symbolic",
            Action::Volume => "audio-volume-high-symbolic",
            Action::Scroll => "input-mouse-symbolic",
            Action::HScroll => "go-next-symbolic",
            Action::Brightness => "display-brightness-symbolic",
            Action::Zoom => "zoom-in-symbolic",
            Action::Track => "media-skip-forward-symbolic",
            Action::Mute => "audio-volume-muted-symbolic",
            Action::PlayPause => "media-playback-start-symbolic",
            Action::Undo => "edit-undo-symbolic",
            Action::Redo => "edit-redo-symbolic",
            Action::Copy => "edit-copy-symbolic",
            Action::Paste => "edit-paste-symbolic",
            Action::Screenshot => "applets-screenshooter-symbolic",
            Action::Lock => "system-lock-screen-symbolic",
            Action::Command(_) => "utilities-terminal-symbolic",
            Action::Chord(_) => "input-keyboard-symbolic",
        }
    }
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
        // Toda tecla que la app puede emitir sale de la tabla de nombres:
        // acordes configurables y acciones de medios. Declarar un set menor
        // hacía que evdev descartara en silencio las teclas no declaradas.
        for k in all_named_keys() {
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

    /// Pulsa un atajo completo: modificadores, tecla y liberación inversa.
    fn tap_chord(&mut self, combo: &KeyCombo) {
        let modifiers = [
            (combo.ctrl, Key::KEY_LEFTCTRL),
            (combo.shift, Key::KEY_LEFTSHIFT),
            (combo.alt, Key::KEY_LEFTALT),
            (combo.logo, Key::KEY_LEFTMETA),
        ];
        let mut evs = Vec::new();
        for (held, key) in modifiers {
            if held {
                evs.push((EventType::KEY, key.0, 1));
            }
        }
        evs.push((EventType::KEY, combo.key.0, 1));
        evs.push((EventType::KEY, combo.key.0, 0));
        for (held, key) in modifiers.iter().rev() {
            if *held {
                evs.push((EventType::KEY, key.0, 0));
            }
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
                    self.tap_chord(&KeyCombo::ctrl(key));
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
            Action::Undo => self.tap_chord(&KeyCombo::ctrl(Key::KEY_Z)),
            Action::Redo => self.tap_chord(&KeyCombo::ctrl(Key::KEY_Y)),
            Action::Copy => self.tap_chord(&KeyCombo::ctrl(Key::KEY_C)),
            Action::Paste => self.tap_chord(&KeyCombo::ctrl(Key::KEY_V)),
            Action::Screenshot => self.tap_key(Key::KEY_SYSRQ),
            Action::Lock => run_command("loginctl lock-session"),
            Action::Command(cmd) => run_command(cmd),
            Action::Chord(chord) => {
                if let Some(combo) = super::keymap::parse_combo(chord) {
                    self.tap_chord(&combo);
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

static PRESS_OPTIONS: [(Action, &str); 10] = [
    (Action::Undo, "Deshacer"),
    (Action::Redo, "Rehacer"),
    (Action::Copy, "Copiar"),
    (Action::Paste, "Pegar"),
    (Action::PlayPause, "Play / Pausa"),
    (Action::Mute, "Silenciar"),
    (Action::Screenshot, "Captura"),
    (Action::Lock, "Bloquear"),
    // Fila centinela: cualquier Action::Command comparte esta entrada en la
    // UI; el comando concreto se pide con un diálogo y nunca se guarda vacío.
    (Action::Command(String::new()), "Ejecutar comando…"),
    (Action::None, "Nada"),
];

pub fn press_options() -> &'static [(Action, &'static str)] {
    &PRESS_OPTIONS
}

/// Índice de una acción dentro de una lista de opciones (fallback: "Nada").
pub fn index_of(options: &[(Action, &'static str)], action: &Action) -> u32 {
    options
        .iter()
        .position(|(a, _)| a == action)
        .or_else(|| match action {
            // Un comando guardado comparte la fila centinela del desplegable.
            Action::Command(_) => options
                .iter()
                .position(|(a, _)| matches!(a, Action::Command(c) if c.is_empty())),
            _ => None,
        })
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
    fn command_row_is_shared_by_saved_commands() {
        let options = press_options();
        let index = index_of(options, &Action::Command("notify-send hola".into()));
        assert_eq!(options[index as usize].1, "Ejecutar comando…");
        assert_eq!(
            index_of(options, &Action::None),
            (options.len() - 1) as u32,
            "las acciones desconocidas caen en Nada"
        );
    }
}
