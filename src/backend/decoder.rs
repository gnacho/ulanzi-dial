//! Traduce eventos crudos de teclado del D100H a gestos semánticos.
//!
//! Máquina de estados:
//!  - La rotación llega como KEY_VOLUMEUP/KEY_VOLUMEDOWN (un par
//!    pulsación/liberación por paso, sin autorrepetición).
//!  - Los tres botones superiores llegan como teclas multimedia sueltas.
//!  - Los cuatro laterales llegan como acordes Ctrl+tecla. El firmware suelta
//!    Ctrl antes que la letra, así que el estado de Ctrl solo se usa para
//!    validar la pulsación y las liberaciones se resuelven por `held_button`.
//!
//! Lo que el aparato no permite (medido con `EVIOCGRAB`, no deducido):
//!  - Mientras un botón está pulsado el firmware no emite giros, así que no
//!    existen los combos rueda+botón.
//!  - La pulsación de la rueda no informa de mantención: una pulsación de dos
//!    segundos llega como pulsación y liberación con 10 ms de diferencia, así
//!    que no hay pulsación larga.
//!
//! Por eso cada control tiene una sola acción y el decoder no acumula estado
//! de tiempo.

use evdev::Key;

use super::keymap::{ButtonId, KeyMap};

/// Evento de tecla crudo, desacoplado de evdev para poder testear la lógica.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub key: Key,
    /// 0 = liberación, 1 = pulsación, 2 = autorrepetición
    pub value: i32,
    /// Marca de tiempo del núcleo, en milisegundos.
    pub time_ms: u64,
}

/// Gesto semántico producido por el decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialEvent {
    /// La rueda giró. `steps`: +1 horario, -1 antihorario.
    Rotate {
        steps: i32,
    },
    /// Pulsación del dial.
    DialPress,
    ButtonPress(ButtonId),
    ButtonRelease(ButtonId),
}

pub struct Decoder {
    map: KeyMap,
    ctrl_held: bool,
    held_button: Option<ButtonId>,
}

impl Decoder {
    pub fn new(map: KeyMap) -> Self {
        Self {
            map,
            ctrl_held: false,
            held_button: None,
        }
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
                out.push(DialEvent::Rotate { steps });
            }
            return out;
        }

        if ev.key == self.map.dial_press {
            if ev.value == 1 {
                out.push(DialEvent::DialPress);
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
                0 if self.held_button == Some(id) => {
                    self.held_button = None;
                    out.push(DialEvent::ButtonRelease(id));
                }
                _ => {} // autorrepetición de botones: ignorar
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: Key, value: i32) -> RawKeyEvent {
        RawKeyEvent {
            key,
            value,
            time_ms: 0,
        }
    }

    fn decoder() -> Decoder {
        Decoder::new(KeyMap::default())
    }

    #[test]
    fn rotation_both_directions_and_autorepeat() {
        let mut d = decoder();
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 1)),
            vec![DialEvent::Rotate { steps: 1 }]
        );
        // autorrepetición también cuenta como paso
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEUP, 2)),
            vec![DialEvent::Rotate { steps: 1 }]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_VOLUMEDOWN, 1)),
            vec![DialEvent::Rotate { steps: -1 }]
        );
        // liberación no genera paso
        assert!(d.feed(ev(Key::KEY_VOLUMEUP, 0)).is_empty());
    }

    #[test]
    fn button_chord_press_and_release() {
        let mut d = decoder();
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 1)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_Z, 1)),
            vec![DialEvent::ButtonPress(ButtonId::SideRightLower)]
        );
        // autorrepetición ignorada
        assert!(d.feed(ev(Key::KEY_Z, 2)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_Z, 0)),
            vec![DialEvent::ButtonRelease(ButtonId::SideRightLower)]
        );
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 0)).is_empty());
    }

    #[test]
    fn release_survives_ctrl_being_let_go_first() {
        // El firmware real suelta Ctrl antes que la letra del combo.
        let mut d = decoder();
        d.feed(ev(Key::KEY_LEFTCTRL, 1));
        assert_eq!(
            d.feed(ev(Key::KEY_V, 1)),
            vec![DialEvent::ButtonPress(ButtonId::SideLeftUpper)]
        );
        assert!(d.feed(ev(Key::KEY_LEFTCTRL, 0)).is_empty());
        assert_eq!(
            d.feed(ev(Key::KEY_V, 0)),
            vec![DialEvent::ButtonRelease(ButtonId::SideLeftUpper)]
        );
    }

    #[test]
    fn media_buttons_work_without_ctrl() {
        let mut d = decoder();
        assert_eq!(
            d.feed(ev(Key::KEY_PREVIOUSSONG, 1)),
            vec![DialEvent::ButtonPress(ButtonId::TopLeft)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_PREVIOUSSONG, 0)),
            vec![DialEvent::ButtonRelease(ButtonId::TopLeft)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_PLAYPAUSE, 1)),
            vec![DialEvent::ButtonPress(ButtonId::TopCenter)]
        );
        assert_eq!(
            d.feed(ev(Key::KEY_NEXTSONG, 1)),
            vec![DialEvent::ButtonPress(ButtonId::TopRight)]
        );
    }

    #[test]
    fn dial_press_is_a_single_event() {
        // El firmware no informa de mantención: pulsación y liberación seguidas.
        let mut d = decoder();
        assert_eq!(d.feed(ev(Key::KEY_MUTE, 1)), vec![DialEvent::DialPress]);
        assert!(d.feed(ev(Key::KEY_MUTE, 0)).is_empty());
        assert_eq!(d.feed(ev(Key::KEY_MUTE, 1)), vec![DialEvent::DialPress]);
    }

    #[test]
    fn ctrl_combo_without_ctrl_is_ignored() {
        // Defensa: si el grab exclusivo falla y llegan teclas sueltas.
        let mut d = decoder();
        assert!(d.feed(ev(Key::KEY_Z, 1)).is_empty());
    }
}
