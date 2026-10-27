//! Mapa del dispositivo estilo OpenLogi: foto frontal + vistas laterales con
//! líneas y puntos que apuntan a cada control, y una tarjeta por control con
//! sus desplegables de acción (Pulsar / Girar).
//!
//! Geometría fija 640×456 (la ventana no es redimensionable en v1).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use crate::backend::actions::{self, Action};
use crate::backend::keymap::ButtonId;
use crate::config::Config;

const DEVICE_PNG: &[u8] = include_bytes!("../../assets/device.png");
const SIDE_LEFT_PNG: &[u8] = include_bytes!("../../assets/side_left.png");
const SIDE_RIGHT_PNG: &[u8] = include_bytes!("../../assets/side_right.png");

/// Qué tarjeta resaltar cuando llega un evento real del dial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Highlight {
    Button(ButtonId),
    DialRotate,
    DialPress,
    DialLong,
}

// Líneas (x1,y1 → x2,y2) y puntos (x,y), del mockup validado.
const LINES: &[(f64, f64, f64, f64)] = &[
    (103.0, 72.0, 245.0, 137.0),
    (320.0, 72.0, 312.0, 137.0),
    (537.0, 72.0, 388.0, 137.0),
    (93.0, 250.0, 79.0, 162.0),
    (93.0, 322.0, 79.0, 191.0),
    (547.0, 250.0, 561.0, 162.0),
    (547.0, 322.0, 561.0, 191.0),
    (108.0, 396.0, 288.0, 215.0),
    (324.0, 396.0, 307.0, 261.0),
    (540.0, 396.0, 327.0, 222.0),
];
const DOTS: &[(f64, f64)] = &[
    (245.0, 137.0),
    (312.0, 137.0),
    (388.0, 137.0),
    (79.0, 162.0),
    (79.0, 191.0),
    (561.0, 162.0),
    (561.0, 191.0),
    (288.0, 215.0),
    (307.0, 261.0),
    (327.0, 222.0),
];

struct CardHandles {
    container: gtk::Box,
}

pub struct ControlMap {
    pub widget: gtk::Fixed,
    cards: HashMap<Highlight, CardHandles>,
}

fn texture(bytes: &'static [u8]) -> gtk::gdk::Texture {
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_static(bytes))
        .expect("PNG de asset empaquetado")
}

fn picture(bytes: &'static [u8], x: f64, y: f64, w: i32, h: i32, fixed: &gtk::Fixed) {
    let pic = gtk::Picture::for_paintable(&texture(bytes));
    pic.set_can_target(false);
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_size_request(w, h);
    // Marco blanco tipo "foto de producto"
    pic.add_css_class("dev-photo");
    fixed.put(&pic, x, y);
}

/// Crea un desplegable plano con las opciones dadas y el callback de cambio.
fn dropdown(
    options: &'static [(Action, &'static str)],
    current: &Action,
    on_change: impl Fn(Action) + 'static,
) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
    let dd = gtk::DropDown::from_strings(&labels);
    dd.set_selected(actions::index_of(options, current));
    dd.add_css_class("flat");
    dd.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        if let Some((action, _)) = options.get(idx) {
            on_change(action.clone());
        }
    });
    dd
}

fn control_card(
    fixed: &gtk::Fixed,
    x: f64,
    y: f64,
    w: i32,
    name: &str,
    rows: &[(&str, gtk::DropDown)],
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
    card.add_css_class("ctl-card");
    card.set_size_request(w, -1);

    let title = gtk::Label::new(Some(name));
    title.add_css_class("ctl-name");
    title.set_xalign(0.0);
    card.append(&title);

    for (mode, dd) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let chip = gtk::Label::new(Some(mode));
        chip.add_css_class("ctl-mode");
        row.append(&chip);
        dd.set_hexpand(true);
        row.append(dd);
        card.append(&row);
    }

    fixed.put(&card, x, y);
    card
}

impl ControlMap {
    pub fn new(config: Rc<RefCell<Config>>, on_toast: impl Fn(String) + 'static) -> Self {
        let fixed = gtk::Fixed::new();
        fixed.set_size_request(640, 456);
        fixed.set_halign(gtk::Align::Center);
        fixed.set_margin_top(6);

        // 1) líneas debajo de las fotos
        let lines = gtk::DrawingArea::new();
        lines.set_size_request(640, 456);
        lines.set_can_target(false);
        lines.set_draw_func(|_, cr, _, _| {
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.55);
            cr.set_line_width(1.1);
            for (x1, y1, x2, y2) in LINES {
                cr.move_to(*x1, *y1);
                cr.line_to(*x2, *y2);
            }
            let _ = cr.stroke();
        });
        fixed.put(&lines, 0.0, 0.0);

        // 2) fotos
        picture(DEVICE_PNG, 206.0, 110.0, 228, 250, &fixed);
        picture(SIDE_LEFT_PNG, 8.0, 120.0, 120, 120, &fixed);
        picture(SIDE_RIGHT_PNG, 512.0, 120.0, 120, 120, &fixed);

        // 3) puntos encima de las fotos
        let dots = gtk::DrawingArea::new();
        dots.set_size_request(640, 456);
        dots.set_can_target(false);
        dots.set_draw_func(|_, cr, _, _| {
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.8);
            for (x, y) in DOTS {
                cr.arc(*x, *y, 4.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(0.95, 0.95, 0.95, 0.9);
                let _ = cr.fill();
                cr.set_source_rgba(0.5, 0.5, 0.5, 0.8);
                cr.arc(*x, *y, 4.0, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
            }
        });
        fixed.put(&dots, 0.0, 0.0);

        // 4) tarjetas de controles
        let mut cards: HashMap<Highlight, CardHandles> = HashMap::new();

        let on_toast = Rc::new(on_toast);
        let mk_binding = |cfg: &Rc<RefCell<Config>>,
                          id: ButtonId,
                          mode: &'static str,
                          toast: Rc<dyn Fn(String)>| {
            let cfg = cfg.clone();
            let label = id.label();
            move |action: Action| {
                {
                    let mut c = cfg.borrow_mut();
                    let binding =
                        c.buttons
                            .entry(id)
                            .or_insert_with(|| crate::config::ControlBinding {
                                press: Action::None,
                                spin: Action::None,
                            });
                    match mode {
                        "press" => binding.press = action.clone(),
                        _ => binding.spin = action.clone(),
                    }
                    c.save();
                }
                let what = if mode == "press" {
                    "pulsar"
                } else {
                    "rueda + botón"
                };
                toast(format!("{label} · {what} → {}", action.label()));
            }
        };

        // Botones (Pulsar + Girar)
        let button_specs: &[(ButtonId, f64, f64, i32)] = &[
            (ButtonId::TopLeft, 8.0, 6.0, 190),
            (ButtonId::TopCenter, 225.0, 6.0, 190),
            (ButtonId::TopRight, 442.0, 6.0, 190),
            (ButtonId::SideLeftUpper, 8.0, 250.0, 170),
            (ButtonId::SideLeftLower, 8.0, 322.0, 170),
            (ButtonId::SideRightUpper, 462.0, 250.0, 170),
            (ButtonId::SideRightLower, 462.0, 322.0, 170),
        ];
        for (id, x, y, w) in button_specs {
            let (press_cur, spin_cur) = {
                let c = config.borrow();
                match c.buttons.get(id) {
                    Some(b) => (b.press.clone(), b.spin.clone()),
                    None => (Action::None, Action::None),
                }
            };
            let press_dd = dropdown(
                actions::press_options(),
                &press_cur,
                mk_binding(&config, *id, "press", on_toast.clone()),
            );
            let spin_dd = dropdown(
                actions::spin_options(),
                &spin_cur,
                mk_binding(&config, *id, "spin", on_toast.clone()),
            );
            let card = control_card(
                &fixed,
                *x,
                *y,
                *w,
                id.label(),
                &[("Pulsar", press_dd), ("Girar", spin_dd)],
            );
            cards.insert(Highlight::Button(*id), CardHandles { container: card });
        }

        // Gestos de la rueda (un desplegable cada uno)
        let dial_specs: &[(
            Highlight,
            &'static str,
            f64,
            f64,
            i32,
            fn(&Config) -> Action,
            &'static [(Action, &'static str)],
        )] = &[
            (
                Highlight::DialRotate,
                "Rueda · girar",
                8.0,
                396.0,
                200,
                |c| c.dial_rotate.clone(),
                actions::spin_options(),
            ),
            (
                Highlight::DialPress,
                "Rueda · pulsar",
                224.0,
                396.0,
                200,
                |c| c.dial_press.clone(),
                actions::press_options(),
            ),
            (
                Highlight::DialLong,
                "Rueda · pulsación larga",
                440.0,
                396.0,
                192,
                |c| c.dial_long_press.clone(),
                actions::press_options(),
            ),
        ];
        for (hl, name, x, y, w, getter, options) in dial_specs {
            let name: &'static str = name;
            let current = getter(&config.borrow());
            let cfg = config.clone();
            let hl2 = *hl;
            let toast = on_toast.clone();
            let dd = dropdown(options, &current, move |action| {
                {
                    let mut c = cfg.borrow_mut();
                    match hl2 {
                        Highlight::DialRotate => c.dial_rotate = action.clone(),
                        Highlight::DialPress => c.dial_press = action.clone(),
                        Highlight::DialLong => c.dial_long_press = action.clone(),
                        _ => {}
                    }
                    c.save();
                }
                toast(format!("{} → {}", name, action.label()));
            });
            let card = control_card(&fixed, *x, *y, *w, name, &[("Acción", dd)]);
            cards.insert(*hl, CardHandles { container: card });
        }

        Self {
            widget: fixed,
            cards,
        }
    }

    /// Resalta brevemente la tarjeta del control que generó un evento real.
    pub fn highlight(&self, target: Highlight) {
        let Some(card) = self.cards.get(&target) else {
            return;
        };
        card.container.add_css_class("ctl-active");
        let container = card.container.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
            container.remove_css_class("ctl-active");
        });
    }
}
