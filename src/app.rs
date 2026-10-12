use std::borrow::Cow;
use eframe::Frame;
use eframe::epaint::text::TextWrapMode;
use egui::text::LayoutJob;
use egui::{CentralPanel, CollapsingHeader, Color32, MenuBar, Panel, ScrollArea, TextFormat, TextStyle, Ui};
use protobin::decode::{MsgDecoder, MsgRecordRef};
use protobin::wire::WireValueRef;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use rfd::AsyncFileDialog;
use tokio_with_wasm::alias as tokio;
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use tracing::debug;

pub struct ProtoViewApp {
    msg: Option<Box<[u8]>>,
}

impl ProtoViewApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            msg: None,
        }
    }
}

impl eframe::App for ProtoViewApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        Panel::top("top_panel").show(ui, |ui| {
            MenuBar::new().ui(ui, |ui| {
                ui.heading("ProtoView");
                ui.menu_button("File", |ui| {
                    if ui.button("Open file").clicked() {
                        let dialog_res = AsyncFileDialog::new()
                            .add_filter("Protobuf Wire Format", &["bin", "binpb"])
                            .add_filter("All files", &["*"])
                            .pick_file();
                        tokio::spawn(async {
                            let file = dialog_res.await;

                            debug!("Opening file: {:?}", file);
                        });
                        // if let Some(path) = dialog_res {
                        //     let mut file = File::open(path).unwrap();
                        //     let mut bytes = Vec::new();
                        //     file.read_to_end(&mut bytes).unwrap();
                        //     self.msg = Some(bytes.into_boxed_slice());
                        // }
                    }
                    if ui.button("Open schema").clicked() {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            let dialog_res = FileDialog::new()
                                .add_filter("Protobuf Schema File", &["proto"])
                                .add_filter("All files", &["*"])
                                .pick_file();
                            if let Some(path) = dialog_res {
                                println!("Loading Context from {}", path.display());
                            }
                        }
                    };
                })
            })
        });

        Panel::right("inspector")
            .show(ui, |ui| {
                ScrollArea::both()
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        ui.heading("Metadata inspector");

                        ui.take_available_space();
                    });
            });

        CentralPanel::default().show(ui, |ui| {
            if let Some(data) = self.msg.as_deref() {
                let decoder = MsgDecoder::new(data);

                let data = decoder.collect::<Result<Vec<_>, _>>().unwrap();

                ScrollArea::new(true)
                    .show(ui, |ui| {

                        show_records(ui, &data);

                        ui.take_available_space();
                    });
            } else {
                ui.heading("No file opened");
            }
        });
    }
}

fn show_records(ui: &mut Ui, data: &[MsgRecordRef]) {
    let mut hash = DefaultHasher::new();
    data.hash(&mut hash);
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

    for (i, record) in data.iter().enumerate() {
        let field_num = record.field_number.value();
        let mut lj = LayoutJob::default();
        lj.append(
            &format!("{}. ", field_num),
            18.0,
            weak_format.clone(),
        );
        match record.value {
            WireValueRef::VarInt(v) => {
                let unsigned = v.as_uint64();
                let signed = v.as_int64();
                let zigzag = v.as_sint64();

                if signed >= 0 && signed as u64 == unsigned {
                    lj.append(
                        &unsigned.to_string(),
                        0.0,
                        strong_format.clone(),
                    );
                } else {
                    lj.append(
                        &unsigned.to_string(),
                        0.0,
                        strong_format.clone(),
                    );
                    lj.append(
                        " / ",
                        0.0,
                        weak_format.clone(),
                    );
                    lj.append(
                        &signed.to_string(),
                        0.0,
                        strong_format.clone(),
                    );
                }
                if zigzag != 0 {
                    lj.append(
                        " / ",
                        0.0,
                        weak_format.clone(),
                    );
                    lj.append(
                        &zigzag.to_string(),
                        0.0,
                        normal_format.clone(),
                    );
                    lj.append(
                        "z",
                        0.0,
                        weak_format.clone(),
                    );
                }
                ui.label(lj);
            }
            WireValueRef::Len(v) => {
                if v.data.is_empty() {
                    lj.append(
                        "[empty]",
                        0.0,
                        normal_format.clone(),
                    );
                    ui.label(lj);
                    continue;
                }

                let subdata = v.as_sub_msg().collect::<Result<Vec<_>, _>>();
                let str = str::from_utf8(v.data);
                let is_str = str.is_ok() && str.unwrap().chars().all(|c| !c.is_ascii_control());

                if let Ok(subdata) = v.as_sub_msg().collect::<Result<Vec<_>, _>>() {
                    if is_str {
                        let str = str.unwrap();
                        lj.append(
                            &format!("{:?}", str),
                            0.0,
                            strong_format.clone(),
                        );
                        lj.append(
                            " / ",
                            0.0,
                            weak_format.clone()
                        );
                    }
                    lj.append(
                        "[object]",
                        0.0,
                        normal_format.clone(),
                    );
                    lj.sections[0].leading_space = 0.0;
                    CollapsingHeader::new(lj)
                        .id_salt(hash + i as u64)
                        .show(ui, |ui| {
                            show_records(ui, &subdata);
                        });
                } else if is_str {
                    let str = str.unwrap();
                    lj.append(
                        &format!("{:?}", str),
                        0.0,
                        strong_format.clone(),
                    );
                    ui.label(lj);
                } else {
                    lj.append(
                        "[bytes]",
                        0.0,
                        normal_format.clone(),
                    );
                    ui.label(lj);
                }
            }
            other => {
                lj.append(
                    &format!("Unsupported wire type: {:?}", other.write_type()),
                    0.0,
                    TextFormat::simple(TextStyle::Body.resolve(ui.style()), Color32::RED),
                );
                ui.label(lj);
            }
        }
    }
}
