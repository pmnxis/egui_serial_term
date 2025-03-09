use egui::{Color32, FontId, Vec2};
use egui_serial_term::{
    FontSettings, SerialMonitorView, TerminalFont, TtyEvent,
};
use egui_serial_term::{SerialMonitorBackend, SimpleSerialMonitorManager};
use std::sync::{mpsc::Receiver, mpsc::Sender, Arc};

const TERM_FONT_JET_BRAINS_NAME: &str = "jet brains";
const TERM_FONT_3270_NAME: &str = "3270";
const TERM_FONT_CJK_NAME: &str = "cjk";

const TERM_FONT_JET_BRAINS_BYTES: &[u8] = include_bytes!(
    "../assets/fonts/JetBrains/JetBrainsMonoNerdFontMono-Bold.ttf"
);

const TERM_FONT_3270_BYTES: &[u8] =
    include_bytes!("../assets/fonts/3270/3270NerdFont-Regular.ttf");

const TERM_FONT_CJK_BYTES: &[u8] =
    include_bytes!("../assets/fonts/cjk/LXGWWenKaiMonoTC-Regular.ttf");

fn setup_font(ctx: &egui::Context, name: &str) {
    let bytes = match name {
        TERM_FONT_3270_NAME => &TERM_FONT_3270_BYTES,
        TERM_FONT_CJK_NAME => &TERM_FONT_CJK_BYTES,
        _ => &TERM_FONT_JET_BRAINS_BYTES,
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        name.to_owned(),
        Arc::new(egui::FontData::from_static(bytes)),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, name.to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push(name.to_owned());

    ctx.set_fonts(fonts);
}

pub struct App {
    serial_monitor_backend: Option<SerialMonitorBackend>,
    simple_manager: SimpleSerialMonitorManager,
    font_size: f32,
    tty_proxy_sender: Sender<(u64, egui_serial_term::TtyEvent)>,
    tty_proxy_receiver: Receiver<(u64, egui_serial_term::TtyEvent)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_font(&cc.egui_ctx, TERM_FONT_JET_BRAINS_NAME);
        let (tty_proxy_sender, tty_proxy_receiver) = std::sync::mpsc::channel();

        let simple_manager: SimpleSerialMonitorManager =
            SimpleSerialMonitorManager::new(None);

        Self {
            serial_monitor_backend: None,

            font_size: 14.0,
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
                    if ui.button(TERM_FONT_JET_BRAINS_NAME).clicked() {
                        setup_font(ctx, TERM_FONT_JET_BRAINS_NAME);
                    }

                    if ui.button(TERM_FONT_3270_NAME).clicked() {
                        setup_font(ctx, TERM_FONT_3270_NAME);
                    }

                    if ui.button(TERM_FONT_CJK_NAME).clicked() {
                        setup_font(ctx, TERM_FONT_CJK_NAME);
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("+ size").clicked() {
                        self.font_size += 1.0;
                    }

                    if ui.button("- size").clicked() {
                        self.font_size -= 1.0;
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
                        .set_font(TerminalFont::new(FontSettings {
                            font_type: FontId::proportional(self.font_size),
                        }))
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
