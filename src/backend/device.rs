//! Localización, apertura exclusiva y lectura del D100H por evdev.
//!
//! - Identifica el dispositivo por nombre (`Ulanzi Dial Keyboard`): el VID/PID
//!   `FFF1:0082` es un placeholder genérico y no sirve para identificarlo.
//! - `grab()` aplica EVIOCGRAB: las teclas multimedia del firmware no llegan
//!   al resto de aplicaciones mientras la app está activa.
//! - Hotplug por reescaneo periódico de /dev/input (sencillo y robusto; no
//!   requiere udev ni tokio).

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use async_channel::Sender;
use evdev::{AbsoluteAxisType, Device, EventType, InputEvent, Key};

use super::decoder::{Decoder, DialEvent, RawKeyEvent};
use super::keymap::KeyMap;

/// Mensajes del hilo lector hacia la UI.
#[derive(Clone, Debug)]
pub enum BackendMsg {
    Event(DialEvent),
    Connected(String),
    Disconnected,
    /// Nivel de batería 0-100 si el nodo ABS_MISC hermano se puede leer.
    Battery(u8),
}

const DEVICE_NAME_PREFIX: &str = "Ulanzi";
const RESCAN_INTERVAL: Duration = Duration::from_secs(1);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// La batería no cambia a la vista: con un refresco por minuto sobra.
const BATTERY_INTERVAL: Duration = Duration::from_secs(60);

/// Busca el nodo evdev que lleva los eventos de teclado del dial.
pub fn find_device() -> Option<(PathBuf, Device)> {
    for (path, device) in evdev::enumerate() {
        if let Some(name) = device.name() {
            // El nodo útil es el de teclado; los nodos "Mouse" y los dos
            // sensores ABS_MISC no emiten eventos de rueda/botones.
            if name.starts_with(DEVICE_NAME_PREFIX) && name.contains("Keyboard") {
                return Some((path, device));
            }
        }
    }
    None
}

/// Lectura best-effort de la batería desde el nodo ABS_MISC hermano.
/// El firmware la expone como un eje absoluto con rango 0-255.
pub fn read_battery() -> Option<u8> {
    for (_path, device) in evdev::enumerate() {
        let Some(name) = device.name() else { continue };
        if !name.starts_with(DEVICE_NAME_PREFIX) || name.contains("Keyboard") {
            continue;
        }
        // Solo el eje ABS_MISC es la batería; tomar el primer eje del nodo
        // leía valores ajenos si el firmware añade ejes nuevos.
        let Some(axes) = device.supported_absolute_axes() else {
            continue;
        };
        if !axes.contains(AbsoluteAxisType::ABS_MISC) {
            continue;
        }
        let Ok(state) = device.get_abs_state() else {
            continue;
        };
        // get_abs_state devuelve un array indexado por código de eje.
        let raw = state[AbsoluteAxisType::ABS_MISC.0 as usize].value;
        return Some(battery_percent(raw));
    }
    None
}

/// ABS_MISC va de 0 a 255; fuera de rango se satura, nunca se envuelve.
fn battery_percent(raw: i32) -> u8 {
    let clamped = raw.clamp(0, 255);
    ((clamped as u32) * 100 / 255) as u8
}

pub struct ReaderConfig {
    pub keymap: KeyMap,
    /// Si es true, aplica EVIOCGRAB al abrir el dispositivo.
    pub exclusive_grab: bool,
}

/// Lanza el hilo lector. Devuelve un handle para poder desconectarlo al salir.
pub fn spawn_reader(cfg: ReaderConfig, tx: Sender<BackendMsg>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("ulanzi-dial-reader".into())
        .spawn(move || reader_loop(cfg, tx))
        .expect("no se pudo crear el hilo lector")
}

fn reader_loop(cfg: ReaderConfig, tx: Sender<BackendMsg>) {
    loop {
        let Some((path, mut device)) = find_device() else {
            // Sin dispositivo: reescaneo periódico hasta que aparezca.
            thread::sleep(RESCAN_INTERVAL);
            continue;
        };

        let name = device.name().unwrap_or("Ulanzi Dial").to_string();
        tracing::info!(?path, %name, "dial encontrado");

        if cfg.exclusive_grab {
            if let Err(e) = device.grab() {
                tracing::warn!("EVIOCGRAB falló: {e} (¿permisos? ver README, regla udev)");
            }
        }

        // evdev 0.12 no expone set_nonblocking: fcntl directo sobre el fd.
        {
            use std::os::unix::io::AsRawFd;
            let fd = device.as_raw_fd();
            // SAFETY: fd válido y abierto; F_SETFL con O_NONBLOCK es seguro.
            if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } == -1 {
                tracing::warn!("no se pudo poner el fd en non-blocking");
            }
        }

        let _ = tx.send_blocking(BackendMsg::Connected(name));
        if let Some(pct) = read_battery() {
            let _ = tx.send_blocking(BackendMsg::Battery(pct));
        }
        let mut last_battery = std::time::Instant::now();

        let mut decoder = Decoder::new(cfg.keymap.clone());

        // Bucle de lectura hasta que el dispositivo desaparece o da error.
        'read: loop {
            match device.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        handle_raw(ev, &mut decoder, &tx);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    tracing::info!("dispositivo perdido: {e}");
                    break 'read;
                }
            }

            // Detección de desconexión por desaparición del nodo.
            if !Path::new(&path).exists() {
                break 'read;
            }
            // La batería se refresca en sesión: al conectar solo se lee una
            // vez y el nivel quedaba congelado hasta reconectar.
            if last_battery.elapsed() >= BATTERY_INTERVAL {
                last_battery = std::time::Instant::now();
                if let Some(pct) = read_battery() {
                    let _ = tx.send_blocking(BackendMsg::Battery(pct));
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        let _ = device.ungrab();
        let _ = tx.send_blocking(BackendMsg::Disconnected);
    }
}

fn handle_raw(ev: InputEvent, decoder: &mut Decoder, tx: &Sender<BackendMsg>) {
    if ev.event_type() != EventType::KEY {
        return;
    }
    let raw = RawKeyEvent {
        key: Key(ev.code()),
        value: ev.value(),
        time_ms: 0,
    };
    for gesture in decoder.feed(raw) {
        let _ = tx.send_blocking(BackendMsg::Event(gesture));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_percent_scales_and_saturates() {
        assert_eq!(battery_percent(0), 0);
        assert_eq!(battery_percent(255), 100);
        assert_eq!(battery_percent(128), 50);
        assert_eq!(battery_percent(-5), 0);
        assert_eq!(battery_percent(300), 100);
    }
}
