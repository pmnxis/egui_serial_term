use egui::{Color32, Key, Modifiers, Vec2};
use egui_serial_term::{
    generate_bindings, Binding, BindingAction, InputKind, KeyboardBinding,
    TerminalMode,
};
use egui_serial_term::{SerialMonitorBackend, SimpleSerialMonitorManager};
use egui_serial_term::{SerialMonitorView, TerminalTheme, TtyEvent};
use std::sync::mpsc::{Receiver, Sender};

pub struct App {
    serial_monitor_backend: Option<SerialMonitorBackend>,
    terminal_theme: TerminalTheme,
    simple_manager: SimpleSerialMonitorManager,
    tty_proxy_sender: Sender<(u64, egui_serial_term::TtyEvent)>,
    tty_proxy_receiver: Receiver<(u64, egui_serial_term::TtyEvent)>,
    custom_terminal_bindings: Vec<(Binding<InputKind>, BindingAction)>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tty_proxy_sender, tty_proxy_receiver) = std::sync::mpsc::channel();

        let simple_manager: SimpleSerialMonitorManager =
            SimpleSerialMonitorManager::new(None);

        let mut custom_terminal_bindings = vec![
            (
                Binding {
                    target: InputKind::KeyCode(egui::Key::C),
                    modifiers: Modifiers::SHIFT,
                    terminal_mode_include: TerminalMode::ALT_SCREEN,
                    terminal_mode_exclude: TerminalMode::empty(),
                },
                BindingAction::Paste,
            ),
            (
                Binding {
                    target: InputKind::KeyCode(egui::Key::A),
                    modifiers: Modifiers::SHIFT | Modifiers::CTRL,
                    terminal_mode_include: TerminalMode::empty(),
                    terminal_mode_exclude: TerminalMode::empty(),
                },
                BindingAction::Char('B'),
            ),
            (
                Binding {
                    target: InputKind::KeyCode(egui::Key::B),
                    modifiers: Modifiers::SHIFT | Modifiers::CTRL,
                    terminal_mode_include: TerminalMode::empty(),
                    terminal_mode_exclude: TerminalMode::empty(),
                },
                BindingAction::Esc("\x1b[5~".into()),
            ),
        ];

        custom_terminal_bindings = [
            custom_terminal_bindings,
            // You can also use generate_bindings macros
            generate_bindings!(
                KeyboardBinding;
                L, Modifiers::SHIFT; BindingAction::Char('K');
            ),
        ]
        .concat();

        Self {
            serial_monitor_backend: None,
            terminal_theme: TerminalTheme::default(),
            simple_manager,
            tty_proxy_sender,
            tty_proxy_receiver,
            custom_terminal_bindings,
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
            ui.horizontal(|ui| {
                self.simple_manager.add_bar_style(
                    ctx,
                    ui,
                    &mut self.serial_monitor_backend,
                    &self.tty_proxy_sender,
                )
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(serial_monitor_backend) =
                &mut self.serial_monitor_backend
            {
                let terminal =
                    SerialMonitorView::new(ui, serial_monitor_backend)
                        .set_focus(true)
                        .add_bindings(self.custom_terminal_bindings.clone())
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
