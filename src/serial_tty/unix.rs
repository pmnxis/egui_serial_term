//! Serial TTY related functionality.
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result};
use std::os::fd::OwnedFd;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use polling::{Event, PollMode, Poller};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use signal_hook::low_level::{
    pipe as signal_pipe, unregister as unregister_signal,
};
use signal_hook::{consts as sigconsts, SigId};
use std::os::fd::FromRawFd;

use crate::serial_tty::SerialTtyOptions;
use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite};

// Interest in PTY read/writes.
pub(crate) const PTY_READ_WRITE_TOKEN: usize = 0;

// Interest in new child events.
pub(crate) const PTY_CHILD_EVENT_TOKEN: usize = 1;

pub struct SerialTty {
    // child: Child,
    file: File,
    signals: UnixStream,
    sig_id: SigId,
    #[allow(unused)]
    pub port: serialport::TTYPort,
}

/// Create a new TTY and return a handle to interact with it.
pub fn new(
    config: &SerialTtyOptions,
    _window_size: WindowSize,
    _window_id: u64,
) -> Result<SerialTty> {
    if let Ok(ports) = serialport::available_ports() {
        if let Some(matched) = ports.iter().find(|x| x.port_name == config.name)
        {
            if let serialport::SerialPortType::UsbPort(u) = &matched.port_type {
                println!(
                    "mfn : {}",
                    u.manufacturer.clone().unwrap_or("".to_owned())
                );
            }
        } else {
            // not found
            println!("Not Found");
        }

        // `fcntl(fd.0, F_SETFL(nix::fcntl::OFlag::empty()))?;` internally
        let tty = serialport::new(&config.name, config.baud_rate)
            .timeout(Duration::from_millis(100))
            .open_native()?;

        let fd = unsafe { OwnedFd::from_raw_fd(tty.as_raw_fd()) };
        let file = File::from(fd);

        // Prepare signal handling before spawning child.
        let (signals, sig_id) = {
            let (sender, recv) = UnixStream::pair()?;

            // Register the recv end of the pipe for SIGCHLD.
            let sig_id = signal_pipe::register(sigconsts::SIGCHLD, sender)?;
            recv.set_nonblocking(true)?;
            (recv, sig_id)
        };

        // set_nonblocking
        unsafe {
            use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

            let res = fcntl(
                tty.as_raw_fd(),
                F_SETFL,
                fcntl(tty.as_raw_fd(), F_GETFL, 0) | O_NONBLOCK,
            );
            assert_eq!(res, 0);
        }

        Ok(SerialTty {
            file,
            signals,
            sig_id,
            port: tty,
        })
    } else {
        Err(Error::new(ErrorKind::InvalidData, "Unknown SerialTty Call"))
    }
}

impl Drop for SerialTty {
    fn drop(&mut self) {
        // Do not terminate TTY, rather than PTY
        // Clear signal-hook handler.
        unregister_signal(self.sig_id);
    }
}

impl EventedReadWrite for SerialTty {
    type Reader = File;
    type Writer = File;

    #[inline]
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        poll_opts: PollMode,
    ) -> Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;

        unsafe {
            poll.add_with_mode(&self.file, interest, poll_opts)?;
        }

        unsafe {
            poll.add_with_mode(
                &self.signals,
                Event::readable(PTY_CHILD_EVENT_TOKEN),
                PollMode::Level,
            )
        }
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        poll_opts: PollMode,
    ) -> Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        poll.modify_with_mode(&self.file, interest, poll_opts)?;

        poll.modify_with_mode(
            &self.signals,
            Event::readable(PTY_CHILD_EVENT_TOKEN),
            PollMode::Level,
        )
    }

    #[inline]
    fn deregister(&mut self, poll: &Arc<Poller>) -> Result<()> {
        poll.delete(&self.file)?;
        poll.delete(&self.signals)
    }

    #[inline]
    fn reader(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn writer(&mut self) -> &mut File {
        &mut self.file
    }
}

impl EventedPty for SerialTty {
    #[inline]
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        // See if there has been a SIGCHLD.
        let mut buf = [0u8; 1];
        if let Err(err) = self.signals.read(&mut buf) {
            if err.kind() != ErrorKind::WouldBlock {
                log::error!("Error reading from signal pipe: {}", err);
            }
            return None;
        }

        None
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
