//! Mapa del dispositivo estilo OpenLogi: foto frontal + vistas laterales con
//! puntos de referencia, y una tarjeta por control con sus desplegables de
//! acción (Pulsar / Girar).
//!
//! La disposición usa un `gtk::Grid`: las tarjetas fluyen alrededor del bloque
//! de fotos, así que no se solapan aunque cambie el tema, la fuente o el
//! escalado de la interfaz.

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

/// Lienzo lógico del bloque de fotos (coordenadas del mockup) y su escala.
const HERO_W: f64 = 640.0;
const HERO_H: f64 = 456.0;
const HERO_SCALE: f64 = 0.75;

fn s(v: f64) -> f64 {
    v * HERO_SCALE
}

fn s_i(v: f64) -> i32 {
    (v * HERO_SCALE).round() as i32
}

/// Qué tarjeta resaltar cuando llega un evento real del dial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Highlight {
    Button(ButtonId),
    DialRotate,
    DialPress,
    DialLong,
}

// Líneas guía: de cada punto a la fila de tarjetas que le corresponde (arriba
// para los botones superiores, abajo para los gestos de la rueda). Se dibujan
// por debajo de las fotos, así que solo se ven en el espacio libre.
const LINES: &[(f64, f64, f64, f64)] = &[
    (245.0, 0.0, 245.0, 137.0),
    (312.0, 0.0, 312.0, 137.0),
    (388.0, 0.0, 388.0, 137.0),
    (288.0, 215.0, 288.0, 456.0),
    (307.0, 261.0, 307.0, 456.0),
    (327.0, 222.0, 327.0, 456.0),
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
    pub widget: gtk::Grid,
    cards: HashMap<Highlight, CardHandles>,
}

fn texture(bytes: &'static [u8]) -> gtk::gdk::Texture {
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_static(bytes))
        .expect("PNG de asset empaquetado")
}

fn picture(bytes: &'static [u8], x: f64, y: f64, w: f64, h: f64, fixed: &gtk::Fixed) {
    let pic = gtk::Picture::for_paintable(&texture(bytes));
    pic.set_can_target(false);
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_size_request(s_i(w), s_i(h));
    // Marco blanco tipo "foto de producto"
    pic.add_css_class("dev-photo");
    fixed.put(&pic, s(x), s(y));
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
    dd.set_size_request(150, -1);
    dd.set_hexpand(true);
    dd.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        if let Some((action, _)) = options.get(idx) {
            on_change(action.clone());
        }
    });
    dd
}

fn control_card(name: &str, rows: &[(&str, gtk::DropDown)]) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
    card.add_css_class("ctl-card");
    card.set_hexpand(true);
    card.set_halign(gtk::Align::Fill);

    let title = gtk::Label::new(Some(name));
    title.add_css_class("ctl-name");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(1);
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

    card
}

/// Bloque central: las tres fotos con sus puntos y líneas de referencia.
fn build_hero() -> gtk::Fixed {
    let w = s_i(HERO_W);
    let h = s_i(HERO_H);

    let fixed = gtk::Fixed::new();
    fixed.set_size_request(w, h);
    fixed.set_halign(gtk::Align::Center);
    fixed.set_valign(gtk::Align::Center);

    // 1) líneas debajo de las fotos
    let lines = gtk::DrawingArea::new();
    lines.set_size_request(w, h);
    lines.set_can_target(false);
    lines.set_draw_func(|_, cr, _, _| {
        cr.scale(HERO_SCALE, HERO_SCALE);
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.55);
        cr.set_line_width(1.4);
        for (x1, y1, x2, y2) in LINES {
            cr.move_to(*x1, *y1);
            cr.line_to(*x2, *y2);
        }
        let _ = cr.stroke();
    });
    fixed.put(&lines, 0.0, 0.0);

    // 2) fotos
    picture(DEVICE_PNG, 206.0, 110.0, 228.0, 250.0, &fixed);
    picture(SIDE_LEFT_PNG, 8.0, 120.0, 120.0, 120.0, &fixed);
    picture(SIDE_RIGHT_PNG, 512.0, 120.0, 120.0, 120.0, &fixed);

    // 3) puntos encima de las fotos
    let dots = gtk::DrawingArea::new();
    dots.set_size_request(w, h);
    dots.set_can_target(false);
    dots.set_draw_func(|_, cr, _, _| {
        cr.scale(HERO_SCALE, HERO_SCALE);
        for (x, y) in DOTS {
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.8);
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

    fixed
}

/// Columna vertical de tarjetas (lados del dispositivo).
fn column(cards: &[gtk::Box]) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 12);
    col.set_valign(gtk::Align::Center);
    col.set_halign(gtk::Align::Fill);
    col.set_hexpand(true);
    for card in cards {
        col.append(card);
    }
    col
}

impl ControlMap {
    pub fn new(config: Rc<RefCell<Config>>, on_toast: impl Fn(String) + 'static) -> Self {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(16);
        grid.set_row_spacing(14);
        grid.set_halign(gtk::Align::Center);

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

        // Tarjeta de un botón con sus dos modos (Pulsar / Girar).
        let mut button_card = |id: ButtonId| -> gtk::Box {
            let (press_cur, spin_cur) = {
                let c = config.borrow();
                match c.buttons.get(&id) {
                    Some(b) => (b.press.clone(), b.spin.clone()),
                    None => (Action::None, Action::None),
                }
            };
            let press_dd = dropdown(
                actions::press_options(),
                &press_cur,
                mk_binding(&config, id, "press", on_toast.clone()),
            );
            let spin_dd = dropdown(
                actions::spin_options(),
                &spin_cur,
                mk_binding(&config, id, "spin", on_toast.clone()),
            );
            let card = control_card(id.label(), &[("Pulsar", press_dd), ("Girar", spin_dd)]);
            cards.insert(
                Highlight::Button(id),
                CardHandles {
                    container: card.clone(),
                },
            );
            card
        };

        // Fila superior (la del centro no se expande: marca el ancho del bloque de fotos)
        let top_left = button_card(ButtonId::TopLeft);
        let top_center = button_card(ButtonId::TopCenter);
        let top_right = button_card(ButtonId::TopRight);
        top_center.set_hexpand(false);

        // Filas laterales
        let side_left = column(&[
            button_card(ButtonId::SideLeftUpper),
            button_card(ButtonId::SideLeftLower),
        ]);
        let side_right = column(&[
            button_card(ButtonId::SideRightUpper),
            button_card(ButtonId::SideRightLower),
        ]);

        // Gestos de la rueda (un desplegable cada uno)
        let dial_specs: &[(
            Highlight,
            &'static str,
            fn(&Config) -> Action,
            &'static [(Action, &'static str)],
        )] = &[
            (
                Highlight::DialRotate,
                "Rueda · girar",
                |c| c.dial_rotate.clone(),
                actions::spin_options(),
            ),
            (
                Highlight::DialPress,
                "Rueda · pulsar",
                |c| c.dial_press.clone(),
                actions::press_options(),
            ),
            (
                Highlight::DialLong,
                "Rueda · pulsación larga",
                |c| c.dial_long_press.clone(),
                actions::press_options(),
            ),
        ];
        let mut dial_cards = Vec::new();
        for (hl, name, getter, options) in dial_specs {
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
            let card = control_card(name, &[("Acción", dd)]);
            cards.insert(
                *hl,
                CardHandles {
                    container: card.clone(),
                },
            );
            dial_cards.push(card);
        }

        let hero = build_hero();

        grid.attach(&top_left, 0, 0, 1, 1);
        grid.attach(&top_center, 1, 0, 1, 1);
        grid.attach(&top_right, 2, 0, 1, 1);
        grid.attach(&side_left, 0, 1, 1, 1);
        grid.attach(&hero, 1, 1, 1, 1);
        grid.attach(&side_right, 2, 1, 1, 1);
        for (col, card) in dial_cards.iter().enumerate() {
            // La tarjeta central sigue el ancho del bloque de fotos, no se estira.
            if col == 1 {
                card.set_hexpand(false);
            }
            grid.attach(card, col as i32, 2, 1, 1);
        }

        Self {
            widget: grid,
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
