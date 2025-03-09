use egui::{Color32, Vec2};
use egui_serial_term::ColorPalette;
use egui_serial_term::{SerialMonitorBackend, SimpleSerialMonitorManager};
use egui_serial_term::{SerialMonitorView, TerminalTheme, TtyEvent};
use std::sync::mpsc::{Receiver, Sender};

pub struct App {
    serial_monitor_backend: Option<SerialMonitorBackend>,
    terminal_theme: TerminalTheme,
    simple_manager: SimpleSerialMonitorManager,
    tty_proxy_sender: Sender<(u64, egui_serial_term::TtyEvent)>,
    tty_proxy_receiver: Receiver<(u64, egui_serial_term::TtyEvent)>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tty_proxy_sender, tty_proxy_receiver) = std::sync::mpsc::channel();

        let simple_manager: SimpleSerialMonitorManager =
            SimpleSerialMonitorManager::new(None);

        Self {
            serial_monitor_backend: None,
            terminal_theme: TerminalTheme::default(),
            simple_manager,
            tty_proxy_sender,
            tty_proxy_receiver,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok((_, TtyEvent::Exit)) = self.tty_proxy_receiver.try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    self.simple_manager.add_bar_style(
                        ctx,
                        ui,
                        &mut self.serial_monitor_backend,
                        &self.tty_proxy_sender,
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Theme Select : ");

                    if ui.button("default").clicked() {
                        self.terminal_theme =
                            egui_serial_term::TerminalTheme::default();
                    }

                    if ui.button("3024 Day").clicked() {
                        self.terminal_theme =
                            egui_serial_term::TerminalTheme::new(Box::new(
                                ColorPalette {
                                    background: String::from("#F7F7F7"),
                                    foreground: String::from("#4A4543"),
                                    black: String::from("#090300"),
                                    red: String::from("#DB2D20"),
                                    green: String::from("#01A252"),
                                    yellow: String::from("#FDED02"),
                                    blue: String::from("#01A0E4"),
                                    magenta: String::from("#A16A94"),
                                    cyan: String::from("#B5E4F4"),
                                    white: String::from("#A5A2A2"),
                                    bright_black: String::from("#5C5855"),
                                    bright_red: String::from("#E8BBD0"),
                                    bright_green: String::from("#3A3432"),
                                    bright_yellow: String::from("#4A4543"),
                                    bright_blue: String::from("#807D7C"),
                                    bright_magenta: String::from("#D6D5D4"),
                                    bright_cyan: String::from("#CDAB53"),
                                    bright_white: String::from("#F7F7F7"),
                                    ..Default::default()
                                },
                            ));
                    }

                    if ui.button("ubuntu").clicked() {
                        self.terminal_theme =
                            egui_serial_term::TerminalTheme::new(Box::new(
                                ColorPalette {
                                    background: String::from("#300A24"),
                                    foreground: String::from("#FFFFFF"),
                                    black: String::from("#2E3436"),
                                    red: String::from("#CC0000"),
                                    green: String::from("#4E9A06"),
                                    yellow: String::from("#C4A000"),
                                    blue: String::from("#3465A4"),
                                    magenta: String::from("#75507B"),
                                    cyan: String::from("#06989A"),
                                    white: String::from("#D3D7CF"),
                                    bright_black: String::from("#555753"),
                                    bright_red: String::from("#EF2929"),
                                    bright_green: String::from("#8AE234"),
                                    bright_yellow: String::from("#FCE94F"),
                                    bright_blue: String::from("#729FCF"),
                                    bright_magenta: String::from("#AD7FA8"),
                                    bright_cyan: String::from("#34E2E2"),
                                    bright_white: String::from("#EEEEEC"),
                                    ..Default::default()
                                },
                            ));
                    }
                });
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
            } else if self.simple_manager.is_failed_to_connect() {
                ui.colored_label(Color32::RED, "⚠ Failed to open!");
            }
        });
    }
}
