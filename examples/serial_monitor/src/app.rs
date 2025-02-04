use egui::{Color32, Vec2};
use egui_term::{PtyEvent, SerialMonitorView, TerminalTheme};
use egui_term::{SerialMonitorBackend, SerialTtyOptions};
use mio_serial::{DataBits, FlowControl, Parity, StopBits};
use std::sync::mpsc::{Receiver, Sender};

macro_rules! selectable_add {
    ($ui:expr, $current_value:expr, [$($variant:expr),*]) => {
        $(
            $ui.selectable_value(
                &mut $current_value,
                $variant,
                $variant.to_string(),
            );
        )*
    };
}
pub struct App {
    serial_monitor_backend: Option<SerialMonitorBackend>,
    terminal_theme: TerminalTheme,
    tty_list: Vec<String>,
    tty_conn: SerialTtyOptions,
    last_failed: Option<std::time::Instant>,
    pty_proxy_sender: Sender<(u64, egui_term::PtyEvent)>,
    pty_proxy_receiver: Receiver<(u64, egui_term::PtyEvent)>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (pty_proxy_sender, pty_proxy_receiver) = std::sync::mpsc::channel();

        let tty_list: Vec<String> = mio_serial::available_ports()
            .unwrap()
            .iter()
            .map(|x| x.port_name.clone())
            .collect();

        let tty_conn = SerialTtyOptions::default()
            .set_name(tty_list.first().map_or("".to_owned(), |x| x.clone()))
            .set_baud_rate(9600);

        Self {
            serial_monitor_backend: None,
            terminal_theme: TerminalTheme::default(),
            tty_list,
            tty_conn,
            last_failed: None,
            pty_proxy_sender,
            pty_proxy_receiver,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok((_, PtyEvent::Exit)) = self.pty_proxy_receiver.try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("tty_name")
                    .selected_text(self.tty_conn.name.clone())
                    .width(40.0)
                    .show_ui(ui, |ui| {
                        for dev in self.tty_list.iter() {
                            ui.selectable_value(
                                &mut self.tty_conn.name,
                                dev.clone(),
                                dev,
                            );
                        }
                    });

                ui.label("Baudrate");
                egui::ComboBox::from_id_salt("baudrate")
                    .selected_text(format!("{} bps", self.tty_conn.baud_rate))
                    .width(30.0)
                    .show_ui(ui, |ui| {
                        selectable_add!(
                            ui,
                            self.tty_conn.baud_rate,
                            [
                                300, 600, 1200, 1800, 2400, 3600, 4800, 7200,
                                9600, 14400, 19200, 28800, 38400, 56000, 57600,
                                115200, 128000, 134400, 161280, 201600, 230400,
                                256000, 268800, 403200, 460800, 614400, 806400,
                                921600, 1228800, 2457600, 3000000, 6000000,
                                12000000
                            ]
                        );
                    });

                ui.add_space(5.0);
                ui.label("Data Bits");
                egui::ComboBox::from_id_salt("databits")
                    .selected_text(self.tty_conn.data_bits.to_string())
                    .width(30.0)
                    .show_ui(ui, |ui| {
                        selectable_add!(
                            ui,
                            self.tty_conn.data_bits,
                            [
                                DataBits::Five,
                                DataBits::Six,
                                DataBits::Seven,
                                DataBits::Eight
                            ]
                        );
                    });
                ui.add_space(5.0);

                ui.label("Parity");
                egui::ComboBox::from_id_salt("parity")
                    .selected_text(self.tty_conn.parity.to_string())
                    .width(30.0)
                    .show_ui(ui, |ui| {
                        selectable_add!(
                            ui,
                            self.tty_conn.parity,
                            [Parity::None, Parity::Odd, Parity::Even]
                        );
                    });
                ui.add_space(5.0);

                ui.label("Stop Bits");
                egui::ComboBox::from_id_salt("stopbits")
                    .selected_text(self.tty_conn.stop_bits.to_string())
                    .width(30.0)
                    .show_ui(ui, |ui| {
                        selectable_add!(
                            ui,
                            self.tty_conn.stop_bits,
                            [StopBits::One, StopBits::Two]
                        );
                    });
                ui.add_space(5.0);

                ui.label("Flow Control");
                egui::ComboBox::from_id_salt("flowcontrol")
                    .selected_text(self.tty_conn.flow_control.to_string())
                    .width(30.0)
                    .show_ui(ui, |ui| {
                        selectable_add!(
                            ui,
                            self.tty_conn.flow_control,
                            [
                                FlowControl::None,
                                FlowControl::Software,
                                FlowControl::Hardware
                            ]
                        );
                    });

                ui.add_space(15.0);

                if ui.button("Open").clicked() {
                    let new_backend = SerialMonitorBackend::new(
                        0,
                        ctx.clone(),
                        self.pty_proxy_sender.clone(),
                        self.tty_conn.clone(),
                    );

                    if let Ok(backend) = new_backend {
                        self.serial_monitor_backend = Some(backend);
                        self.last_failed = None;
                    } else {
                        self.last_failed = Some(std::time::Instant::now())
                    }
                }
                if ui.button("Close").clicked() {
                    self.serial_monitor_backend = None;
                    self.last_failed = None;
                }
                if ui.button("Refresh List").clicked() {
                    self.tty_list = mio_serial::available_ports()
                        .unwrap()
                        .iter()
                        .map(|x| x.port_name.clone())
                        .collect();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(serial_monitor_backend) =
                &mut self.serial_monitor_backend
            {
                let terminal =
                    SerialMonitorView::new(ui, serial_monitor_backend)
                        .set_focus(true)
                        .set_theme(self.terminal_theme.clone())
                        .set_size(Vec2::new(
                            ui.available_width(),
                            ui.available_height(),
                        ));

                ui.add(terminal);
            } else if let Some(failed_time) = self.last_failed {
                if (std::time::Instant::now() - failed_time).as_secs() < 5 {
                    ui.colored_label(Color32::RED, "⚠ Failed to open!");
                }
            }
        });
    }
}
