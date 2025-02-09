//! The main event loop which performs I/O on the pseudoterminal.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use log::error;
use mio::Registry;
use polling::Event as PollingEvent;

use alacritty_terminal::event::{self, Event, OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
// use alacritty_terminal::vte::ansi;
use alacritty_terminal::thread;
use alacritty_terminal::vte::ansi;

use crate::serial_tty::SerialTty;

/// Max bytes to read from the PTY before forced terminal synchronization.
pub(crate) const READ_BUFFER_SIZE: usize = 0x10_0000;

/// Max bytes to read from the PTY while the terminal is locked.
pub(crate) const MAX_LOCKED_READ: usize = u16::MAX as usize;

const SERIAL_TOKEN: mio::Token = mio::Token(0);

const INTERESTS: mio::Interest =
    mio::Interest::READABLE.add(mio::Interest::WRITABLE);

/// Messages that may be sent to the `SerialEventLoop`.
#[derive(Debug)]
pub enum SerialMsg {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    /// Indicates that the `SerialEventLoop` should shut down, as Alacritty is shutting down.
    Shutdown,

    /// Instruction to resize the PTY.
    Resize(WindowSize),
}

/// The main event loop.
///
/// Handles all the PTY I/O and runs the PTY parser which updates terminal
/// state.

pub struct SerialEventLoop<U: alacritty_terminal::event::EventListener> {
    poll: mio::Poll,
    registry: Arc<Registry>,
    tty: SerialTty,
    rx: PeekableReceiver<SerialMsg>,
    tx: Sender<SerialMsg>,
    terminal: Arc<FairMutex<Term<U>>>,
    event_proxy: U,
    _drain_on_exit: bool,
    ref_test: bool,
}

impl<U> SerialEventLoop<U>
where
    U: alacritty_terminal::event::EventListener + Send + 'static,
{
    pub fn new(
        terminal: Arc<FairMutex<Term<U>>>,
        event_proxy: U,
        tty: SerialTty,
        _drain_on_exit: bool,
        ref_test: bool,
    ) -> std::io::Result<SerialEventLoop<U>> {
        let (tx, rx) = mpsc::channel();
        // let poll = Arc::new(RwLock::new(mio::Poll::new()?));
        let poll = mio::Poll::new()?;
        let registry = Arc::new(
            poll.registry()
                .try_clone()
                .expect("Failed to create shared poller registry"),
        );

        Ok(SerialEventLoop {
            poll,
            registry,
            tty,
            rx: PeekableReceiver::new(rx),
            tx,
            terminal,
            event_proxy,
            _drain_on_exit,
            ref_test,
        })
    }

    pub fn channel(&self) -> SerialEventLoopSender {
        SerialEventLoopSender {
            sender: self.tx.clone(),
            poller: self.registry.clone(),
        }
    }

    /// Drain the channel.
    ///
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, state: &mut State) -> bool {
        while let Some(msg) = self.rx.recv() {
            match msg {
                SerialMsg::Input(input) => state.write_list.push_back(input),
                SerialMsg::Resize(window_size) => {
                    self.tty.on_resize(window_size)
                },
                SerialMsg::Shutdown => return false,
            }
        }

        true
    }

    #[inline]
    fn tty_read<X>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut writer: Option<&mut X>,
    ) -> io::Result<()>
    where
        X: Write,
    {
        let mut unprocessed = 0;
        let mut processed = 0;

        // Reserve the next terminal lock for PTY reading.
        let _terminal_lease = Some(self.terminal.lease());
        let mut terminal = None;

        loop {
            // Read from the PTY.
            match self.tty.read(&mut buf[unprocessed..]) {
                // This is received on Windows/macOS when no more data is readable from the PTY.
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        // Go back to mio if we're caught up on parsing and the PTY would block.
                        if unprocessed == 0 {
                            break;
                        }
                    },
                    _ => return Err(err),
                },
            }

            // Attempt to lock the terminal.
            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => {
                    terminal.insert(match self.terminal.try_lock_unfair() {
                        // Force block if we are at the buffer size limit.
                        None if unprocessed >= READ_BUFFER_SIZE => {
                            self.terminal.lock_unfair()
                        },
                        None => continue,
                        Some(terminal) => terminal,
                    })
                },
            };

            // Write a copy of the bytes to the ref test file.
            if let Some(writer) = &mut writer {
                writer.write_all(&buf[..unprocessed]).unwrap();
            }

            // Parse the incoming bytes.
            for byte in &buf[..unprocessed] {
                state.parser.advance(&mut **terminal, *byte);
            }
            // state.parser.advance(&mut **terminal, &buf[..unprocessed]); // alacritty-terminal 0.25.0

            processed += unprocessed;
            unprocessed = 0;

            // Assure we're not blocking the terminal too long unnecessarily.
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }

        // Queue terminal redraw unless all processed bytes were synchronized.
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            self.event_proxy.send_event(Event::Wakeup);
        }

        Ok(())
    }

    #[inline]
    fn tty_write(&mut self, state: &mut State) -> io::Result<()> {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                // match self.pty.writer().write(current.remaining_bytes()) {
                match self.tty.write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    },
                    Ok(n) => {
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    },
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                                break 'write_many
                            },
                            _ => return Err(err),
                        }
                    },
                }
            }
        }

        Ok(())
    }

    pub fn spawn(mut self) -> JoinHandle<(Self, State)> {
        thread::spawn_named("PTY reader", move || {
            let mut state = State::default();
            let mut buf = [0u8; READ_BUFFER_SIZE];

            let mut interest = PollingEvent::readable(0);

            // Register TTY through EventedRW interface.
            if let Err(()) = self
                .poll
                .registry()
                .register(&mut self.tty.stream, SERIAL_TOKEN, INTERESTS)
                .map_err(|e| {
                    error!("Event loop registration error: {}", e);
                    ()
                })
            {
                return (self, state);
            }

            let mut events = mio::event::Events::with_capacity(64); // 1 is enough

            let mut pipe = if self.ref_test {
                Some(
                    File::create("./alacritty.recording")
                        .expect("create alacritty recording"),
                )
            } else {
                None
            };

            'event_loop: loop {
                // Wakeup the event loop when a synchronized update timeout was reached.
                let handler = state.parser.sync_timeout();
                let timeout = handler
                    .sync_timeout()
                    .map(|st| st.saturating_duration_since(Instant::now()));

                events.clear();

                if let Err(err) = self.poll.poll(&mut events, timeout) {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => {
                            error!("Event loop polling error: {}", err);
                            break 'event_loop;
                        },
                    }
                }

                // Handle synchronized update timeout.
                if events.is_empty() && self.rx.peek().is_none() {
                    state.parser.stop_sync(&mut *self.terminal.lock());
                    self.event_proxy.send_event(Event::Wakeup);
                    continue;
                }

                // Handle channel events, if there are any.
                if !self.drain_recv_channel(&mut state) {
                    break;
                }

                for event in events.iter() {
                    match event.token() {
                        SERIAL_TOKEN => {
                            let mut neither_rw = true;

                            if event.is_readable() {
                                neither_rw = false;

                                if let Err(err) = self.tty_read(
                                    &mut state,
                                    &mut buf,
                                    pipe.as_mut(),
                                ) {
                                    // On Linux, a `read` on the master side of a PTY can fail
                                    // with `EIO` if the client side hangs up.  In that case,
                                    // just loop back round for the inevitable `Exited` event.
                                    // This sucks, but checking the process is either racy or
                                    // blocking.
                                    #[cfg(target_os = "linux")]
                                    if err.raw_os_error() == Some(libc::EIO) {
                                        continue;
                                    }

                                    error!("Error reading from PTY in event loop: {}", err);
                                    break 'event_loop;
                                }
                            }

                            if event.is_writable() {
                                neither_rw = false;

                                if let Err(err) = self.tty_write(&mut state) {
                                    error!("Error writing to PTY in event loop: {}", err);
                                    break 'event_loop;
                                }
                            }

                            if neither_rw {
                                println!("unknown event : {:?}", event)
                            }
                        },
                        neither => {
                            println!("neither token : {:?}", neither);
                        },
                    }
                }

                // Register write interest if necessary.
                let needs_write = state.needs_write();
                if needs_write != interest.writable {
                    interest.writable = needs_write;

                    self.poll
                        .registry()
                        .reregister(
                            &mut self.tty.stream,
                            SERIAL_TOKEN,
                            INTERESTS,
                        )
                        .unwrap();
                }
            }

            // The evented instances are not dropped here so deregister them explicitly.
            let _ = self.poll.registry().deregister(&mut self.tty.stream);

            (self, state)
        })
    }
}

/// Helper type which tracks how much of a buffer has been written.
pub(crate) struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

pub struct SerialNotifier(pub SerialEventLoopSender);

impl event::Notify for SerialNotifier {
    fn notify<B>(&self, bytes: B)
    where
        B: Into<Cow<'static, [u8]>>,
    {
        let bytes = bytes.into();
        // Terminal hangs if we send 0 bytes through.
        if bytes.len() == 0 {
            return;
        }

        let _ = self.0.send(SerialMsg::Input(bytes));
    }
}

impl event::OnResize for SerialNotifier {
    fn on_resize(&mut self, window_size: WindowSize) {
        let _ = self.0.send(SerialMsg::Resize(window_size));
    }
}

#[derive(Debug)]
pub enum SerialEventLoopSendError {
    /// Error polling the event loop.
    Io(io::Error),

    /// Error sending a message to the event loop.
    Send(mpsc::SendError<SerialMsg>),
}

impl Display for SerialEventLoopSendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SerialEventLoopSendError::Io(err) => err.fmt(f),
            SerialEventLoopSendError::Send(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for SerialEventLoopSendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerialEventLoopSendError::Io(err) => err.source(),
            SerialEventLoopSendError::Send(err) => err.source(),
        }
    }
}

#[derive(Clone)]
pub struct SerialEventLoopSender {
    sender: Sender<SerialMsg>,
    #[allow(dead_code)]
    poller: Arc<Registry>,
}

impl SerialEventLoopSender {
    #[allow(dead_code)]
    pub(crate) fn new(
        sender: Sender<SerialMsg>,
        poller: Arc<Registry>,
    ) -> SerialEventLoopSender {
        Self { sender, poller }
    }
}

impl SerialEventLoopSender {
    pub fn send(&self, msg: SerialMsg) -> Result<(), SerialEventLoopSendError> {
        self.sender
            .send(msg)
            .map_err(SerialEventLoopSendError::Send)?;
        // self.poller.notify().map_err(SerialEventLoopSendError::Io)
        Ok(())
    }
}

/// All of the mutable state needed to run the event loop.
///
/// Contains list of items to write, current write state, etc. Anything that
/// would otherwise be mutated on the `SerialEventLoop` goes here.
#[derive(Default)]
pub struct State {
    pub(crate) write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    pub(crate) parser: ansi::Processor,
}

impl State {
    #[inline]
    pub(crate) fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    #[inline]
    pub(crate) fn goto_next(&mut self) {
        self.writing = self.write_list.pop_front().map(Writing::new);
    }

    #[inline]
    pub(crate) fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    pub(crate) fn needs_write(&self) -> bool {
        self.writing.is_some() || !self.write_list.is_empty()
    }

    #[inline]
    pub(crate) fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>) -> Writing {
        Writing {
            source: c,
            written: 0,
        }
    }

    #[inline]
    pub(crate) fn advance(&mut self, n: usize) {
        self.written += n;
    }

    #[inline]
    pub(crate) fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    #[inline]
    pub(crate) fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

pub(crate) struct PeekableReceiver<T> {
    pub(crate) rx: Receiver<T>,
    pub(crate) peeked: Option<T>,
}

impl<T> PeekableReceiver<T> {
    pub(crate) fn new(rx: Receiver<T>) -> Self {
        Self { rx, peeked: None }
    }

    pub(crate) fn peek(&mut self) -> Option<&T> {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }

        self.peeked.as_ref()
    }

    pub(crate) fn recv(&mut self) -> Option<T> {
        if self.peeked.is_some() {
            self.peeked.take()
        } else {
            match self.rx.try_recv() {
                Err(TryRecvError::Disconnected) => {
                    panic!("event loop channel closed")
                },
                res => res.ok(),
            }
        }
    }
}
