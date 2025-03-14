mod backend;
mod bindings;
mod font;
mod serial_tty;
mod simple_manager;
mod theme;
mod types;
mod view;

// pub use backend::settings::BackendSettings;
pub use backend::{
    BackendCommand, SerialMonitorBackend, TerminalMode, TtyEvent,
};
pub use bindings::{Binding, BindingAction, InputKind, KeyboardBinding};
pub use font::{FontSettings, TerminalFont};
pub use theme::{ColorPalette, TerminalTheme};
pub use view::SerialMonitorView;

// Modify for Serial Monitor
pub use serial_tty::{SerialTty, SerialTtyOptions};
pub use simple_manager::SimpleSerialMonitorManager;
