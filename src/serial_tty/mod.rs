//! Serial TTY instead of existing PTY in alacritty_terminal.

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;

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
    target_os = "openbsd"
)))]
const DEFAULT_TTY_PATH: &str = "/dev/ttyS0";

const DEFAULT_BAUDRATE: u32 = 115200;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SerialTtyOptions {
    pub name: String,
    pub baud_rate: u32,
    pub data_bits: serialport::DataBits,
    pub flow_control: serialport::FlowControl,
    pub parity: serialport::Parity,
    pub stop_bits: serialport::StopBits,
    pub timeout: std::time::Duration,
}

impl Default for SerialTtyOptions {
    fn default() -> Self {
        Self {
            name: DEFAULT_TTY_PATH.to_owned(),
            baud_rate: DEFAULT_BAUDRATE,
            data_bits: serialport::DataBits::Eight,
            flow_control: serialport::FlowControl::None,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
            timeout: std::time::Duration::from_millis(100),
        }
    }
}

impl SerialTtyOptions {
    fn in_to_builder(&self) -> serialport::SerialPortBuilder {
        serialport::new(self.name.clone(), self.baud_rate)
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
    pub fn set_data_bits(mut self, data_bits: serialport::DataBits) -> Self {
        self.data_bits = data_bits;
        self
    }

    #[must_use]
    pub fn set_flow_control(
        mut self,
        flow_control: serialport::FlowControl,
    ) -> Self {
        self.flow_control = flow_control;
        self
    }

    #[must_use]
    pub fn set_parity(mut self, parity: serialport::Parity) -> Self {
        self.parity = parity;
        self
    }

    #[must_use]
    pub fn set_stop_bits(mut self, stop_bits: serialport::StopBits) -> Self {
        self.stop_bits = stop_bits;
        self
    }

    #[must_use]
    pub fn set_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl From<&SerialTtyOptions> for serialport::SerialPortBuilder {
    fn from(val: &SerialTtyOptions) -> Self {
        val.in_to_builder()
    }
}
