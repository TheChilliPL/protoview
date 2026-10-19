use crate::protobuf::ProtobufDecoder;
use crate::protobuf::schema::ProtobufSchema;
use crate::protobuf::wire_types::WireValue;
use eframe::Frame;
use eframe::epaint::text::TextWrapMode;
use egui::text::LayoutJob;
use egui::{AsIdSalt, Button, CentralPanel, CollapsingHeader, Color32, Context, MenuBar, Panel, ScrollArea, TextFormat, TextStyle, Ui};
use rfd::AsyncFileDialog;
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use std::borrow::Cow;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use tokio_with_wasm::alias as tokio;
use tracing::debug;
use crate::protobuf::types::ProtobufType;

pub enum Message {
    OpenBuffer(Box<[u8]>),
    OpenSchema(ProtobufSchema),
    ClearSchema,
}

pub struct ProtoViewApp {
    msg: Option<Box<[u8]>>,
    schema: Option<ProtobufSchema>,
    rx: tokio::sync::mpsc::Receiver<Message>,
    tx: tokio::sync::mpsc::Sender<Message>,
}

impl ProtoViewApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_opts(_cc, None, None)
    }

    pub fn new_with_opts(
        _cc: &eframe::CreationContext<'_>,
        msg: Option<Box<[u8]>>,
        schema: Option<ProtobufSchema>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        Self {
            msg,
            schema,
            rx,
            tx,
        }
    }
}

impl eframe::App for ProtoViewApp {
    fn logic(&mut self, _ctx: &Context, _frame: &mut Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Message::OpenBuffer(buffer) => {
                    self.msg = Some(buffer);
                }
                Message::OpenSchema(schema) => {
                    self.schema = Some(schema);
                }
                Message::ClearSchema => {
                    self.schema = None;
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        Panel::top("top_panel").show(ui, |ui| {
            MenuBar::new().ui(ui, |ui| {
                ui.heading("ProtoView");
                ui.menu_button("File", |ui| {
                    if ui.button("Open file").clicked() {
                        let dialog_res = AsyncFileDialog::new()
                            .add_filter("Protobuf Wire Format", &["bin", "binpb"])
                            .add_filter("All files", &["*"])
                            .pick_file();
                        let tx = self.tx.clone();
                        tokio::spawn(async move {
                            if let Some(file) = dialog_res.await {
                                debug!("Opening file: {:?}", file);
                                let bytes = file.read().await.into_boxed_slice();
                                tx.send(Message::OpenBuffer(bytes))
                                    .await
                                    .expect("couldn't send message");
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("Open schema").clicked() {
                        let dialog_res = AsyncFileDialog::new()
                            .add_filter("Protobuf Schema File", &["proto"])
                            .add_filter("All files", &["*"])
                            .pick_file();
                        let tx = self.tx.clone();
                        tokio::spawn(async move {
                            if let Some(file) = dialog_res.await {
                                debug!("Opening schema: {:?}", file);
                                let bytes = file.read().await.into_boxed_slice();
                                let schema = ProtobufSchema::compile_new(bytes).unwrap();
                                tx.send(Message::OpenSchema(schema))
                                    .await
                                    .expect("couldn't send message");
                            }
                        });
                    };
                    if ui
                        .add_enabled(self.schema.is_some(), Button::new("Close schema"))
                        .clicked()
                    {
                        self.tx
                            .blocking_send(Message::ClearSchema)
                            .expect("couldn't send message");
                    }
                })
            })
        });

        Panel::right("inspector").show(ui, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                ui.heading("Metadata inspector");

                ui.take_available_space();
            });
        });

        CentralPanel::default().show(ui, |ui| {
            if let Some(data) = self.msg.as_deref() {
                ScrollArea::new(true).show(ui, |ui| {
                    let decoder = ProtobufDecoder::with_schema(data, self.schema.as_ref());

                    show_records(ui, decoder);

                    ui.take_available_space();
                });
            } else {
                ui.heading("No file opened");
            }
        });
    }
}

fn show_records(ui: &mut Ui, decoder: ProtobufDecoder) {
    let mut hash = DefaultHasher::new();
    decoder.data().hash(&mut hash);
    let hash = hash.finish();

    let weak_format = TextFormat::simple(
        TextStyle::Body.resolve(ui.style()),
        ui.visuals().weak_text_color(),
    );
    let normal_format = TextFormat::simple(
        TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let strong_format = TextFormat::simple(
        TextStyle::Body.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );

    for (i, record) in decoder.clone().enumerate() {
        let record = match record {
            Ok(record) => record,
            Err(e) => {
                ui.colored_label(Color32::RED, format!("Error parsing: {}", e));
                return;
            }
        };

        let field_num = record.field_number;
        let mut lj = LayoutJob::default();
        lj.append(&format!("{}. ", field_num), 0.0, weak_format.clone());
        if let Some(name) = &record.name {
            lj.append(name, 0.0, normal_format.clone());
            lj.append(" = ", 0.0, weak_format.clone());
        }
        let salt = hash + i as u64;
        match record.value {
            WireValue::VarInt(v) => {
                if record.protobuf_type == Some(ProtobufType::Enum) {
                    let enum_name = record.protobuf_type_name.unwrap();
                    lj.append(&enum_name, 0.0, weak_format.clone());
                    lj.append(".", 0.0, weak_format.clone());
                    let enum_value = v.as_i64() as i32;
                    let enum_desc = decoder.schema().unwrap()
                        .resolve_enum_absolute(&enum_name).unwrap()
                        .1.value.iter().find(|v| v.number == Some(enum_value))
                        .unwrap().name.as_deref().unwrap();
                    lj.append(enum_desc, 0.0, strong_format.clone());
                    lj.append(" = ", 0.0, weak_format.clone());
                    lj.append(&enum_value.to_string(), 0.0, normal_format.clone());

                    show_empty_entry(ui, lj, salt);
                    continue;
                }

                let unsigned = v.as_u64();
                let signed = v.as_i64();
                let zigzag = v.as_s64();

                if signed >= 0 && signed as u64 == unsigned {
                    lj.append(&unsigned.to_string(), 0.0, strong_format.clone());
                } else {
                    lj.append(&unsigned.to_string(), 0.0, strong_format.clone());
                    lj.append(" / ", 0.0, weak_format.clone());
                    lj.append(&signed.to_string(), 0.0, strong_format.clone());
                }
                if zigzag != 0 {
                    lj.append(" / ", 0.0, weak_format.clone());
                    lj.append(&zigzag.to_string(), 0.0, normal_format.clone());
                    lj.append("z", 0.0, weak_format.clone());
                }
                show_empty_entry(ui, lj, salt);
            }
            WireValue::LengthDelimited(v) => {
                if let Some(path) = &record.protobuf_type_name {
                    lj.append(&format!("[{}] ", path), 0.0, weak_format.clone());
                }
                if v.as_bytes().is_empty() {
                    lj.append("[empty]", 0.0, normal_format.clone());
                    ui.label(lj);
                    continue;
                }

                let sub_decoder = v.sub_decoder(decoder.schema(), record.protobuf_type_name);
                let str = v.try_as_str();
                let is_str = str.is_ok_and(|s| s.chars().all(|c| !c.is_ascii_control()));

                if is_str {
                    let str = str.unwrap();
                    lj.append(&format!("{:?}", str), 0.0, strong_format.clone());
                    lj.append(" / ", 0.0, weak_format.clone());
                }
                lj.append("[object] / [bytes]", 0.0, weak_format.clone());
                show_entry(ui, lj, salt, Some(|ui: &mut Ui| {
                    show_records(ui, sub_decoder);
                }));
            }
            WireValue::I32(v) => {
                lj.append(&format!("{}i", v.as_i32()), 0.0, strong_format.clone());
                lj.append(&" / ", 0.0, weak_format.clone());
                lj.append(&format!("{}u", v.as_u32()), 0.0, strong_format.clone());
                lj.append(&" / ", 0.0, weak_format.clone());
                lj.append(&format!("{}f", v.as_f32()), 0.0, strong_format.clone());
                show_empty_entry(ui, lj, salt);
            }
            other => {
                lj.append(
                    &format!("Unsupported wire type: {:?}", other.wire_type()),
                    0.0,
                    TextFormat::simple(TextStyle::Body.resolve(ui.style()), Color32::RED),
                );
                ui.label(lj);
            }
        }
    }
}

fn show_empty_entry(ui: &mut Ui, text: LayoutJob, id_salt: impl AsIdSalt) {
    show_entry::<fn(&mut Ui)>(ui, text, id_salt, None);
}
fn show_entry<T>(ui: &mut Ui, mut text: LayoutJob, id_salt: impl AsIdSalt, body: Option<T>)
    where T : FnOnce(&mut Ui) {
    match body {
        Some(body) => {
            CollapsingHeader::new(text)
                .id_salt(id_salt)
                .show(ui, body);
        },
        None => {
            if text.sections.is_empty() {
                text.append("", 0.0, TextFormat::default());
            }
            text.sections[0].leading_space += 18.0;

            ui.label(text);
        }
    }
}
