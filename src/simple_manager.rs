use crate::{SerialMonitorBackend, SerialTtyOptions, TtyEvent};
use egui::{Color32, RichText};
use mio_serial::{DataBits, FlowControl, Parity, StopBits};
use std::sync::mpsc::Sender;

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

pub struct SimpleSerialMonitorManager {
    tty_list: Vec<String>,
    tty_conn: SerialTtyOptions,
    last_failed: Option<std::time::Instant>,
}

impl SimpleSerialMonitorManager {
    pub fn new(default_baudrate: Option<u32>) -> Self {
        let baudrate = default_baudrate.unwrap_or(115200);

        let tty_list: Vec<String> = mio_serial::available_ports()
            .unwrap()
            .iter()
            .map(|x| x.port_name.clone())
            .collect();

        let tty_conn = SerialTtyOptions::default()
            .set_name(tty_list.last().map_or("".to_owned(), |x| x.clone()))
            .set_baud_rate(baudrate);

        Self {
            tty_list,
            tty_conn,
            last_failed: None,
        }
    }

    /// Add bar style UI to open connection or close.
    pub fn add_bar_style(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        serial_monitor_backend: &mut Option<SerialMonitorBackend>,
        tty_proxy_sender: &Sender<(u64, TtyEvent)>,
    ) {
        egui::ComboBox::from_id_salt("tty_name")
            .selected_text(self.tty_conn.name.clone())
            .width(110.0)
            .height(
                /* https://github.com/emilk/egui/issues/5138 */
                (ui.spacing().interact_size.y
                    * (1.0 + self.tty_list.len() as f32))
                    .max(200.0),
            )
            .wrap_mode(egui::TextWrapMode::Extend)
            .show_ui(ui, |ui| {
                if self.tty_list.is_empty() {
                    ui.label("Missing Devices");
                } else {
                    for dev in self.tty_list.iter() {
                        ui.selectable_value(
                            &mut self.tty_conn.name,
                            dev.clone(),
                            dev,
                        );
                    }
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
                        300, 600, 1200, 1800, 2400, 3600, 4800, 7200, 9600,
                        14400, 19200, 28800, 38400, 56000, 57600, 115200,
                        128000, 134400, 161280, 201600, 230400, 256000, 268800,
                        403200, 460800, 614400, 806400, 921600, 1228800,
                        2457600, 3000000, 6000000, 12000000
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

        if ui.button("Refresh List").clicked() {
            self.tty_list = mio_serial::available_ports()
                .unwrap()
                .iter()
                .map(|x| x.port_name.clone())
                .collect();

            println!("listing serial port");
            println!("{:?}", self.tty_list);
        }

        if serial_monitor_backend.is_some() {
            if ui.button("CLOSE").clicked() {
                *serial_monitor_backend = None;
                self.last_failed = None;
            }

            if ui.button("Send ^C").clicked() {
                // self.serial_monitor_backend.as_mut()
                if let Some(backend) = serial_monitor_backend {
                    backend.write(&[0x03]);
                }
            }
        } else if ui
            .button(RichText::new("OPEN ").color(Color32::GREEN))
            .clicked()
        {
            let new_backend = SerialMonitorBackend::new(
                0,
                ctx.clone(),
                tty_proxy_sender.clone(),
                self.tty_conn.clone(),
            );

            if let Ok(backend) = new_backend {
                *serial_monitor_backend = Some(backend);
                self.last_failed = None;
            } else {
                self.last_failed = Some(std::time::Instant::now())
            }
        }
    }

    pub fn is_failed_to_connect(&self) -> bool {
        if let Some(failed_time) = self.last_failed {
            (std::time::Instant::now() - failed_time).as_secs() < 5
        } else {
            false
        }
    }
}
