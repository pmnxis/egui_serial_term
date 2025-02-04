//! Serial TTY instead of existing PTY in alacritty_terminal.
use alacritty_terminal::event::{OnResize, WindowSize};
use std::io::{Error, ErrorKind, Result};
use std::ops::{Deref, DerefMut};

#[cfg(target_os = "macos")]
const DEFAULT_TTY_PATH: &str = "/dev/cu.usbserial-2110";

#[cfg(target_os = "linux")]
const DEFAULT_TTY_PATH: &str = "/dev/ttyUSB0";

#[cfg(target_os = "freebsd")]
const DEFAULT_TTY_PATH: &str = "/dev/cuaU0";

#[cfg(target_os = "openbsd")]
const DEFAULT_TTY_PATH: &str = "/dev/ttyU0";

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "windows",
)))]
const DEFAULT_TTY_PATH: &str = "/dev/ttyS0";

#[cfg(target_os = "windows")]
const DEFAULT_TTY_PATH: &str = "COM3";

const DEFAULT_BAUDRATE: u32 = 115200;

pub mod event_loop;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SerialTtyOptions {
    pub name: String,
    pub baud_rate: u32,
    pub data_bits: mio_serial::DataBits,
    pub flow_control: mio_serial::FlowControl,
    pub parity: mio_serial::Parity,
    pub stop_bits: mio_serial::StopBits,
    pub timeout: std::time::Duration,
}

impl Default for SerialTtyOptions {
    fn default() -> Self {
        Self {
            name: DEFAULT_TTY_PATH.to_owned(),
            baud_rate: DEFAULT_BAUDRATE,
            data_bits: mio_serial::DataBits::Eight,
            flow_control: mio_serial::FlowControl::None,
            parity: mio_serial::Parity::None,
            stop_bits: mio_serial::StopBits::One,
            timeout: std::time::Duration::from_millis(100),
        }
    }
}

impl SerialTtyOptions {
    fn in_to_builder(&self) -> mio_serial::SerialPortBuilder {
        mio_serial::new(self.name.clone(), self.baud_rate)
            .data_bits(self.data_bits)
            .flow_control(self.flow_control)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .timeout(self.timeout)
    }

    #[allow(clippy::assigning_clones)]
    #[must_use]
    pub fn set_name<'a>(
        mut self,
        path: impl Into<std::borrow::Cow<'a, str>>,
    ) -> Self {
        self.name = path.into().as_ref().to_owned();
        self
    }

    #[must_use]
    pub fn set_baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    #[must_use]
    pub fn set_data_bits(mut self, data_bits: mio_serial::DataBits) -> Self {
        self.data_bits = data_bits;
        self
    }

    #[must_use]
    pub fn set_flow_control(
        mut self,
        flow_control: mio_serial::FlowControl,
    ) -> Self {
        self.flow_control = flow_control;
        self
    }

    #[must_use]
    pub fn set_parity(mut self, parity: mio_serial::Parity) -> Self {
        self.parity = parity;
        self
    }

    #[must_use]
    pub fn set_stop_bits(mut self, stop_bits: mio_serial::StopBits) -> Self {
        self.stop_bits = stop_bits;
        self
    }

    #[must_use]
    pub fn set_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl From<&SerialTtyOptions> for mio_serial::SerialPortBuilder {
    fn from(val: &SerialTtyOptions) -> Self {
        val.in_to_builder()
    }
}

#[derive(Debug)]
pub struct SerialTty {
    stream: mio_serial::SerialStream,
}

impl Deref for SerialTty {
    type Target = mio_serial::SerialStream;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for SerialTty {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

impl OnResize for SerialTty {
    /// Resize the PTY.
    ///
    /// Tells the kernel that the window size changed with the new pixel
    /// dimensions and line/column counts.
    fn on_resize(&mut self, _window_size: WindowSize) {
        // serial tty do nothing for window size event
        // but in future, there's possibility to do something
    }
}

/// Create a new TTY and return a handle to interact with it.
pub fn new(
    config: &SerialTtyOptions,
    _window_size: WindowSize,
    _window_id: u64,
) -> Result<SerialTty> {
    if let Ok(ports) = mio_serial::available_ports() {
        if let Some(matched) = ports.iter().find(|x| x.port_name == config.name)
        {
            match &matched.port_type {
                mio_serial::SerialPortType::UsbPort(u) => {
                    println!(
                        "mfn : {}",
                        u.manufacturer.clone().unwrap_or("".to_owned())
                    );
                },
                _ => {},
            }
        } else {
            println!("Not Found");
        }

        let stream = mio_serial::SerialStream::open(&config.in_to_builder())?;

        Ok(SerialTty { stream })
    } else {
        Err(Error::new(ErrorKind::InvalidData, "Unknown SerialTty Call"))
    }
}
