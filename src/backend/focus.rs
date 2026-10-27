//! Watcher del identificador de la aplicación activa.
//!
//! GNOME no deja a un proceso normal saber qué ventana tiene el foco
//! (`org.gnome.Shell.Introspect.GetWindows` responde AccessDenied y Mutter no
//! implementa `wlr-foreign-toplevel-management`). La vía soportada es la
//! extensión propia de `gnome-extension/`, que publica el app-id en el bus de
//! sesión y emite `Changed`. Aquí se consulta por sondeo: un `timeout` de un
//! segundo es barato y no necesita maquinaria de señales asíncronas.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::glib;

const BUS_NAME: &str = "dev.ulanzi.Dial.Focus";
const OBJECT_PATH: &str = "/dev/ulanzi/Dial/Focus";
const INTERFACE: &str = "dev.ulanzi.Dial.Focus";
const POLL: Duration = Duration::from_millis(1000);

/// Vigila la ventana activa y llama a `on_change` solo cuando cambia.
/// Si la extensión no está instalada o habilitada, no avisa de nada.
pub fn spawn_watcher(on_change: impl Fn(Option<String>) + 'static) {
    let connection = match gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) {
        Ok(connection) => connection,
        Err(e) => {
            tracing::warn!("sin bus de sesión, sin perfiles por app: {e}");
            return;
        }
    };

    let last: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let on_change = Rc::new(on_change);

    glib::timeout_add_local(POLL, move || {
        let app = query(&connection);
        let mut slot = last.borrow_mut();
        if *slot != app {
            *slot = app.clone();
            on_change(app);
        }
        glib::ControlFlow::Continue
    });
}

/// Consulta el app-id actual. `None` si la extensión no responde.
fn query(connection: &gio::DBusConnection) -> Option<String> {
    let reply = connection
        .call_sync(
            Some(BUS_NAME),
            OBJECT_PATH,
            INTERFACE,
            "Get",
            None,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            200,
            None::<&gio::Cancellable>,
        )
        .ok()?;
    let id: String = reply.get::<(String,)>()?.0;
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
