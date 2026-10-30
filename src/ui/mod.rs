//! Ensamblado de la ventana principal y cableado backend ↔ UI.

pub mod chord_dialog;
pub mod control_map;
pub mod device_page;
pub mod general_page;
pub mod hud;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use crate::backend::actions::{Action, ActionExecutor};
use crate::backend::decoder::DialEvent;
use crate::backend::device::{self, BackendMsg, ReaderConfig};
use crate::config::{Config, VolumeMode};
use control_map::Highlight;

/// Estado compartido entre la UI y el reenviador de eventos del backend.
struct Shared {
    config: Rc<RefCell<Config>>,
    executor: RefCell<Option<ActionExecutor>>,
    /// Valor sintético 0-100 para el HUD (volumen/brillo/zoom).
    hud_value: Cell<i32>,
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
        overlay: overlay.clone(),
        banner: banner.clone(),
    });

    // --- páginas ---
    let toast_cb = {
        let shared = shared.clone();
        move |msg: String| shared.toast(msg)
    };
    let device = device_page::DevicePage::new(shared.config.clone(), {
        let shared = shared.clone();
        move |msg| shared.toast(msg)
    });
    stack.add_titled(&device.borrow().widget, Some("device"), "Dispositivo");

    // Perfiles por aplicación: la extensión de GNOME publica la ventana activa.
    {
        let shared = shared.clone();
        let device = device.clone();
        crate::backend::focus::spawn_watcher(move |app| {
            let changed = shared.config.borrow_mut().set_active_app(app);
            if !changed {
                return;
            }
            device.borrow().map.refresh();
            let key = shared.config.borrow().current_profile_key().cloned();
            match key {
                Some(name) => shared.toast(format!("Perfil: {name}")),
                None => shared.toast("Perfil global".to_string()),
            }
        });
    }

    let general = general_page::build(shared.config.clone(), toast_cb, {
        let device = device.clone();
        move || device.borrow().map.refresh()
    });
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
}

fn handle_event(
    shared: &Rc<Shared>,
    device: &Rc<RefCell<device_page::DevicePage>>,
    hud: &Rc<hud::Hud>,
    ev: DialEvent,
) {
    let cfg = shared.config.borrow();

    match ev {
        DialEvent::Rotate { steps } => {
            let action = cfg.dial_rotate.clone();
            let detents = if cfg.invert { -steps } else { steps };
            // La sensibilidad solo tiene sentido en las acciones continuas:
            // aplicada al volumen multiplicaba cada muesca por cinco.
            let continuous = matches!(action, Action::Scroll | Action::HScroll | Action::Zoom);
            let amount = if continuous {
                detents * cfg.sensitivity as i32
            } else {
                detents
            };
            let volume_step = cfg.volume_step;
            let volume_mode = cfg.volume_mode;
            let show_hud = cfg.show_hud;
            drop(cfg);

            let measured = shared
                .with_executor(|exec| exec.rotate(&action, amount, volume_mode, volume_step))
                .flatten();

            // El escritorio ya pinta su propio OSD cuando mandamos las teclas
            // de volumen o brillo, así que el HUD solo hace falta cuando el
            // valor no lo muestra nadie (zoom y volumen por mezclador).
            let wants_hud = show_hud
                && match action {
                    Action::Zoom => true,
                    Action::Volume => volume_mode == VolumeMode::Mixer,
                    _ => false,
                };
            if wants_hud {
                let v = match measured {
                    // Volumen: el valor real que devuelve el mezclador.
                    Some(real) => real as i32,
                    None => {
                        // Brillo del escritorio: salta de 5 en 5. El zoom es
                        // un contador local sin medida real.
                        let delta = match action {
                            Action::Brightness => amount * 5,
                            _ => amount,
                        };
                        (shared.hud_value.get() + delta).clamp(0, 100)
                    }
                };
                shared.hud_value.set(v);
                hud.show(&action.label(), v);
            }
            device.borrow().map.highlight(Highlight::DialRotate);
        }
        DialEvent::DialPress => {
            let action = cfg.dial_press.clone();
            drop(cfg);
            shared.with_executor(|exec| exec.press(&action));
            device.borrow().map.highlight(Highlight::DialPress);
        }
        DialEvent::ButtonPress(id) => {
            let action = cfg.press_for(id);
            drop(cfg);
            shared.with_executor(|exec| exec.press(&action));
            device.borrow().map.highlight(Highlight::Button(id));
        }
        DialEvent::ButtonRelease(_) => {}
    }
}
