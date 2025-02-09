use mio_serial::{Error, ErrorKind, Result};
use nix::fcntl::FcntlArg::F_SETFL;
use nix::fcntl::{fcntl, OFlag};
use nix::libc::{cfmakeraw, tcgetattr, tcsetattr};
use std::mem::MaybeUninit;
use std::os::unix::prelude::*;
use std::path::Path;
use std::time::Duration;

/// fork from serialport-rs
mod ioctl {
    use nix::ioctl_none_bad;
    use nix::libc; // exclude ioctl_read_bad

    ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
    // ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);
    // ioctl_read_bad!(tiocmget, libc::TIOCMGET, libc::c_int);
    // ioctl_none_bad!(tiocsbrk, libc::TIOCSBRK);
    // ioctl_none_bad!(tioccbrk, libc::TIOCCBRK);
}

#[allow(dead_code)]
#[derive(Debug)]
struct DummyTtyPort {
    fd: RawFd,
    timeout: Duration,
    exclusive: bool,
    port_name: Option<String>,
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    baud_rate: u32,
}

#[allow(path_statements)]
const _: () = {
    // DummyTtyPort should be same size on SerialStream.
    assert!(
        core::mem::size_of::<mio_serial::SerialStream>()
            == core::mem::size_of::<DummyTtyPort>()
    );
};

/// fork from serialport-rs
mod termios {
    use mio_serial::{DataBits, FlowControl, Parity, Result, StopBits};
    use nix::libc;
    use std::os::fd::RawFd;

    pub(crate) type Termios = libc::termios;

    // #[cfg(any(target_os = "ios", target_os = "macos",))]
    pub(crate) fn get_termios(fd: RawFd) -> Result<Termios> {
        use std::mem::MaybeUninit;

        let mut termios = MaybeUninit::uninit();
        let res = unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) };
        nix::errno::Errno::result(res)?;
        let mut termios = unsafe { termios.assume_init() };
        termios.c_ispeed = self::libc::B9600;
        termios.c_ospeed = self::libc::B9600;
        Ok(termios)
    }

    #[cfg(any(target_os = "ios", target_os = "macos",))]
    #[rustfmt::skip]
    /// https://github.com/serialport/serialport-rs/pull/194
    /// Issued for prolific + apple platform
    pub(crate) fn set_termios(fd: RawFd, termios: &mut libc::termios, baud_rate: u32) -> Result<()> {

        let mut ispeed_res = 0;
        let mut ospeed_res = 0;
        if baud_rate > 0 {
            unsafe {
                ispeed_res = libc::cfsetispeed(&mut *termios, baud_rate as libc::speed_t);
                ospeed_res = libc::cfsetospeed(&mut *termios, baud_rate as libc::speed_t);
            }
        }
        nix::errno::Errno::result(ispeed_res)?;
        nix::errno::Errno::result(ospeed_res)?;
    
        let res = unsafe { libc::tcsetattr(fd, libc::TCSANOW, termios) };
        nix::errno::Errno::result(res)?;
    
        Ok(())
    }

    pub(crate) fn set_parity(termios: &mut Termios, parity: Parity) {
        match parity {
            Parity::None => {
                termios.c_cflag &= !(libc::PARENB | libc::PARODD);
                termios.c_iflag &= !libc::INPCK;
                termios.c_iflag |= libc::IGNPAR;
            },
            Parity::Odd => {
                termios.c_cflag |= libc::PARENB | libc::PARODD;
                termios.c_iflag |= libc::INPCK;
                termios.c_iflag &= !libc::IGNPAR;
            },
            Parity::Even => {
                termios.c_cflag &= !libc::PARODD;
                termios.c_cflag |= libc::PARENB;
                termios.c_iflag |= libc::INPCK;
                termios.c_iflag &= !libc::IGNPAR;
            },
        };
    }

    pub(crate) fn set_flow_control(
        termios: &mut Termios,
        flow_control: FlowControl,
    ) {
        match flow_control {
            FlowControl::None => {
                termios.c_iflag &= !(libc::IXON | libc::IXOFF);
                termios.c_cflag &= !libc::CRTSCTS;
            },
            FlowControl::Software => {
                termios.c_iflag |= libc::IXON | libc::IXOFF;
                termios.c_cflag &= !libc::CRTSCTS;
            },
            FlowControl::Hardware => {
                termios.c_iflag &= !(libc::IXON | libc::IXOFF);
                termios.c_cflag |= libc::CRTSCTS;
            },
        };
    }

    pub(crate) fn set_data_bits(termios: &mut Termios, data_bits: DataBits) {
        let size = match data_bits {
            DataBits::Five => libc::CS5,
            DataBits::Six => libc::CS6,
            DataBits::Seven => libc::CS7,
            DataBits::Eight => libc::CS8,
        };

        termios.c_cflag &= !libc::CSIZE;
        termios.c_cflag |= size;
    }

    pub(crate) fn set_stop_bits(termios: &mut Termios, stop_bits: StopBits) {
        match stop_bits {
            StopBits::One => termios.c_cflag &= !libc::CSTOPB,
            StopBits::Two => termios.c_cflag |= libc::CSTOPB,
        };
    }
}

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
    fcntl(fd, F_SETFL(nix::fcntl::OFlag::empty()))?;

    let mut termios = termios::get_termios(fd)?;
    termios::set_parity(&mut termios, config.parity);
    termios::set_flow_control(&mut termios, config.flow_control);
    termios::set_data_bits(&mut termios, config.data_bits);
    termios::set_stop_bits(&mut termios, config.stop_bits);

    termios::set_termios(fd, &mut termios, config.baud_rate)?; // Acutal patched area

    unsafe {
        use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

        let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
        assert_eq!(res, 0);
    }

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
