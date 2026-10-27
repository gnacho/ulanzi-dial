//! Mapa del dispositivo estilo OpenLogi: una sola foto frontal con un punto
//! sobre cada control real y una tarjeta por control alrededor, unida a su
//! punto por una línea guía.
//!
//! Las tarjetas viven en un `gtk::Fixed` con coordenadas fijas, pero la línea
//! y el punto se dibujan en una capa aparte que calcula la caja real de cada
//! tarjeta al pintar (`compute_bounds`), así que la guía sigue apuntando al
//! control correcto aunque el tema o la fuente cambien el alto de la tarjeta.

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

/// Lienzo del mapa, en píxeles lógicos.
const CANVAS_W: f64 = 960.0;
const CANVAS_H: f64 = 542.0;

/// Foto frontal dentro del lienzo (aspecto 316x348).
const PHOTO_X: f64 = 330.0;
const PHOTO_Y: f64 = 120.0;
const PHOTO_W: f64 = 300.0;
const PHOTO_H: f64 = 330.0;

const CARD_W: i32 = 290;

/// Punto de un control expresado en fracciones de la foto (0..1). Sacado de
/// la foto real: tres botones en la franja superior, dos pulsadores en cada
/// canto lateral y la rueda en el centro.
fn dot(fx: f64, fy: f64) -> (f64, f64) {
    (PHOTO_X + fx * PHOTO_W, PHOTO_Y + fy * PHOTO_H)
}

fn dots() -> Vec<(Highlight, (f64, f64))> {
    vec![
        (Highlight::Button(ButtonId::TopLeft), dot(0.17, 0.11)),
        (Highlight::Button(ButtonId::TopCenter), dot(0.50, 0.11)),
        (Highlight::Button(ButtonId::TopRight), dot(0.83, 0.11)),
        (Highlight::Button(ButtonId::SideLeftUpper), dot(0.05, 0.37)),
        (Highlight::Button(ButtonId::SideLeftLower), dot(0.05, 0.59)),
        (Highlight::Button(ButtonId::SideRightUpper), dot(0.95, 0.37)),
        (Highlight::Button(ButtonId::SideRightLower), dot(0.95, 0.59)),
        (Highlight::DialRotate, dot(0.31, 0.78)),
        (Highlight::DialPress, dot(0.50, 0.59)),
    ]
}

/// Posición de cada tarjeta en el lienzo. Alrededor de la foto, no encima.
const CARD_POS: &[(Highlight, f64, f64)] = &[
    (Highlight::Button(ButtonId::TopLeft), 6.0, 6.0),
    (Highlight::Button(ButtonId::TopCenter), 335.0, 6.0),
    (Highlight::Button(ButtonId::TopRight), 664.0, 6.0),
    (Highlight::Button(ButtonId::SideLeftUpper), 6.0, 176.0),
    (Highlight::Button(ButtonId::SideLeftLower), 6.0, 330.0),
    (Highlight::Button(ButtonId::SideRightUpper), 664.0, 176.0),
    (Highlight::Button(ButtonId::SideRightLower), 664.0, 330.0),
    (Highlight::DialRotate, 167.0, 470.0),
    (Highlight::DialPress, 503.0, 470.0),
];

/// Qué tarjeta resaltar cuando llega un evento real del dial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Highlight {
    Button(ButtonId),
    DialRotate,
    DialPress,
}

/// Tarjeta con su punto de la foto y el control al que representa.
struct Link {
    card: gtk::Box,
    dot: (f64, f64),
    target: Highlight,
}

pub struct ControlMap {
    pub widget: gtk::Fixed,
    cards: Rc<RefCell<HashMap<Highlight, gtk::Box>>>,
    selected: Rc<RefCell<Option<Highlight>>>,
    transient: Rc<RefCell<Option<Highlight>>>,
    overlay: gtk::DrawingArea,
}

fn texture(bytes: &'static [u8]) -> gtk::gdk::Texture {
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_static(bytes))
        .expect("PNG de asset empaquetado")
}

fn picture(bytes: &'static [u8]) -> gtk::Picture {
    let pic = gtk::Picture::for_paintable(&texture(bytes));
    pic.set_can_target(false);
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_size_request(PHOTO_W as i32, PHOTO_H as i32);
    pic.add_css_class("dev-photo");
    pic
}

/// Color de acento del sistema (el que tenga configurado el escritorio).
#[allow(deprecated)]
fn accent(widget: &impl IsA<gtk::Widget>) -> gtk::gdk::RGBA {
    widget
        .style_context()
        .lookup_color("accent_color")
        .unwrap_or_else(|| gtk::gdk::RGBA::new(0.21, 0.52, 0.89, 1.0))
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
    card.set_size_request(CARD_W, -1);
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
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
        row.append(dd);
        card.append(&row);
    }

    card
}

impl ControlMap {
    pub fn new(config: Rc<RefCell<Config>>, on_toast: impl Fn(String) + 'static) -> Self {
        let fixed = gtk::Fixed::new();
        fixed.set_size_request(CANVAS_W as i32, CANVAS_H as i32);
        fixed.set_halign(gtk::Align::Center);
        fixed.set_valign(gtk::Align::Center);

        // 1) la foto, al fondo
        fixed.put(&picture(DEVICE_PNG), PHOTO_X, PHOTO_Y);

        // 2) capa de líneas y puntos, por encima de la foto
        let overlay = gtk::DrawingArea::new();
        overlay.set_size_request(CANVAS_W as i32, CANVAS_H as i32);
        overlay.set_can_target(false);
        fixed.put(&overlay, 0.0, 0.0);

        let cards: Rc<RefCell<HashMap<Highlight, gtk::Box>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let selected: Rc<RefCell<Option<Highlight>>> = Rc::new(RefCell::new(None));
        let transient: Rc<RefCell<Option<Highlight>>> = Rc::new(RefCell::new(None));
        let links: Rc<RefCell<Vec<Link>>> = Rc::new(RefCell::new(Vec::new()));

        let on_toast = Rc::new(on_toast);
        let mk_binding = |cfg: &Rc<RefCell<Config>>, id: ButtonId, toast: Rc<dyn Fn(String)>| {
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
                            });
                    binding.press = action.clone();
                    c.save();
                }
                toast(format!("{label} → {}", action.label()));
            }
        };

        // Tarjeta de un botón: solo tiene acción de pulsar.
        let button_card = |id: ButtonId| -> gtk::Box {
            let press_cur = {
                let c = config.borrow();
                match c.buttons.get(&id) {
                    Some(b) => b.press.clone(),
                    None => Action::None,
                }
            };
            let press_dd = dropdown(
                actions::press_options(),
                &press_cur,
                mk_binding(&config, id, on_toast.clone()),
            );
            control_card(id.label(), &[("Pulsar", press_dd)])
        };

        for id in [
            ButtonId::TopLeft,
            ButtonId::TopCenter,
            ButtonId::TopRight,
            ButtonId::SideLeftUpper,
            ButtonId::SideLeftLower,
            ButtonId::SideRightUpper,
            ButtonId::SideRightLower,
        ] {
            let card = button_card(id);
            cards.borrow_mut().insert(Highlight::Button(id), card);
        }

        // Gestos de la rueda (un desplegable cada uno).
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
                actions::rotate_options(),
            ),
            (
                Highlight::DialPress,
                "Rueda · pulsar",
                |c| c.dial_press.clone(),
                actions::press_options(),
            ),
        ];
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
                        _ => {}
                    }
                    c.save();
                }
                toast(format!("{} → {}", name, action.label()));
            });
            let card = control_card(name, &[("Acción", dd)]);
            cards.borrow_mut().insert(*hl, card);
        }

        // 3) colocar cada tarjeta y registrar su enlace con el punto.
        let dot_map: HashMap<Highlight, (f64, f64)> = dots().into_iter().collect();
        for (hl, x, y) in CARD_POS {
            let Some(card) = cards.borrow_mut().remove(hl) else {
                continue;
            };
            fixed.put(&card, *x, *y);

            // Al pulsar la tarjeta queda seleccionada (acento del sistema).
            let gesture = gtk::GestureClick::new();
            {
                let hl = *hl;
                let selected = selected.clone();
                let cards = cards.clone();
                let overlay = overlay.clone();
                gesture.connect_pressed(move |_, _, _, _| {
                    for (_, card) in cards.borrow().iter() {
                        card.remove_css_class("ctl-selected");
                    }
                    *selected.borrow_mut() = Some(hl);
                    if let Some(card) = cards.borrow().get(&hl) {
                        card.add_css_class("ctl-selected");
                    }
                    overlay.queue_draw();
                });
            }
            card.add_controller(gesture);

            links.borrow_mut().push(Link {
                card: card.clone(),
                dot: *dot_map.get(hl).unwrap_or(&(0.0, 0.0)),
                target: *hl,
            });
            cards.borrow_mut().insert(*hl, card);
        }

        // 4) repintar durante los primeros fotogramas: al colocarse las tarjetas
        //    la caja real cambia, y la guía se calcula con esa caja.
        {
            let overlay_tick = overlay.clone();
            let frames = std::cell::Cell::new(0u32);
            fixed.add_tick_callback(move |_, _| {
                overlay_tick.queue_draw();
                frames.set(frames.get() + 1);
                if frames.get() < 20 {
                    gtk::glib::ControlFlow::Continue
                } else {
                    gtk::glib::ControlFlow::Break
                }
            });
        }

        // 5) pintura de líneas y puntos.
        {
            let links = links.clone();
            let selected = selected.clone();
            let transient = transient.clone();
            let dot_map = dot_map.clone();
            overlay.set_draw_func(move |area, cr, _, _| {
                let Some(parent) = area.parent() else {
                    return;
                };
                let accent = accent(area);

                // Líneas: del borde de la tarjeta más cercano al punto hasta el punto.
                for link in links.borrow().iter() {
                    let Some(rect) = link.card.compute_bounds(&parent) else {
                        continue;
                    };
                    let (dx, dy) = link.dot;
                    let x = dx.clamp(rect.x() as f64, (rect.x() + rect.width()) as f64);
                    let y = dy.clamp(rect.y() as f64, (rect.y() + rect.height()) as f64);
                    let highlighted = *selected.borrow() == Some(link.target)
                        || *transient.borrow() == Some(link.target);
                    if highlighted {
                        cr.set_source_rgba(
                            accent.red() as f64,
                            accent.green() as f64,
                            accent.blue() as f64,
                            0.85,
                        );
                        cr.set_line_width(2.0);
                    } else {
                        cr.set_source_rgba(0.45, 0.45, 0.5, 0.4);
                        cr.set_line_width(1.2);
                    }
                    cr.move_to(x, y);
                    cr.line_to(dx, dy);
                    let _ = cr.stroke();
                }

                // Puntos: uno por control, sobre la foto.
                let sel = *selected.borrow();
                let tr = *transient.borrow();
                for (target, (dx, dy)) in dot_map.iter() {
                    let active = sel == Some(*target) || tr == Some(*target);
                    if active {
                        cr.set_source_rgba(
                            accent.red() as f64,
                            accent.green() as f64,
                            accent.blue() as f64,
                            0.22,
                        );
                        cr.arc(*dx, *dy, 10.0, 0.0, std::f64::consts::TAU);
                        let _ = cr.fill();
                        cr.set_source_rgba(
                            accent.red() as f64,
                            accent.green() as f64,
                            accent.blue() as f64,
                            1.0,
                        );
                        cr.arc(*dx, *dy, 5.0, 0.0, std::f64::consts::TAU);
                    } else {
                        cr.set_source_rgba(0.30, 0.30, 0.34, 0.75);
                        cr.arc(*dx, *dy, 3.6, 0.0, std::f64::consts::TAU);
                    }
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                    cr.set_line_width(1.4);
                    let _ = cr.stroke();
                }
            });
        }

        Self {
            widget: fixed,
            cards,
            selected,
            transient,
            overlay,
        }
    }

    /// Resalta brevemente la tarjeta del control que generó un evento real.
    pub fn highlight(&self, target: Highlight) {
        *self.transient.borrow_mut() = Some(target);
        if let Some(card) = self.cards.borrow().get(&target) {
            card.add_css_class("ctl-active");
            let card = card.clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                card.remove_css_class("ctl-active");
            });
        }
        self.overlay.queue_draw();
        let transient = self.transient.clone();
        let overlay = self.overlay.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
            *transient.borrow_mut() = None;
            overlay.queue_draw();
        });
    }
}
