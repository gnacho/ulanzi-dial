//! Traduce eventos crudos de teclado del D100H a gestos semánticos.
//!
//! Máquina de estados:
//!  - La rotación llega como KEY_VOLUMEUP/KEY_VOLUMEDOWN (un par
//!    pulsación/liberación por paso, sin autorrepetición).
//!  - Los tres botones superiores llegan como teclas multimedia sueltas.
//!  - Los cuatro laterales llegan como acordes Ctrl+tecla. El firmware suelta
//!    Ctrl antes que la letra, así que el estado de Ctrl solo se usa para
//!    validar la pulsación y las liberaciones se resuelven por `held_button`.
//!  - La pulsación larga del dial se detecta por tiempo (umbral configurable),
//!    pero el firmware actual no mantiene la tecla pulsada: emite pulsación y
//!    liberación seguidas, así que en la práctica no se dispara.

use evdev::Key;

use super::keymap::{ButtonId, KeyMap};

/// Evento de tecla crudo, desacoplado de evdev para poder testear la lógica.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub key: Key,
    /// 0 = liberación, 1 = pulsación, 2 = autorrepetición
    pub value: i32,
    pub time_ms: u64,
}

/// Gesto semántico producido por el decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialEvent {
    /// La rueda giró. `steps`: +1 horario, -1 antihorario.
    /// `modifier`: botón mantenido mientras se giraba (combo rueda + botón).
    Rotate {
        steps: i32,
        modifier: Option<ButtonId>,
    },
    /// Pulsación corta del dial.
    DialPress,
    /// El dial lleva pulsado más del umbral configurado.
    DialLongPress,
    ButtonPress(ButtonId),
    ButtonRelease(ButtonId),
}

pub struct Decoder {
    map: KeyMap,
    ctrl_held: bool,
    held_button: Option<ButtonId>,
    dial_down_at: Option<u64>,
    long_fired: bool,
    long_press_ms: u64,
}

impl Decoder {
    pub fn new(map: KeyMap, long_press_ms: u64) -> Self {
        Self {
            map,
            ctrl_held: false,
            held_button: None,
            dial_down_at: None,
            long_fired: false,
            long_press_ms,
        }
    }

    pub fn set_long_press_ms(&mut self, ms: u64) {
        self.long_press_ms = ms;
    }

    /// Procesa un evento crudo y devuelve los gestos que genera.
    pub fn feed(&mut self, ev: RawKeyEvent) -> Vec<DialEvent> {
        let mut out = Vec::new();

        match ev.key {
            Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => {
                // No se limpia held_button aquí: el firmware suelta Ctrl antes
                // que la letra, y la liberación se resuelve por tecla.
                self.ctrl_held = ev.value != 0;
                return out;
            }
            _ => {}
        }

        if ev.key == self.map.rotate_cw || ev.key == self.map.rotate_ccw {
            // Pulsación o autorrepetición: ambas cuentan como un paso.
            if ev.value >= 1 {
                let steps = if ev.key == self.map.rotate_cw { 1 } else { -1 };
                out.push(DialEvent::Rotate {
                    steps,
                    modifier: self.held_button,
                });
            }
            return out;
        }

        if ev.key == self.map.dial_press {
            match ev.value {
                1 => {
                    self.dial_down_at = Some(ev.time_ms);
                    self.long_fired = false;
                    out.push(DialEvent::DialPress);
                }
                0 => {
                    self.dial_down_at = None;
                    // Si ya se emitió la pulsación larga, la liberación no genera nada más.
                }
                _ => {}
            }
            return out;
        }

        if let Some((id, combo)) = self.map.combo_for(ev.key) {
            match ev.value {
                1 => {
                    // Los combos con Ctrl solo cuentan si el firmware lo envió
                    // antes; las teclas multimedia sueltas cuentan siempre.
                    if combo.ctrl == self.ctrl_held {
                        self.held_button = Some(id);
                        out.push(DialEvent::ButtonPress(id));
                    }
                }
                0 => {
                    if self.held_button == Some(id) {
                        self.held_button = None;
                        out.push(DialEvent::ButtonRelease(id));
                    }
                }
                _ => {} // autorrepetición de botones: ignorar
            }
        }

        out
    }

    /// Llamar periódicamente (p. ej. cada 50 ms) para detectar la pulsación larga.
    pub fn tick(&mut self, now_ms: u64) -> Option<DialEvent> {
        if let Some(down_at) = self.dial_down_at {
            if !self.long_fired && now_ms.saturating_sub(down_at) >= self.long_press_ms {
                self.long_fired = true;
                return Some(DialEvent::DialLongPress);
            }
        }
        None
    }

    /// Indica si el dial está en pulsación larga (para suprimir la acción corta al soltar, si se desea).
    pub fn long_press_fired(&self) -> bool {
        self.long_fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: Key, value: i32, t: u64) -> RawKeyEvent {
        RawKeyEvent {
            key,
            value,
            time_ms: t,
        }
    }

    fn decoder() -> Decoder {
        Decoder::new(KeyMap::default(), 750)
    }

    #[test]
    fn rotation_both_directions_and_autorepeat() {
        let mut d = decoder();
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 1, 0)),
            vec![DialEvent::Rotate {
                steps: 1,
                modifier: None
            }]
        );
        // autorrepetición también cuenta como paso
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 2, 33)),
            vec![DialEvent::Rotate {
                steps: 1,
                modifier: None
            }]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEDOWN, 1, 66)),
            vec![DialEvent::Rotate {
                steps: -1,
                modifier: None
            }]
        );
        // liberación no genera paso
        assert!(d.feed(ev(Key::KEY_VOLUMEUP, 0, 99)).is_empty());
    }

    #[test]
    fn button_chord_press_and_release() {
        let mut d = decoder();
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 1, 0)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_Z, 1, 10)),
            vec![DialEvent::ButtonPress(ButtonId::SideRightLower)]
        );
        // autorrepetición ignorada
        assert!(d.feed(ev(Key::KEY_Z, 2, 40)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_Z, 0, 100)),
            vec![DialEvent::ButtonRelease(ButtonId::SideRightLower)]
        );
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 0, 110)).is_empty());
    }

    #[test]
    fn release_survives_ctrl_being_let_go_first() {
        // El firmware real suelta Ctrl antes que la letra del combo.
        let mut d = decoder();
        d.feed(ev(Key::KEY_LEFTCTRL, 1, 0));
        assert_eq!(
            d.feed(ev(Key::KEY_V, 1, 10)),
            vec![DialEvent::ButtonPress(ButtonId::SideLeftUpper)]
        );
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 0, 20)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_V, 0, 30)),
            vec![DialEvent::ButtonRelease(ButtonId::SideLeftUpper)]
        );
    }

    #[test]
    fn media_buttons_work_without_ctrl() {
        let mut d = decoder();
        assert_eq!(
            d.feed(ev(Key::KEY_PREVIOUSSONG, 1, 0)),
            vec![DialEvent::ButtonPress(ButtonId::TopLeft)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_PREVIOUSSONG, 0, 10)),
            vec![DialEvent::ButtonRelease(ButtonId::TopLeft)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_PLAYPAUSE, 1, 20)),
            vec![DialEvent::ButtonPress(ButtonId::TopCenter)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_NEXTSONG, 1, 30)),
            vec![DialEvent::ButtonPress(ButtonId::TopRight)]
        );
    }

    #[test]
    fn rotate_with_button_held_carries_modifier() {
        let mut d = decoder();
        d.feed(ev(Key::KEY_LEFTCTRL, 1, 0));
        d.feed(ev(Key::KEY_Y, 1, 10));
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 1, 20)),
            vec![DialEvent::Rotate {
                steps: 1,
                modifier: Some(ButtonId::SideRightUpper)
            }]
        );
        d.feed(ev(Key::KEY_LEFTCTRL, 0, 30));
        d.feed(ev(Key::KEY_Y, 0, 35));
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 1, 50)),
            vec![DialEvent::Rotate {
                steps: 1,
                modifier: None
            }]
        );
    }

    #[test]
    fn dial_press_and_long_press() {
        let mut d = decoder();
        assert_eq!(
            d.feed(ev(Key::KEY_MUTE, 1, 1000)),
            vec![DialEvent::DialPress]
        );
        assert_eq!(d.tick(1400), None); // aún no llega al umbral
        assert_eq!(d.tick(1760), Some(DialEvent::DialLongPress));
        assert_eq!(d.tick(2000), None); // no se repite
        assert!(d.long_press_fired());
        assert!(d.feed(ev(Key::KEY_MUTE, 0, 2100)).is_empty());
    }

    #[test]
    fn short_press_never_fires_long() {
        let mut d = decoder();
        d.feed(ev(Key::KEY_MUTE, 1, 0));
        d.feed(ev(Key::KEY_MUTE, 0, 300));
        assert_eq!(d.tick(5000), None);
    }

    #[test]
    fn ctrl_combo_without_ctrl_is_ignored() {
        // Defensa: si el grab exclusivo falla y llegan teclas sueltas.
        let mut d = decoder();
        assert!(d.feed(ev(Key::KEY_Z, 1, 0)).is_empty());
    }
}
