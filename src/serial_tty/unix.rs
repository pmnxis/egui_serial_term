use std::os::unix::prelude::*;
use std::time::Duration;

/// fork from serialport-rs
pub(crate) mod ioctl {
    use nix::ioctl_none_bad;
    use nix::libc; // exclude ioctl_read_bad

    ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
    // ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);
    // ioctl_read_bad!(tiocmget, libc::TIOCMGET, libc::c_int);
    // ioctl_none_bad!(tiocsbrk, libc::TIOCSBRK);
    // ioctl_none_bad!(tioccbrk, libc::TIOCCBRK);
}

/// fork from serialport-rs
pub(crate) mod termios {
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

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct DummyTtyPort {
    pub(crate) fd: RawFd,
    pub(crate) timeout: Duration,
    pub(crate) exclusive: bool,
    pub(crate) port_name: Option<String>,
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub(crate) baud_rate: u32,
}

#[allow(path_statements)]
const _: () = {
    // DummyTtyPort should be same size on SerialStream.
    assert!(
        core::mem::size_of::<mio_serial::SerialStream>()
            == core::mem::size_of::<DummyTtyPort>()
    );
};

impl DummyTtyPort {
    unsafe fn borrow_from_serial_stream(
        stream_ref: &mio_serial::SerialStream,
    ) -> &Self {
        std::mem::transmute(stream_ref)
    }

    unsafe fn set_nonblocking(&self) {
        use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

        let res =
            fcntl(self.fd, F_SETFL, fcntl(self.fd, F_GETFL, 0) | O_NONBLOCK);
        assert_eq!(res, 0);
    }
}

pub fn set_nonblocking_serial(stream_ref: &mio_serial::SerialStream) {
    unsafe {
        let dummy = DummyTtyPort::borrow_from_serial_stream(stream_ref);
        dummy.set_nonblocking();
    }
}
