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
use std::time::{Duration, Instant};

use async_channel::Sender;
use evdev::{Device, EventType, InputEvent, Key};

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
        let axes = device.supported_absolute_axes()?;
        for axis in axes.iter() {
            if let Ok(state) = device.get_abs_state() {
                // state es un vector indexado por código de eje
                let idx = axis.0 as usize;
                if let Some(info) = state.get(idx) {
                    let value = info.value.clamp(0, 255) as u32;
                    return Some((value * 100 / 255) as u8);
                }
            }
        }
    }
    None
}

pub struct ReaderConfig {
    pub keymap: KeyMap,
    pub long_press_ms: u64,
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
    let epoch = Instant::now();
    let mut next_rescan = Instant::now();

    loop {
        let Some((path, mut device)) = find_device() else {
            if next_rescan <= Instant::now() {
                next_rescan = Instant::now() + RESCAN_INTERVAL;
            }
            thread::sleep(Duration::from_millis(100));
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

        let mut decoder = Decoder::new(cfg.keymap.clone(), cfg.long_press_ms);

        // Bucle de lectura hasta que el dispositivo desaparece o da error.
        'read: loop {
            match device.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        handle_raw(ev, epoch, &mut decoder, &tx);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    tracing::info!("dispositivo perdido: {e}");
                    break 'read;
                }
            }

            // Pulsación larga y detección de desconexión por desaparición del nodo.
            if let Some(long) = decoder.tick(epoch.elapsed().as_millis() as u64) {
                let _ = tx.send_blocking(BackendMsg::Event(long));
            }
            if !Path::new(&path).exists() {
                break 'read;
            }
            thread::sleep(POLL_INTERVAL);
        }

        let _ = device.ungrab();
        let _ = tx.send_blocking(BackendMsg::Disconnected);
    }
}

fn handle_raw(ev: InputEvent, epoch: Instant, decoder: &mut Decoder, tx: &Sender<BackendMsg>) {
    if ev.event_type() != EventType::KEY {
        return;
    }
    let raw = RawKeyEvent {
        key: Key(ev.code()),
        value: ev.value(),
        time_ms: epoch.elapsed().as_millis() as u64,
    };
    for gesture in decoder.feed(raw) {
        let _ = tx.send_blocking(BackendMsg::Event(gesture));
    }
}
