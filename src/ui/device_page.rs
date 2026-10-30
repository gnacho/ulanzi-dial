//! Pestaña "Dispositivo": tarjeta de estado + mapa de controles.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use super::control_map::ControlMap;
use crate::config::Config;

pub struct DevicePage {
    pub widget: gtk::Box,
    pub map: ControlMap,
    status_subtitle: gtk::Label,
    connected: bool,
    battery: Option<u8>,
}

impl DevicePage {
    pub fn new(
        config: Rc<RefCell<Config>>,
        on_toast: impl Fn(String) + 'static,
    ) -> Rc<RefCell<Self>> {
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.set_margin_start(18);
        page.set_margin_end(18);
        page.set_margin_top(18);
        page.set_margin_bottom(18);

        // --- tarjeta de estado ---
        let status_group = adw::PreferencesGroup::new();
        let row = adw::ActionRow::new();
        row.set_title("Ulanzi Dial D100H");
        let icon = gtk::Image::from_icon_name("input-keyboard-symbolic");
        icon.set_pixel_size(28);
        row.add_prefix(&icon);

        let subtitle = gtk::Label::new(Some("Buscando el dial…"));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        row.add_suffix(&subtitle);

        status_group.add(&row);
        page.append(&status_group);

        // --- cabecera del mapa ---
        let title = gtk::Label::new(Some("Distribución de controles"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_margin_top(18);
        title.set_margin_start(12);
        page.append(&title);

        let desc = gtk::Label::new(Some(
            "Pulsar es la acción del botón. La rueda siempre hace lo suyo: el firmware \
             no manda giros mientras mantienes un botón. Los cambios se guardan al instante.",
        ));
        desc.set_xalign(0.0);
        desc.set_wrap(true);
        desc.add_css_class("dim-label");
        desc.set_margin_start(12);
        page.append(&desc);

        // --- mapa ---
        let map = ControlMap::new(config, on_toast);
        page.append(&map.widget);

        Rc::new(RefCell::new(Self {
            widget: page,
            map,
            status_subtitle: subtitle,
            connected: false,
            battery: None,
        }))
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
        self.refresh_subtitle();
    }

    pub fn set_battery(&mut self, pct: u8) {
        self.battery = Some(pct);
        self.refresh_subtitle();
    }

    fn refresh_subtitle(&self) {
        let text = if self.connected {
            match self.battery {
                Some(pct) => format!("Conectado por Bluetooth · Batería {pct} %"),
                None => "Conectado por Bluetooth".to_string(),
            }
        } else {
            "Desconectado".to_string()
        };
        self.status_subtitle.set_text(&text);
    }
}
