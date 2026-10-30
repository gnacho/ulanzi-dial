//! HUD: indicador en pantalla al girar la rueda.
//!
//! Nota Wayland: sin gtk4-layer-shell no podemos hacer un overlay real, así
//! que es una ventana pequeña sin decorar. En X11 queda flotante encima; en
//! Wayland aparece como ventana normal. Migrar a gtk4-layer-shell es el paso
//! natural cuando se empaquete.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

pub struct Hud {
    window: gtk::Window,
    action_label: gtk::Label,
    value_label: gtk::Label,
    bar: gtk::LevelBar,
    /// Generación del temporizador de ocultado. Cada `show()` la incrementa y
    /// el temporizador solo esconde el HUD si sigue siendo el suyo. Así no hay
    /// que borrar el `SourceId` (un `timeout_add_local_once` ya disparado no se
    /// puede eliminar: `SourceId::remove` hace panic).
    hide_gen: Rc<Cell<u64>>,
}

impl Hud {
    pub fn new() -> Rc<Self> {
        let window = gtk::Window::new();
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_modal(false);
        // El HUD nunca acepta el foco: en Wayland present() de una ventana
        // normal puede robarlo y, con la extensión de perfiles cargada,
        // cada cambio de foco conmuta de perfil en pleno gesto. gtk4-rs no
        // expone setters para estas dos propiedades.
        window.set_property("accept-focus", false);
        window.set_property("focus-on-map", false);
        window.add_css_class("hud-window");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
        vbox.set_margin_top(12);
        vbox.set_margin_bottom(12);
        vbox.set_margin_start(18);
        vbox.set_margin_end(18);

        let action_label = gtk::Label::new(None);
        action_label.add_css_class("dim-label");
        let value_label = gtk::Label::new(None);
        value_label.add_css_class("hud-value");
        let bar = gtk::LevelBar::for_interval(0.0, 100.0);
        bar.set_size_request(120, -1);

        vbox.append(&action_label);
        vbox.append(&value_label);
        vbox.append(&bar);
        window.set_child(Some(&vbox));

        Rc::new(Self {
            window,
            action_label,
            value_label,
            bar,
            hide_gen: Rc::new(Cell::new(0)),
        })
    }

    /// Muestra el HUD ~900 ms con la acción y el valor actual (0-100).
    pub fn show(self: &Rc<Self>, action: &str, value: i32) {
        self.action_label.set_text(action);
        self.value_label.set_text(&value.to_string());
        self.bar.set_value(value as f64);
        if !self.window.is_visible() {
            self.window.present();
        }

        let gen = self.hide_gen.get() + 1;
        self.hide_gen.set(gen);
        let hud = self.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || {
            if hud.hide_gen.get() == gen {
                hud.window.set_visible(false);
            }
        });
    }
}
