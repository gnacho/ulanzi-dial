//! Detección de la aplicación en primer plano.
//!
//! GNOME no permite a un proceso normal saber qué ventana tiene el foco
//! (`org.gnome.Shell.Introspect.GetWindows` responde `AccessDenied` y Mutter no
//! implementa `wlr-foreign-toplevel-management`), así que la información llega
//! desde la extensión de `gnome-extension/`: ella ve `notify::focus-window` y
//! emite la señal `Changed(s)` en cuanto cambia el foco.
//!
//! Aquí solo hay una consulta al arrancar, para saber con qué aplicación
//! empieza la sesión. A partir de ahí no se pregunta nada más: la app se queda
//! dormida esperando la señal.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;

const BUS_NAME: &str = "dev.ulanzi.Dial.Focus";
const OBJECT_PATH: &str = "/dev/ulanzi/Dial/Focus";
const INTERFACE: &str = "dev.ulanzi.Dial.Focus";

thread_local! {
    /// La suscripción se libera sola al soltarla, así que se guarda aquí para
    /// que viva tanto como el proceso.
    static SUBSCRIPTION: RefCell<Option<gio::SignalSubscriptionId>> = const { RefCell::new(None) };
}

/// Escucha los cambios de ventana activa y llama a `on_change` con el
/// identificador de la aplicación. Si la extensión no está, recibe `None`.
pub fn spawn_watcher(on_change: impl Fn(Option<String>) + 'static) {
    let connection = match gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) {
        Ok(connection) => connection,
        Err(err) => {
            tracing::warn!("sin bus de sesión: {err}");
            return;
        }
    };

    // Estado de partida: una única consulta, y solo si la extensión responde.
    let current = Rc::new(RefCell::new(query(&connection)));
    on_change(current.borrow().clone());

    let notify = Rc::new(on_change);
    let subscription = connection.signal_subscribe(
        None,
        Some(INTERFACE),
        Some("Changed"),
        Some(OBJECT_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |_connection, _sender, _path, _interface, _signal, params| {
            let Some((raw,)) = params.get::<(String,)>() else {
                return;
            };
            let next = if raw.is_empty() { None } else { Some(raw) };
            tracing::info!(app = ?next, "ventana en primer plano");
            if *current.borrow() == next {
                return;
            }
            *current.borrow_mut() = next.clone();
            notify(next);
        },
    );

    SUBSCRIPTION.with(|slot| *slot.borrow_mut() = Some(subscription));
}

/// Pregunta una vez por la aplicación activa. Cadena vacía o error -> `None`.
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
    let (id,) = reply.get::<(String,)>()?;
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
