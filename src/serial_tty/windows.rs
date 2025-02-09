//! This is fixing open COM port with FILE_FLAG_OVERLAPPED , not FILE_ATTRIBUTE_NORMAL

/// Copied from `src/windows/error.rs` in serialport-4.7.0
mod error {
    use std::io;
    use std::ptr;

    use winapi::shared::minwindef::DWORD;
    use winapi::shared::winerror::*;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::winbase::{
        FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM,
        FORMAT_MESSAGE_IGNORE_INSERTS,
    };
    use winapi::um::winnt::{
        LANG_SYSTEM_DEFAULT, MAKELANGID, SUBLANG_SYS_DEFAULT, WCHAR,
    };

    use mio_serial::{Error, ErrorKind};

    pub fn last_os_error() -> Error {
        let errno = errno();

        let kind = match errno {
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND
            | ERROR_ACCESS_DENIED => ErrorKind::NoDevice,
            _ => ErrorKind::Io(io::ErrorKind::Other),
        };

        Error::new(kind, error_string(errno).trim())
    }

    // the rest of this module is borrowed from libstd

    fn errno() -> u32 {
        unsafe { GetLastError() }
    }

    fn error_string(errnum: u32) -> String {
        #![allow(non_snake_case)]

        // This value is calculated from the macro
        // MAKELANGID(LANG_SYSTEM_DEFAULT, SUBLANG_SYS_DEFAULT)
        let langId =
            MAKELANGID(LANG_SYSTEM_DEFAULT, SUBLANG_SYS_DEFAULT) as DWORD;

        let mut buf = [0 as WCHAR; 2048];

        unsafe {
            let res = FormatMessageW(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                ptr::null_mut(),
                errnum as DWORD,
                langId as DWORD,
                buf.as_mut_ptr(),
                buf.len() as DWORD,
                ptr::null_mut(),
            );
            if res == 0 {
                // Sometimes FormatMessageW can fail e.g. system doesn't like langId,
                let fm_err = errno();
                return format!(
                    "OS Error {} (FormatMessageW() returned error {})",
                    errnum, fm_err
                );
            }

            let b = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let msg = String::from_utf16(&buf[..b]);
            match msg {
                Ok(msg) => msg,
                Err(..) => format!(
                    "OS Error {} (FormatMessageW() returned invalid UTF-16)",
                    errnum
                ),
            }
        }
    }
}

/// Copied from `src/windows/dcb.rs` in serialport-4.7.0
mod dcb {
    use std::mem::MaybeUninit;
    use winapi::shared::minwindef::*;
    use winapi::um::commapi::*;
    use winapi::um::winbase::*;
    use winapi::um::winnt::HANDLE;

    use mio_serial::{DataBits, FlowControl, Parity, Result, StopBits};

    pub(crate) fn get_dcb(handle: HANDLE) -> Result<DCB> {
        let mut dcb: DCB = unsafe { MaybeUninit::zeroed().assume_init() };
        dcb.DCBlength = std::mem::size_of::<DCB>() as u32;

        if unsafe { GetCommState(handle, &mut dcb) } != 0 {
            Ok(dcb)
        } else {
            Err(super::error::last_os_error())
        }
    }

    /// Initialize the DCB struct
    /// Set all values that won't be affected by `SerialPortBuilder` options.
    pub(crate) fn init(dcb: &mut DCB) {
        // dcb.DCBlength
        // dcb.BaudRate
        // dcb.BitFields
        // dcb.wReserved
        // dcb.XonLim
        // dcb.XoffLim
        // dcb.ByteSize
        // dcb.Parity
        // dcb.StopBits
        dcb.XonChar = 17;
        dcb.XoffChar = 19;
        dcb.ErrorChar = '\0' as winapi::ctypes::c_char;
        dcb.EofChar = 26;
        // dcb.EvtChar
        // always true for communications resources
        dcb.set_fBinary(TRUE as DWORD);
        // dcb.set_fParity()
        // dcb.set_fOutxCtsFlow()
        // serialport-rs doesn't support toggling DSR: so disable fOutxDsrFlow
        dcb.set_fOutxDsrFlow(FALSE as DWORD);
        dcb.set_fDtrControl(DTR_CONTROL_DISABLE);
        // disable because fOutxDsrFlow is disabled as well
        dcb.set_fDsrSensitivity(FALSE as DWORD);
        // dcb.set_fTXContinueOnXoff()
        // dcb.set_fOutX()
        // dcb.set_fInX()
        dcb.set_fErrorChar(FALSE as DWORD);
        // fNull: when set to TRUE null bytes are discarded when received.
        // null bytes won't be discarded by serialport-rs
        dcb.set_fNull(FALSE as DWORD);
        // dcb.set_fRtsControl()
        // serialport-rs does not handle the fAbortOnError behaviour, so we must make sure it's not enabled
        dcb.set_fAbortOnError(FALSE as DWORD);
    }

    pub(crate) fn set_dcb(handle: HANDLE, mut dcb: DCB) -> Result<()> {
        if unsafe { SetCommState(handle, &mut dcb as *mut _) != 0 } {
            Ok(())
        } else {
            Err(super::error::last_os_error())
        }
    }

    pub(crate) fn set_baud_rate(dcb: &mut DCB, baud_rate: u32) {
        dcb.BaudRate = baud_rate as DWORD;
    }

    pub(crate) fn set_data_bits(dcb: &mut DCB, data_bits: DataBits) {
        dcb.ByteSize = match data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        };
    }

    pub(crate) fn set_parity(dcb: &mut DCB, parity: Parity) {
        dcb.Parity = match parity {
            Parity::None => NOPARITY,
            Parity::Odd => ODDPARITY,
            Parity::Even => EVENPARITY,
        };

        dcb.set_fParity(
            if parity == Parity::None { FALSE } else { TRUE } as DWORD
        );
    }

    pub(crate) fn set_stop_bits(dcb: &mut DCB, stop_bits: StopBits) {
        dcb.StopBits = match stop_bits {
            StopBits::One => ONESTOPBIT,
            StopBits::Two => TWOSTOPBITS,
        };
    }

    pub(crate) fn set_flow_control(dcb: &mut DCB, flow_control: FlowControl) {
        match flow_control {
            FlowControl::None => {
                dcb.set_fOutxCtsFlow(0);
                dcb.set_fRtsControl(0);
                dcb.set_fOutX(0);
                dcb.set_fInX(0);
            },
            FlowControl::Software => {
                dcb.set_fOutxCtsFlow(0);
                dcb.set_fRtsControl(0);
                dcb.set_fOutX(1);
                dcb.set_fInX(1);
            },
            FlowControl::Hardware => {
                dcb.set_fOutxCtsFlow(1);
                dcb.set_fRtsControl(1);
                dcb.set_fOutX(0);
                dcb.set_fInX(0);
            },
        }
    }
}

/// Copied and modified from `src/windows/com.rs` in serialport-4.7.0
mod com {
    use std::os::windows::prelude::*;
    use std::ptr;
    use std::time::Duration;

    use super::dcb;
    use mio::windows::NamedPipe;
    use mio_serial::Result;
    use winapi::shared::minwindef::*;
    use winapi::um::commapi::*;
    use winapi::um::fileapi::*;
    use winapi::um::handleapi::*;
    use winapi::um::winbase::*;
    use winapi::um::winnt::{
        FILE_ATTRIBUTE_NORMAL, GENERIC_READ, GENERIC_WRITE, HANDLE, MAXDWORD,
    };

    #[derive(Debug)]
    pub(crate) struct DummyTtyPort {
        pub(crate) handle: HANDLE,
        pub(crate) timeout: Duration,
        pub(crate) port_name: Option<String>,
    }

    /// A [`SerialStream`].
    #[allow(dead_code)]
    #[derive(Debug)]
    pub(crate) struct DummyStream {
        pub(crate) inner: std::mem::ManuallyDrop<DummyTtyPort>,
        pub(crate) pipe: mio::windows::NamedPipe,
    }

    unsafe impl Send for DummyTtyPort {}

    impl DummyTtyPort {
        pub fn open(
            config: &crate::SerialTtyOptions,
        ) -> Result<(DummyTtyPort, RawHandle)> {
            let mut name = Vec::<u16>::with_capacity(4 + config.name.len() + 1);

            name.extend(r"\\.\".encode_utf16());
            name.extend(config.name.encode_utf16());
            name.push(0);

            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null_mut(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED, // This is actual changed point
                    0 as HANDLE,
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                return Err(super::error::last_os_error());
            }

            // create the COMPort here so the handle is getting closed
            // if one of the calls to `get_dcb()` or `set_dcb()` fails
            let mut com =
                DummyTtyPort::open_from_raw_handle(handle as RawHandle);

            let mut dcb = dcb::get_dcb(handle)?;
            dcb::init(&mut dcb);
            dcb::set_baud_rate(&mut dcb, config.baud_rate);
            dcb::set_data_bits(&mut dcb, config.data_bits);
            dcb::set_parity(&mut dcb, config.parity);
            dcb::set_stop_bits(&mut dcb, config.stop_bits);
            dcb::set_flow_control(&mut dcb, config.flow_control);
            dcb::set_dcb(handle, dcb)?;

            if let Some(dtr) = config.dtr_on_open {
                com.write_data_terminal_ready(dtr)?;
            }

            com.set_timeout(config.timeout)?;
            com.port_name = Some(config.name.clone());
            Ok((com, handle))
        }

        fn open_from_raw_handle(handle: RawHandle) -> Self {
            // It is not trivial to get the file path corresponding to a handle.
            // We'll punt and set it `None` here.
            DummyTtyPort {
                handle: handle as HANDLE,
                timeout: Duration::from_millis(100),
                port_name: None,
            }
        }

        fn escape_comm_function(&mut self, function: DWORD) -> Result<()> {
            match unsafe { EscapeCommFunction(self.handle, function) } {
                0 => Err(super::error::last_os_error()),
                _ => Ok(()),
            }
        }

        fn write_data_terminal_ready(&mut self, level: bool) -> Result<()> {
            if level {
                self.escape_comm_function(SETDTR)
            } else {
                self.escape_comm_function(CLRDTR)
            }
        }

        fn timeout_constant(duration: Duration) -> DWORD {
            let milliseconds = duration.as_millis();
            // In the way we are setting up COMMTIMEOUTS, a timeout_constant of MAXDWORD gets rejected.
            // Let's clamp the timeout constant for values of MAXDWORD and above. See remarks at
            // https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-commtimeouts.
            //
            // This effectively throws away accuracy for really long timeouts but at least preserves a
            // long-ish timeout. But just casting to DWORD would result in presumably unexpected short
            // and non-monotonic timeouts from cutting off the higher bits.
            u128::min(milliseconds, MAXDWORD as u128 - 1) as DWORD
        }

        fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
            let timeout_constant = Self::timeout_constant(timeout);

            let mut timeouts = COMMTIMEOUTS {
                ReadIntervalTimeout: MAXDWORD,
                ReadTotalTimeoutMultiplier: MAXDWORD,
                ReadTotalTimeoutConstant: timeout_constant,
                WriteTotalTimeoutMultiplier: 0,
                WriteTotalTimeoutConstant: timeout_constant,
            };

            if unsafe { SetCommTimeouts(self.handle, &mut timeouts) } == 0 {
                return Err(super::error::last_os_error());
            }

            self.timeout = timeout;
            Ok(())
        }
    }

    impl DummyStream {
        pub fn open(config: &crate::SerialTtyOptions) -> Result<DummyStream> {
            let (tty, handle) = DummyTtyPort::open(config)?;

            let pipe = unsafe { NamedPipe::from_raw_handle(handle) };
            let com_port = std::mem::ManuallyDrop::new(tty);

            Ok(Self {
                inner: com_port,
                pipe,
            })
        }
    }
}

use com::DummyStream;

#[allow(dead_code)]
pub(crate) fn open(
    config: &crate::SerialTtyOptions,
) -> mio_serial::Result<mio_serial::SerialStream> {
    let dummy_struct = Box::new(DummyStream::open(config)?);

    let dummy_ref: &DummyStream = Box::leak(dummy_struct);

    Ok(unsafe {
        let ret: mio_serial::SerialStream = std::mem::transmute_copy(dummy_ref);
        ret
    })
}
