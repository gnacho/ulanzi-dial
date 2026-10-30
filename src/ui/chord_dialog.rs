//! Diálogo para grabar un atajo de teclado.
//!
//! El atajo se compone con el estado de los modificadores en el momento de la
//! pulsación y se valida con `parse_combo`, la misma función que usa el backend
//! al ejecutar la acción. Así lo que se ve en el diálogo es exactamente lo que
//! se va a emitir.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4 as gtk;

use crate::backend::keymap::{named_key, parse_combo};

/// Atajos conocidos de GNOME. No dejan de ser combinaciones de teclas: se
/// ofrecen como atajo para no tener que recordarlos al configurar.
pub const PRESETS: &[(&str, &str)] = &[
    ("Nueva pestaña", "Ctrl+T"),
    ("Nueva ventana", "Ctrl+N"),
    ("Cerrar pestaña", "Ctrl+W"),
    ("Reabrir pestaña cerrada", "Ctrl+Shift+T"),
    ("Pestaña siguiente", "Ctrl+Tab"),
    ("Pestaña anterior", "Ctrl+Shift+Tab"),
    ("Pantalla completa", "F11"),
    ("Maximizar ventana", "Super+Up"),
    ("Restaurar ventana", "Super+Down"),
    ("Ocultar ventana", "Super+H"),
    ("Captura de pantalla", "Print"),
    ("Captura de ventana", "Alt+Print"),
];

/// Abre el diálogo. `current` es el atajo actual, si la acción ya era un atajo.
pub fn present(
    parent: Option<&gtk::Window>,
    current: Option<&str>,
    on_accept: impl Fn(String) + 'static,
) -> gtk::Window {
    let window = gtk::Window::builder()
        .title("Atajo de teclado")
        .modal(true)
        .resizable(false)
        .default_width(460)
        .build();
    if let Some(parent) = parent {
        window.set_transient_for(Some(parent));
    }

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 12);
    outer.set_margin_top(18);
    outer.set_margin_bottom(18);
    outer.set_margin_start(18);
    outer.set_margin_end(18);
    window.set_child(Some(&outer));

    let hint = gtk::Label::new(Some("Pulsa la combinación que quieras enviar."));
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    outer.append(&hint);

    let captured = gtk::Label::new(Some(current.unwrap_or("Sin asignar")));
    captured.add_css_class("title-1");
    captured.set_ellipsize(gtk::pango::EllipsizeMode::End);
    captured.set_margin_top(6);
    captured.set_margin_bottom(6);
    outer.append(&captured);

    let pending = gtk::Label::new(None);
    pending.set_xalign(0.0);
    pending.add_css_class("dim-label");
    pending.set_height_request(18);
    outer.append(&pending);

    let preset_labels: Vec<&str> = PRESETS.iter().map(|(label, _)| *label).collect();
    let presets = gtk::DropDown::from_strings(&preset_labels);
    presets.set_tooltip_text(Some("Atajos habituales de GNOME"));
    // Sin seleccion inicial: el atajo actual puede no ser ninguno de los presets.
    presets.set_selected(gtk::INVALID_LIST_POSITION);
    outer.append(&presets);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancelar");
    let save = gtk::Button::with_label("Guardar");
    save.add_css_class("suggested-action");
    save.set_sensitive(current.map(|c| parse_combo(c).is_some()).unwrap_or(false));
    buttons.append(&cancel);
    buttons.append(&save);
    outer.append(&buttons);

    let chosen: Rc<std::cell::RefCell<Option<String>>> =
        Rc::new(std::cell::RefCell::new(current.map(str::to_string)));

    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let window = window.clone();
        let captured = captured.clone();
        let pending = pending.clone();
        let save = save.clone();
        let chosen = chosen.clone();
        controller.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gdk::Key::Escape && state.is_empty() {
                window.close();
                return glib::Propagation::Stop;
            }
            match combo_from_event(keyval, state) {
                Some(combo) => {
                    captured.set_text(&combo);
                    pending.set_text("");
                    save.set_sensitive(parse_combo(&combo).is_some());
                    *chosen.borrow_mut() = Some(combo);
                }
                None => {
                    // Solo hay modificadores de momento: se muestra lo que
                    // llevas pulsado y se sigue esperando la tecla.
                    pending.set_text(&modifier_preview(state));
                }
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(controller);

    {
        let captured = captured.clone();
        let save = save.clone();
        let chosen = chosen.clone();
        presets.connect_selected_notify(move |dd| {
            let idx = dd.selected();
            if idx == gtk::INVALID_LIST_POSITION {
                return;
            }
            if let Some((_, chord)) = PRESETS.get(idx as usize) {
                captured.set_text(chord);
                save.set_sensitive(true);
                *chosen.borrow_mut() = Some((*chord).to_string());
            }
        });
    }

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }

    {
        let window = window.clone();
        let chosen = chosen.clone();
        save.connect_clicked(move |_| {
            if let Some(chord) = chosen.borrow().clone() {
                if parse_combo(&chord).is_some() {
                    on_accept(chord);
                    window.close();
                }
            }
        });
    }

    window.present();
    window
}

fn modifier_preview(state: gdk::ModifierType) -> String {
    let mut parts = Vec::new();
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl");
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift");
    }
    if state.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("Alt");
    }
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        parts.push("Super");
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{}+…", parts.join("+"))
    }
}

/// Traduce la pulsación de GTK al vocabulario de `parse_combo`. Devuelve `None`
/// cuando lo pulsado es solo un modificador.
fn combo_from_event(keyval: gdk::Key, state: gdk::ModifierType) -> Option<String> {
    let raw = keyval.name()?;
    let name = raw.as_str();
    let token = match name {
        "Control_L" | "Control_R" | "Shift_L" | "Shift_R" | "Alt_L" | "Alt_R" | "Meta_L"
        | "Meta_R" | "Super_L" | "Super_R" | "Caps_Lock" | "Num_Lock" | "ISO_Level3_Shift" => {
            return None;
        }
        "Return" | "KP_Enter" => "Enter",
        "Escape" => "Esc",
        "BackSpace" => "Backspace",
        "Tab" | "ISO_Left_Tab" => "Tab",
        "space" => "Space",
        "Delete" => "Delete",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "Page_Up" => "PageUp",
        "Page_Down" => "PageDown",
        "Left" => "Left",
        "Right" => "Right",
        "Up" => "Up",
        "Down" => "Down",
        "Print" | "Sys_Req" => "Print",
        "Menu" => "Menu",
        "minus" => "Minus",
        "equal" => "Equal",
        "plus" => "Plus",
        "comma" => "Comma",
        "period" => "Dot",
        "slash" => "Slash",
        "semicolon" => "Semicolon",
        "apostrophe" => "Apostrophe",
        "grave" => "Grave",
        "bracketleft" => "LeftBrace",
        "bracketright" => "RightBrace",
        "backslash" => "Backslash",
        "XF86AudioPlay" | "XF86AudioPause" => "PlayPause",
        "XF86AudioNext" => "NextSong",
        "XF86AudioPrev" => "PreviousSong",
        "XF86AudioRaiseVolume" => "VolumeUp",
        "XF86AudioLowerVolume" => "VolumeDown",
        "XF86AudioMute" => "Mute",
        "KP_Decimal" | "KP_Separator" => "KPDOT",
        "KP_Add" => "KPPLUS",
        "KP_Subtract" => "KPMINUS",
        "KP_Multiply" => "KPASTERISK",
        "KP_Divide" => "KPSLASH",
        "KP_Equal" => "KPEQUAL",
        other => {
            let mut chars = other.chars();
            let first = chars.next()?;
            if chars.next().is_some() {
                // Nombres con más de un carácter: F11 llega así y vale;
                // "KP_1" (teclado numérico) y "XF86Eject" (media) se
                // aceptan si la tabla de teclas los conoce.
                if other.starts_with('F') && other[1..].chars().all(|c| c.is_ascii_digit()) {
                    return Some(with_modifiers(other, state));
                }
                if let Some(kp) = other.strip_prefix("KP_") {
                    let token = format!("KP{}", kp.to_ascii_uppercase());
                    if named_key(&token).is_some() {
                        return Some(with_modifiers(&token, state));
                    }
                }
                if let Some(rest) = other.strip_prefix("XF86") {
                    let token = rest.to_ascii_uppercase();
                    if named_key(&token).is_some() {
                        return Some(with_modifiers(&token, state));
                    }
                }
                return None;
            }
            if first.is_ascii_alphabetic() {
                return Some(with_modifiers(
                    &first.to_ascii_uppercase().to_string(),
                    state,
                ));
            }
            if first.is_ascii_digit() {
                return Some(with_modifiers(&first.to_string(), state));
            }
            return None;
        }
    };
    Some(with_modifiers(token, state))
}

fn with_modifiers(key: &str, state: gdk::ModifierType) -> String {
    let mut parts = Vec::new();
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".to_string());
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift".to_string());
    }
    if state.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("Alt".to_string());
    }
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        parts.push("Super".to_string());
    }
    parts.push(key.to_string());
    parts.join("+")
}
