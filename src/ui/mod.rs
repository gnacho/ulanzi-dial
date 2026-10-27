//! Ensamblado de la ventana principal y cableado backend ↔ UI.

pub mod control_map;
pub mod device_page;
pub mod general_page;
pub mod hud;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use crate::backend::actions::{Action, ActionExecutor};
use crate::backend::decoder::DialEvent;
use crate::backend::device::{self, BackendMsg, ReaderConfig};
use crate::config::Config;
use control_map::Highlight;

/// Estado compartido entre la UI y el reenviador de eventos del backend.
struct Shared {
    config: Rc<RefCell<Config>>,
    executor: RefCell<Option<ActionExecutor>>,
    /// Valor sintético 0-100 para el HUD (volumen/brillo/zoom).
    hud_value: Cell<i32>,
    enabled: Cell<bool>,
    overlay: adw::ToastOverlay,
    banner: adw::Banner,
}

impl Shared {
    fn toast(&self, msg: impl Into<String>) {
        self.overlay.add_toast(adw::Toast::new(&msg.into()));
    }

    fn with_executor<R>(&self, f: impl FnOnce(&mut ActionExecutor) -> R) -> Option<R> {
        let mut slot = self.executor.borrow_mut();
        if slot.is_none() {
            match ActionExecutor::new() {
                Ok(exec) => *slot = Some(exec),
                Err(e) => {
                    tracing::warn!("uinput no disponible: {e}");
                    self.toast(
                        "No se pudo crear el dispositivo virtual (revisa permisos de /dev/uinput)",
                    );
                    return None;
                }
            }
        }
        slot.as_mut().map(f)
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        .ctl-card {
            background: @card_bg_color;
            border: 1px solid alpha(@borders, 0.7);
            border-radius: 12px;
            padding: 7px 9px;
        }
        .ctl-card:hover {
            border-color: alpha(@accent_color, 0.45);
        }
        .ctl-card:focus-within,
        .ctl-card.ctl-selected {
            border-color: @accent_color;
            background: alpha(@accent_color, 0.08);
            box-shadow: 0 0 0 2px alpha(@accent_color, 0.28);
        }
        .ctl-active {
            border-color: @accent_color;
            box-shadow: 0 0 0 3px alpha(@accent_color, 0.35);
        }
        .ctl-name { font-size: 11px; font-weight: bold; opacity: 0.6; }
        .ctl-mode {
            font-size: 10px; font-weight: bold;
            color: @accent_color;
            background: alpha(@accent_color, 0.12);
            border-radius: 5px;
            padding: 1px 5px;
        }
        .dev-photo {
            background: white;
            border-radius: 14px;
            border: 1px solid @borders;
        }
        .hud-window {
            background: alpha(black, 0.85);
            color: white;
            border-radius: 12px;
        }
        .hud-window label { color: white; }
        .hud-value { font-size: 22px; font-weight: bold; }
        ",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn build_ui(app: &adw::Application) {
    load_css();

    let config = Config::load();

    // --- backend ---
    let (tx, rx) = async_channel::unbounded::<BackendMsg>();
    device::spawn_reader(
        ReaderConfig {
            keymap: config.keymap(),
            long_press_ms: config.long_press_ms,
            exclusive_grab: config.exclusive_grab,
        },
        tx,
    );

    // --- ventana ---
    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some("Ulanzi Dial"));
    // Ancha y no demasiado alta: tres columnas y tres filas de tarjetas.
    window.set_default_size(1020, 760);
    window.set_resizable(false); // el mapa se compone en un Grid de tamaño fijo

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();

    let theme_btn = gtk::Button::from_icon_name("display-brightness-symbolic");
    theme_btn.set_tooltip_text(Some("Alternar tema claro/oscuro"));
    theme_btn.connect_clicked(|_| {
        let mgr = adw::StyleManager::default();
        let next = match mgr.color_scheme() {
            adw::ColorScheme::ForceDark => adw::ColorScheme::ForceLight,
            _ => adw::ColorScheme::ForceDark,
        };
        mgr.set_color_scheme(next);
    });
    header.pack_start(&theme_btn);

    let stack = adw::ViewStack::new();
    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    header.set_title_widget(Some(&switcher));
    toolbar.add_top_bar(&header);

    let banner = adw::Banner::default();
    banner.set_title(
        "Dial desconectado. Enciende el D100H y mantén pulsado el botón de emparejamiento.",
    );
    banner.set_button_label(Some("Buscar"));
    toolbar.add_top_bar(&banner);

    let overlay = adw::ToastOverlay::new();
    toolbar.set_content(Some(&overlay));
    window.set_content(Some(&toolbar));

    let shared = Rc::new(Shared {
        config: Rc::new(RefCell::new(config)),
        executor: RefCell::new(None),
        hud_value: Cell::new(45),
        enabled: Cell::new(true),
        overlay: overlay.clone(),
        banner: banner.clone(),
    });

    // --- páginas ---
    let toast_cb = {
        let shared = shared.clone();
        move |msg: String| shared.toast(msg)
    };
    let device = device_page::DevicePage::new(
        shared.config.clone(),
        {
            let shared = shared.clone();
            move |msg| shared.toast(msg)
        },
        {
            let shared = shared.clone();
            move |enabled| {
                shared.enabled.set(enabled);
                shared.toast(if enabled {
                    "Dial habilitado"
                } else {
                    "Dial deshabilitado"
                });
            }
        },
    );
    stack.add_titled(&device.borrow().widget, Some("device"), "Dispositivo");

    let general = general_page::build(shared.config.clone(), toast_cb);
    stack.add_titled(&general, Some("general"), "General");

    // Alto fijo: el contenido se compone a tamaño constante; si algún tema lo
    // agranda, el scrolled absorbe la diferencia en vertical sin recortar.
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&stack));
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_vexpand(true);
    overlay.set_child(Some(&scrolled));

    // banner "Buscar": fuerza reescaneo inmediato (el hilo ya reescanea cada 1 s)
    {
        let shared = shared.clone();
        banner.connect_button_clicked(move |_| {
            shared.toast("Buscando el dial…");
        });
    }

    let hud = hud::Hud::new();

    // --- reenviador backend → UI ---
    {
        let shared = shared.clone();
        let device = device.clone();
        let hud = hud.clone();
        gtk::glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                match msg {
                    BackendMsg::Connected(name) => {
                        shared.banner.set_revealed(false);
                        device.borrow_mut().set_connected(true);
                        shared.toast(format!("{name} conectado"));
                    }
                    BackendMsg::Disconnected => {
                        shared.banner.set_revealed(true);
                        device.borrow_mut().set_connected(false);
                        shared.toast("Dial desconectado");
                    }
                    BackendMsg::Battery(pct) => {
                        device.borrow_mut().set_battery(pct);
                    }
                    BackendMsg::Event(ev) => {
                        handle_event(&shared, &device, &hud, ev);
                    }
                }
            }
        });
    }

    window.present();
}

fn handle_event(
    shared: &Rc<Shared>,
    device: &Rc<RefCell<device_page::DevicePage>>,
    hud: &Rc<hud::Hud>,
    ev: DialEvent,
) {
    if !shared.enabled.get() {
        return;
    }
    let cfg = shared.config.borrow();

    match ev {
        DialEvent::Rotate { steps, modifier } => {
            let action = match modifier {
                Some(btn) => cfg
                    .buttons
                    .get(&btn)
                    .map(|b| b.spin.clone())
                    .unwrap_or(Action::None),
                None => cfg.dial_rotate.clone(),
            };
            let steps = steps * cfg.sensitivity as i32 * if cfg.invert { -1 } else { 1 };
            drop(cfg);

            shared.with_executor(|exec| exec.rotate(&action, steps));

            // HUD para acciones con valor 0-100
            if matches!(action, Action::Volume | Action::Brightness | Action::Zoom)
                && shared.config.borrow().show_hud
            {
                let v = (shared.hud_value.get() + steps).clamp(0, 100);
                shared.hud_value.set(v);
                hud.show(&action.label(), v);
            }
            let hl = match modifier {
                Some(b) => Highlight::Button(b),
                None => Highlight::DialRotate,
            };
            device.borrow().map.highlight(hl);
        }
        DialEvent::DialPress => {
            let action = cfg.dial_press.clone();
            drop(cfg);
            shared.with_executor(|exec| exec.press(&action));
            device.borrow().map.highlight(Highlight::DialPress);
        }
        DialEvent::DialLongPress => {
            let action = cfg.dial_long_press.clone();
            drop(cfg);
            shared.with_executor(|exec| exec.press(&action));
            device.borrow().map.highlight(Highlight::DialLong);
        }
        DialEvent::ButtonPress(id) => {
            let action = cfg
                .buttons
                .get(&id)
                .map(|b| b.press.clone())
                .unwrap_or(Action::None);
            drop(cfg);
            shared.with_executor(|exec| exec.press(&action));
            device.borrow().map.highlight(Highlight::Button(id));
        }
        DialEvent::ButtonRelease(_) => {}
    }
}
