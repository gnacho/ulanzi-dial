//! Mapa del dispositivo estilo OpenLogi: una sola foto frontal con un punto
//! sobre cada control real y una tarjeta por control alrededor, unida a su
//! punto por una línea guía.
//!
//! Las tarjetas viven en un `gtk::Fixed` con coordenadas fijas, pero la línea
//! y el punto se dibujan en una capa aparte que calcula la caja real de cada
//! tarjeta al pintar (`compute_bounds`), así que la guía sigue apuntando al
//! control correcto aunque el tema o la fuente cambien el alto de la tarjeta.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;

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
    transient: Rc<RefCell<Option<Highlight>>>,
    overlay: gtk::DrawingArea,
    config: Rc<RefCell<Config>>,
    dropdowns: Rc<RefCell<HashMap<ButtonId, gtk::DropDown>>>,
    chord_ui: Rc<RefCell<HashMap<ButtonId, (gtk::Button, gtk::Label)>>>,
    /// Activa durante refresh(): los desplegables repintan sin persistir.
    updating: Rc<Cell<bool>>,
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
/// Texto del atajo tal como se ensena en la tarjeta. Las teclas multimedia
/// se leen mejor con su nombre que con el codigo de evdev.
fn chord_display(combo: &crate::backend::keymap::KeyCombo) -> String {
    use evdev::Key;
    let plain = !combo.ctrl && !combo.shift && !combo.alt && !combo.logo;
    if plain {
        match combo.key {
            Key::KEY_PLAYPAUSE => return "Play".to_string(),
            Key::KEY_MUTE => return "Mudo".to_string(),
            _ => {}
        }
    }
    combo.label()
}

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
    dd.set_list_factory(Some(&icon_factory(options)));
    dd.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        if let Some((action, _)) = options.get(idx) {
            on_change(action.clone());
        }
    });
    dd
}

/// Diálogo para escribir el comando de una acción personalizada. Solo llama
/// a `on_save` con texto no vacío; cancelar o vacío deja la acción como estaba.
fn open_command_dialog(
    parent: Option<&adw::ApplicationWindow>,
    on_save: impl Fn(String) + 'static,
) {
    let dialog = adw::MessageDialog::new(
        parent,
        Some("Comando personalizado"),
        Some("Se ejecuta con tu shell al pulsar el control."),
    );
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("notify-send \"Hola desde el dial\""));
    entry.set_width_chars(38);
    dialog.set_extra_child(Some(&entry));
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("save", "Guardar");
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");
    {
        let dialog = dialog.clone();
        entry.connect_activate(move |_| dialog.response("save"));
    }
    dialog.connect_response(Some("save"), move |dialog, _| {
        let text = dialog
            .extra_child()
            .and_downcast::<gtk::Entry>()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default();
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            on_save(trimmed);
        }
    });
    dialog.present();
}

/// Fábrica de items de la lista desplegable: icono simbólico más etiqueta.
///
/// El icono se pinta solo en la lista abierta. El botón cerrado sigue mostrando
/// texto, porque las tarjetas del mapa tienen 290 px y un icono más el texto no
/// cabe junto al chip de modo y al botón de atajo.
fn icon_factory(options: &'static [(Action, &'static str)]) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let image = gtk::Image::new();
        image.set_pixel_size(16);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        row.append(&image);
        row.append(&label);
        item.set_child(Some(&row));
    });

    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(child), Some(object)) = (item.child(), item.item()) else {
            return;
        };
        let Ok(row) = child.downcast::<gtk::Box>() else {
            return;
        };
        let (Some(first), Ok(string)) = (row.first_child(), object.downcast::<gtk::StringObject>())
        else {
            return;
        };
        let Ok(image) = first.downcast::<gtk::Image>() else {
            return;
        };
        let Some(second) = image.next_sibling() else {
            return;
        };
        let Ok(label) = second.downcast::<gtk::Label>() else {
            return;
        };
        let text = string.string();
        let icon = options
            .iter()
            .find(|(_, label)| *label == text.as_str())
            .map(|(action, _)| action.icon())
            .unwrap_or("action-unavailable-symbolic");
        image.set_icon_name(Some(icon));
        label.set_text(text.as_str());
    });

    factory
}

fn control_card(name: &str, rows: &[(&str, &gtk::Widget)]) -> gtk::Box {
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

    for (mode, widget) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let chip = gtk::Label::new(Some(mode));
        chip.add_css_class("ctl-mode");
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
        row.append(*widget);
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
        let dropdowns: Rc<RefCell<HashMap<ButtonId, gtk::DropDown>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let chord_ui: Rc<RefCell<HashMap<ButtonId, (gtk::Button, gtk::Label)>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let on_toast = Rc::new(on_toast);
        let updating: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let mk_binding = |cfg: &Rc<RefCell<Config>>, id: ButtonId, toast: Rc<dyn Fn(String)>| {
            let cfg = cfg.clone();
            let label = id.label();
            move |action: Action| {
                {
                    let mut c = cfg.borrow_mut();
                    c.set_press(id, action.clone());
                    c.save();
                }
                toast(format!("{label} → {}", action.label()));
            }
        };

        // Tarjeta de un botón: solo tiene acción de pulsar. El desplegable
        // lleva las acciones habituales y, al lado, un botón con el atajo de
        // teclado actual (o "Atajo…" si la acción no es un atajo).
        let button_card = |id: ButtonId| -> gtk::Box {
            let press_cur = config.borrow().press_for(id);
            let on_change = Rc::new(mk_binding(&config, id, on_toast.clone()));

            let chord_text = match press_cur.shortcut() {
                Some(combo) => chord_display(&combo),
                None => "Sin atajo".to_string(),
            };
            let chord_known = press_cur.shortcut().is_some();
            // El botón lleva una etiqueta propia para poder recortarla: el
            // hueco de la tarjeta es fijo y un atajo largo la ensancharía
            // hasta pisar la tarjeta vecina.
            let chord_label = gtk::Label::new(Some(&chord_text));
            chord_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            chord_label.set_max_width_chars(12);
            let chord_button = gtk::Button::new();
            chord_button.add_css_class("flat");
            chord_button.add_css_class("ctl-chord");
            chord_button.set_child(Some(&chord_label));
            chord_button.set_size_request(102, -1);
            chord_button.set_sensitive(chord_known);
            chord_button.set_tooltip_text(Some(if chord_known {
                "Grabar otro atajo de teclado"
            } else {
                "Esta acción no se ejecuta con un atajo"
            }));

            let press_dd = dropdown(actions::press_options(), &press_cur, {
                let on_change = on_change.clone();
                let chord_label = chord_label.clone();
                let chord_button = chord_button.clone();
                let updating = updating.clone();
                move |action: Action| {
                    if updating.get() {
                        // Refresh() programático: repintar sin persistir,
                        // si no, cada cambio de perfil materializaba el
                        // fallback global como binding explícito del perfil.
                        return;
                    }
                    if matches!(&action, Action::Command(c) if c.is_empty()) {
                        // Fila centinela "Ejecutar comando…": pedir el
                        // comando; no se guarda hasta que el usuario escribe
                        // uno y acepta.
                        let root = chord_button.root().and_downcast::<adw::ApplicationWindow>();
                        open_command_dialog(root.as_ref(), {
                            let on_change = on_change.clone();
                            move |cmd| on_change(Action::Command(cmd))
                        });
                        return;
                    }
                    match action.shortcut() {
                        Some(combo) => {
                            chord_label.set_label(&chord_display(&combo));
                            chord_button.set_sensitive(true);
                            chord_button.set_tooltip_text(Some("Grabar otro atajo de teclado"));
                        }
                        None => {
                            chord_label.set_label("Sin atajo");
                            chord_button.set_sensitive(false);
                            chord_button
                                .set_tooltip_text(Some("Esta acción no se ejecuta con un atajo"));
                        }
                    }
                    on_change(action);
                }
            });
            // Con un atajo guardado el desplegable no tiene una entrada que lo
            // represente: se deja sin selección y manda el botón de al lado.
            if matches!(press_cur, Action::Chord(_)) {
                press_dd.set_selected(gtk::INVALID_LIST_POSITION);
            }
            dropdowns.borrow_mut().insert(id, press_dd.clone());
            chord_ui
                .borrow_mut()
                .insert(id, (chord_button.clone(), chord_label.clone()));

            {
                let on_change = on_change.clone();
                let chord_label_weak = chord_label.downgrade();
                let chord_button_weak = chord_button.downgrade();
                let start = match &press_cur {
                    Action::Chord(value) => Some(value.clone()),
                    _ => None,
                };
                chord_button.connect_clicked(move |btn| {
                    let parent = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                    let on_change = on_change.clone();
                    let chord_label_weak = chord_label_weak.clone();
                    let chord_button_weak = chord_button_weak.clone();
                    let start = start.clone();
                    super::chord_dialog::present(parent.as_ref(), start.as_deref(), move |chord| {
                        if let Some(label) = chord_label_weak.upgrade() {
                            label.set_label(&chord);
                        }
                        if let Some(button) = chord_button_weak.upgrade() {
                            button.set_tooltip_text(Some(&chord));
                        }
                        on_change(Action::Chord(chord));
                    });
                });
            }

            press_dd.set_size_request(100, -1);
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.append(&press_dd);
            row.append(&chord_button);
            control_card(id.label(), &[("Pulsar", row.upcast_ref::<gtk::Widget>())])
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
        type DialSpec = (
            Highlight,
            &'static str,
            fn(&Config) -> Action,
            &'static [(Action, &'static str)],
        );
        let dial_specs: &[DialSpec] = &[
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
            let updating = updating.clone();
            let hl2 = *hl;
            let toast = on_toast.clone();
            let dd = dropdown(options, &current, move |action| {
                if updating.get() {
                    // Refresh() programático: repintar sin persistir.
                    return;
                }
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
            let card = control_card(name, &[("Acción", dd.upcast_ref::<gtk::Widget>())]);
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
                    for card in cards.borrow().values() {
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
            transient,
            overlay,
            config,
            dropdowns,
            chord_ui,
            updating,
        }
    }

    /// Resalta brevemente la tarjeta del control que generó un evento real.
    /// Relee las asignaciones de la configuración y repinta las tarjetas.
    /// Se llama cuando cambia el perfil (aplicación activa o perfil fijado).
    pub fn refresh(&self) {
        // Los set_selected de aquí disparan connect_selected_notify: con la
        // bandera activa repintan sin escribir config ni toasts.
        self.updating.set(true);
        let cfg = self.config.borrow();
        for (id, dd) in self.dropdowns.borrow().iter() {
            let action = cfg.press_for(*id);
            dd.set_selected(actions::index_of(actions::press_options(), &action));
            if matches!(action, Action::Chord(_)) {
                dd.set_selected(gtk::INVALID_LIST_POSITION);
            }
            if let Some((button, label)) = self.chord_ui.borrow().get(id) {
                match action.shortcut() {
                    Some(combo) => {
                        label.set_label(&chord_display(&combo));
                        button.set_sensitive(true);
                        button.set_tooltip_text(Some("Grabar otro atajo de teclado"));
                    }
                    None => {
                        label.set_label("Sin atajo");
                        button.set_sensitive(false);
                        button.set_tooltip_text(Some("Esta acción no se ejecuta con un atajo"));
                    }
                }
            }
        }
        self.updating.set(false);
    }

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
