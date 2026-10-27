//! Backend de entrada: lectura del D100H por evdev y ejecución de acciones.
//!
//! Arquitectura (Linux):
//!   /dev/input/event*  →  device::DeviceManager  →  decoder  →  DialEvent
//!   DialEvent → actions::ActionExecutor (uinput virtual) → sistema

pub mod actions;
pub mod decoder;
pub mod device;
pub mod keymap;
