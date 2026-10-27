//! Pestaña "General": opciones genéricas (ruleta, comportamiento, perfil).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use crate::config::Config;

pub fn build(
    config: Rc<RefCell<Config>>,
    on_toast: impl Fn(String) + 'static,
    on_profile_changed: impl Fn() + 'static,
) -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_margin_start(18);
    page.set_margin_end(18);
    page.set_margin_top(6);
    page.set_margin_bottom(18);

    let clamp = adw::Clamp::new();
    let groups = gtk::Box::new(gtk::Orientation::Vertical, 18);
    clamp.set_child(Some(&groups));
    page.append(&clamp);

    let on_toast = Rc::new(on_toast);

    // ---------- Ruleta ----------
    let grp_dial = adw::PreferencesGroup::new();
    grp_dial.set_title("Ruleta");

    let sens = adw::SpinRow::new(
        Some(&gtk::Adjustment::new(5.0, 1.0, 10.0, 1.0, 0.0, 0.0)),
        1.0,
        0,
    );
    sens.set_title("Sensibilidad");
    sens.set_subtitle(
        "Solo se aplica a desplazamiento y zoom. El volumen y el brillo van de una en una",
    );
    sens.set_value(config.borrow().sensitivity as f64);
    {
        let config = config.clone();
        sens.adjustment().connect_value_changed(move |adj| {
            let mut c = config.borrow_mut();
            c.sensitivity = adj.value() as u8;
            c.save();
        });
    }
    grp_dial.add(&sens);

    let invert = adw::SwitchRow::new();
    invert.set_title("Invertir dirección");
    invert.set_subtitle("Girar a la izquierda aumenta en lugar de reducir");
    invert.set_active(config.borrow().invert);
    {
        let config = config.clone();
        invert.connect_active_notify(move |row| {
            let mut c = config.borrow_mut();
            c.invert = row.is_active();
            c.save();
        });
    }
    grp_dial.add(&invert);

    groups.append(&grp_dial);

    // ---------- Comportamiento ----------
    let grp_beh = adw::PreferencesGroup::new();
    grp_beh.set_title("Comportamiento");

    let hud = adw::SwitchRow::new();
    hud.set_title("Mostrar HUD al girar");
    hud.set_subtitle("Solo sale con el zoom y con el volumen en modo mezclador: el resto ya lo muestra el escritorio con su propio indicador");
    hud.set_active(config.borrow().show_hud);
    {
        let config = config.clone();
        hud.connect_active_notify(move |row| {
            let mut c = config.borrow_mut();
            c.show_hud = row.is_active();
            c.save();
        });
    }
    grp_beh.add(&hud);

    let grab = adw::SwitchRow::new();
    grab.set_title("Captura exclusiva del dial");
    grab.set_subtitle("Las teclas multimedia del D100H no llegan a otras aplicaciones");
    grab.set_active(config.borrow().exclusive_grab);
    {
        let config = config.clone();
        let toast = on_toast.clone();
        grab.connect_active_notify(move |row| {
            let mut c = config.borrow_mut();
            c.exclusive_grab = row.is_active();
            c.save();
            toast("Se aplica al próximo reconectar del dial".into());
        });
    }
    grp_beh.add(&grab);

    let autostart = adw::SwitchRow::new();
    autostart.set_title("Arrancar al iniciar sesión");
    autostart.set_subtitle("El servicio se inicia con tu sesión de escritorio");
    autostart.set_active(config.borrow().autostart);
    {
        let config = config.clone();
        autostart.connect_active_notify(move |row| {
            let enabled = row.is_active();
            config.borrow_mut().autostart = enabled;
            config.borrow().save();
            set_autostart(enabled);
        });
    }
    grp_beh.add(&autostart);
    groups.append(&grp_beh);

    // ---------- Perfil ----------
    let grp_profile = adw::PreferencesGroup::new();
    grp_profile.set_title("Perfil");
    let profile_row = adw::ComboRow::new();
    profile_row.set_title("Aplicación");
    profile_row.set_subtitle(
        "Global usa las asignaciones de la pestaña Dispositivo. Con una aplicación elegida, \
         los cambios se guardan solo en su perfil.",
    );

    let mut labels: Vec<String> = vec!["Global".to_string()];
    let mut keys: Vec<String> = Vec::new();
    {
        let c = config.borrow();
        let mut names: Vec<&String> = c.profiles.keys().collect();
        names.sort();
        for name in names {
            labels.push(name.clone());
            keys.push(name.clone());
        }
    }
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let model = gtk::StringList::new(&refs);
    profile_row.set_model(Some(&model));

    let pinned = config.borrow().pinned_profile.clone();
    let selected = pinned
        .and_then(|p| keys.iter().position(|k| *k == p))
        .map(|i| i as u32 + 1)
        .unwrap_or(0);
    profile_row.set_selected(selected);
    {
        let toast = on_toast.clone();
        let refresh = on_profile_changed;
        profile_row.connect_selected_notify(move |row| {
            let idx = row.selected();
            let key = if idx == 0 {
                None
            } else {
                keys.get(idx as usize - 1).cloned()
            };
            let mut c = config.borrow_mut();
            c.pinned_profile = key.clone();
            c.save();
            drop(c);
            refresh();
            match key {
                Some(name) => toast(format!("Perfil fijado: {name}")),
                None => toast("Perfil global".to_string()),
            }
        });
    }
    grp_profile.add(&profile_row);
    groups.append(&grp_profile);

    page.upcast()
}

/// Crea/elimina el .desktop de autostart en ~/.config/autostart.
fn set_autostart(enabled: bool) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::Path::new(&home).join(".config/autostart");
    let file = dir.join("ulanzi-dial.desktop");
    if enabled {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            &file,
            concat!(
                "[Desktop Entry]\n",
                "Type=Application\n",
                "Name=Ulanzi Dial\n",
                "Exec=ulanzi-dial\n",
                "X-GNOME-Autostart-enabled=true\n",
            ),
        );
    } else {
        let _ = std::fs::remove_file(&file);
    }
}
