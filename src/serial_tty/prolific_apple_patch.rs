use crate::serial_tty::unix::DummyTtyPort;
use crate::serial_tty::unix::{ioctl, termios};
use mio_serial::{Error, ErrorKind, Result};
#[allow(unused_imports)]
use nix::fcntl::{fcntl, FcntlArg::F_SETFL, OFlag};
use nix::libc::{cfmakeraw, tcgetattr, tcsetattr};
use std::mem::MaybeUninit;
use std::path::Path;

/// fork from serialport-rs
/// hotfix type open for prolific CDC device on apple platform.
pub fn open(
    config: &crate::SerialTtyOptions,
) -> Result<mio_serial::SerialStream> {
    let path = Path::new(&config.name);
    let fd = nix::fcntl::open(
        path,
        OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .unwrap();

    // Try to claim exclusive access to the port. This is performed even
    // if the port will later be set as non-exclusive, in order to respect
    // other applications that may have an exclusive port lock.
    unsafe {
        ioctl::tiocexcl(fd)?;
    }

    let mut termios = MaybeUninit::uninit();
    nix::errno::Errno::result(unsafe { tcgetattr(fd, termios.as_mut_ptr()) })
        .unwrap();
    let mut termios = unsafe { termios.assume_init() };

    // setup TTY for binary serial port access
    // Enable reading from the port and ignore all modem control lines
    termios.c_cflag |= libc::CREAD | libc::CLOCAL;
    // Enable raw mode which disables any implicit processing of the input or output data streams
    // This also sets no timeout period and a read will block until at least one character is
    // available.
    unsafe { cfmakeraw(&mut termios) };

    // write settings to TTY
    unsafe { tcsetattr(fd, libc::TCSANOW, &termios) };

    // Read back settings from port and confirm they were applied correctly
    let mut actual_termios = MaybeUninit::uninit();
    unsafe { tcgetattr(fd, actual_termios.as_mut_ptr()) };
    let actual_termios = unsafe { actual_termios.assume_init() };

    if actual_termios.c_iflag != termios.c_iflag
        || actual_termios.c_oflag != termios.c_oflag
        || actual_termios.c_lflag != termios.c_lflag
        || actual_termios.c_cflag != termios.c_cflag
    {
        return Err(Error::new(
            ErrorKind::Unknown,
            "Settings did not apply correctly",
        ));
    };

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    if config.baud_rate > 0 {
        unsafe { libc::tcflush(fd, libc::TCIOFLUSH) };
    }

    // clear O_NONBLOCK flag
    // but comment below line for using O_NONBLOCK
    // fcntl(fd, F_SETFL(nix::fcntl::OFlag::empty()))?;

    let mut termios = termios::get_termios(fd)?;
    termios::set_parity(&mut termios, config.parity);
    termios::set_flow_control(&mut termios, config.flow_control);
    termios::set_data_bits(&mut termios, config.data_bits);
    termios::set_stop_bits(&mut termios, config.stop_bits);

    termios::set_termios(fd, &mut termios, config.baud_rate)?; // Acutal patched area

    // rust don't allow access private member as force
    let dummy_struct = Box::new(DummyTtyPort {
        fd,
        timeout: config.timeout,
        exclusive: true,
        port_name: Some(config.name.clone()),
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        baud_rate: config.baud_rate,
    });

    let dummy_ref: &DummyTtyPort = Box::leak(dummy_struct);

    Ok(unsafe {
        let ret: mio_serial::SerialStream = std::mem::transmute_copy(dummy_ref);
        ret
    })
}
