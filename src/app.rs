//! The egui-based DBC editor application.

use eframe::egui;

use crate::dbc::{self, ByteOrder, Dbc, Message, Multiplexer, Node, Signal, ValueTable, ValueType};
use crate::platform;

/// Built-in sample database so the app is usable the moment it loads.
const EXAMPLE_DBC: &str = include_str!("example.dbc");

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct App {
    dbc: Dbc,
    file_name: String,
    selected_msg: Option<usize>,

    // Transient UI state (not persisted).
    #[serde(skip)]
    search: String,
    #[serde(skip)]
    show_raw: bool,
    #[serde(skip)]
    raw_cache: String,
    #[serde(skip)]
    status: String,
    #[serde(skip)]
    dirty: bool,
}

impl Default for App {
    fn default() -> Self {
        let dbc = dbc::parse(EXAMPLE_DBC).unwrap_or_default();
        // Open on the richest message so the bit layout and multiplexing
        // editor are visible straight away.
        let selected_msg = dbc
            .messages
            .iter()
            .enumerate()
            .max_by_key(|(_, m)| m.signals.len())
            .map(|(i, _)| i);
        App {
            dbc,
            file_name: "example.dbc".to_string(),
            selected_msg,
            search: String::new(),
            show_raw: false,
            raw_cache: String::new(),
            status: "Loaded built-in example. Drag a .dbc file in to edit your own.".to_string(),
            dirty: false,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        if let Some(storage) = cc.storage {
            if let Some(app) = eframe::get_value::<App>(storage, eframe::APP_KEY) {
                return app;
            }
        }
        App::default()
    }

    fn load_text(&mut self, name: String, text: &str) {
        match dbc::parse(text) {
            Ok(d) => {
                let n = d.messages.len();
                self.dbc = d;
                self.file_name = name.clone();
                self.selected_msg = if n > 0 { Some(0) } else { None };
                self.dirty = false;
                self.status = format!("Loaded {name}: {n} messages.");
            }
            Err(e) => {
                self.status = format!("Failed to parse {name}: {e}");
            }
        }
    }

    fn save(&mut self) {
        let text = dbc::write(&self.dbc);
        match platform::save_file(&self.file_name, &text) {
            Ok(true) => {
                self.dirty = false;
                self.status = format!("Saved {}.", self.file_name);
            }
            Ok(false) => self.status = "Save cancelled.".to_string(),
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let name = file
                .name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("dropped.dbc")
                .to_string();
            if let Some(bytes) = &file.bytes {
                let text = String::from_utf8_lossy(bytes).into_owned();
                self.load_text(name, &text);
            } else if let Some(path) = &file.path {
                if let Ok(text) = std::fs::read_to_string(path) {
                    self.load_text(name, &text);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ctx);

        self.top_bar(ctx);
        self.left_panel(ctx);
        self.status_bar(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_raw {
                self.raw_view(ui);
            } else {
                self.message_editor(ui);
            }
        });

        // Visual hint while a file is being dragged over the window.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            ));
            painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                "Drop .dbc file to open",
                egui::FontId::proportional(28.0),
                egui::Color32::WHITE,
            );
        }
    }
}

// --- panels ----------------------------------------------------------------

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.heading("rsdbc");
                ui.separator();

                if cfg!(not(target_arch = "wasm32")) && ui.button("📂 Open").clicked() {
                    if let Some((name, text)) = platform::open_file() {
                        self.load_text(name, &text);
                    }
                }
                if ui.button("💾 Save").clicked() {
                    self.save();
                }
                if ui.button("🆕 New").clicked() {
                    self.dbc = Dbc {
                        version: String::new(),
                        nodes: vec![Node {
                            name: "ECU".to_string(),
                            comment: None,
                        }],
                        ..Default::default()
                    };
                    self.file_name = "untitled.dbc".to_string();
                    self.selected_msg = None;
                    self.dirty = true;
                    self.status = "New database created.".to_string();
                }
                if ui.button("📋 Example").clicked() {
                    self.load_text("example.dbc".to_string(), EXAMPLE_DBC);
                }

                ui.separator();
                ui.toggle_value(&mut self.show_raw, "📄 DBC text");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let dirty = if self.dirty { " •" } else { "" };
                    ui.label(format!("{}{}", self.file_name, dirty));
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} nodes · {} messages · {} signals",
                        self.dbc.nodes.len(),
                        self.dbc.messages.len(),
                        self.dbc.signal_count()
                    ))
                    .weak(),
                );
                ui.separator();
                ui.label(&self.status);
            });
        });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search).hint_text("filter messages"),
                    );
                    if !self.search.is_empty() && ui.small_button("✖").clicked() {
                        self.search.clear();
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.nodes_section(ui);
                    ui.separator();
                    self.messages_section(ui);
                    ui.separator();
                    self.value_tables_section(ui);
                });
            });
    }

    fn nodes_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(format!("🖧 Nodes ({})", self.dbc.nodes.len()))
            .default_open(false)
            .show(ui, |ui| {
                let mut remove: Option<usize> = None;
                for (i, node) in self.dbc.nodes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add(egui::TextEdit::singleline(&mut node.name).desired_width(160.0)).changed() {
                            self.dirty = true;
                        }
                        if ui.small_button("🗑").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.dbc.nodes.remove(i);
                    self.dirty = true;
                }
                if ui.button("➕ Add node").clicked() {
                    self.dbc.nodes.push(Node {
                        name: format!("NODE_{}", self.dbc.nodes.len() + 1),
                        comment: None,
                    });
                    self.dirty = true;
                }
            });
    }

    fn messages_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(format!("✉ Messages ({})", self.dbc.messages.len()));
            if ui.small_button("➕").on_hover_text("Add message").clicked() {
                let new_id = self
                    .dbc
                    .messages
                    .iter()
                    .map(|m| m.raw_id())
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(256);
                self.dbc
                    .messages
                    .push(Message::new(new_id, format!("NEW_MSG_{new_id}")));
                self.selected_msg = Some(self.dbc.messages.len() - 1);
                self.dirty = true;
            }
        });

        let filter = self.search.to_lowercase();
        for i in 0..self.dbc.messages.len() {
            let m = &self.dbc.messages[i];
            if !filter.is_empty()
                && !m.name.to_lowercase().contains(&filter)
                && !format!("{:x}", m.raw_id()).contains(&filter)
            {
                continue;
            }
            let label = format!("0x{:X}  {}  ({})", m.raw_id(), m.name, m.signals.len());
            let selected = self.selected_msg == Some(i);
            if ui.selectable_label(selected, label).clicked() {
                self.selected_msg = Some(i);
                self.show_raw = false;
            }
        }
    }

    fn value_tables_section(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut remove: Option<usize> = None;
        egui::CollapsingHeader::new(format!("🗂 Value tables ({})", self.dbc.value_tables.len()))
            .default_open(false)
            .show(ui, |ui| {
                ui.small("Reusable enums. Apply one to a signal from its Value descriptions.");
                for (i, vt) in self.dbc.value_tables.iter_mut().enumerate() {
                    egui::CollapsingHeader::new(format!("{}  ({})", vt.name, vt.values.len()))
                        .id_salt(("vtable", i))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut vt.name)
                                            .desired_width(150.0),
                                    )
                                    .changed();
                                if ui.small_button("🗑").on_hover_text("Delete table").clicked() {
                                    remove = Some(i);
                                }
                            });
                            changed |= edit_value_pairs(ui, "vt_pairs", &mut vt.values);
                        });
                }
                if ui.button("➕ Add table").clicked() {
                    self.dbc.value_tables.push(ValueTable {
                        name: format!("Table_{}", self.dbc.value_tables.len() + 1),
                        values: vec![(0, "off".to_string()), (1, "on".to_string())],
                    });
                    changed = true;
                }
            });
        if let Some(i) = remove {
            self.dbc.value_tables.remove(i);
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    fn raw_view(&mut self, ui: &mut egui::Ui) {
        self.raw_cache = dbc::write(&self.dbc);
        ui.horizontal(|ui| {
            ui.heading("Generated DBC");
            if ui.button("📋 Copy").clicked() {
                ui.ctx().copy_text(self.raw_cache.clone());
                self.status = "Copied DBC to clipboard.".to_string();
            }
        });
        ui.separator();
        egui::ScrollArea::both().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.raw_cache.as_str())
                    .code_editor()
                    .desired_width(f32::INFINITY),
            );
        });
    }

    fn message_editor(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected_msg else {
            self.welcome(ui);
            return;
        };
        if idx >= self.dbc.messages.len() {
            self.selected_msg = None;
            return;
        }

        // Node names and value tables, cloned so the signal editor can use them
        // while we hold a mutable borrow of the selected message.
        let node_names: Vec<String> = self.dbc.nodes.iter().map(|n| n.name.clone()).collect();
        let value_tables: Vec<ValueTableRef> = self
            .dbc
            .value_tables
            .iter()
            .map(|t| (t.name.clone(), t.values.clone()))
            .collect();
        let mut dirty = false;
        let mut delete_msg = false;

        let m = &mut self.dbc.messages[idx];

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&m.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🗑 Delete message").clicked() {
                        delete_msg = true;
                    }
                });
            });
            ui.separator();

            egui::Grid::new("msg_props")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    dirty |= ui.text_edit_singleline(&mut m.name).changed();
                    ui.end_row();

                    ui.label("ID");
                    ui.horizontal(|ui| {
                        let mut raw = m.raw_id();
                        if ui
                            .add(egui::DragValue::new(&mut raw).range(0..=0x1FFF_FFFF))
                            .changed()
                        {
                            let ext = m.is_extended();
                            m.id = raw | if ext { dbc::EXTENDED_FLAG } else { 0 };
                            dirty = true;
                        }
                        ui.monospace(format!("0x{:X}", m.raw_id()));
                        let mut ext = m.is_extended();
                        if ui.checkbox(&mut ext, "Extended (29-bit)").changed() {
                            m.id = m.raw_id() | if ext { dbc::EXTENDED_FLAG } else { 0 };
                            dirty = true;
                        }
                    });
                    ui.end_row();

                    ui.label("DLC (bytes)");
                    dirty |= ui
                        .add(egui::DragValue::new(&mut m.size).range(0..=64))
                        .changed();
                    ui.end_row();

                    ui.label("Transmitter");
                    egui::ComboBox::from_id_salt("transmitter")
                        .selected_text(&m.transmitter)
                        .show_ui(ui, |ui| {
                            dirty |= ui
                                .selectable_value(
                                    &mut m.transmitter,
                                    "Vector__XXX".to_string(),
                                    "Vector__XXX",
                                )
                                .changed();
                            for n in &node_names {
                                dirty |= ui
                                    .selectable_value(&mut m.transmitter, n.clone(), n)
                                    .changed();
                            }
                        });
                    ui.end_row();

                    ui.label("Comment");
                    let mut comment = m.comment.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut comment).changed() {
                        m.comment = if comment.is_empty() { None } else { Some(comment) };
                        dirty = true;
                    }
                    ui.end_row();
                });

            ui.add_space(8.0);
            bit_layout(ui, m);

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.strong(format!("Signals ({})", m.signals.len()));
                if ui.button("➕ Add signal").clicked() {
                    let start = next_free_bit(m);
                    m.signals.push(Signal::new(
                        format!("NewSignal_{}", m.signals.len() + 1),
                        start,
                    ));
                    dirty = true;
                }
            });
            ui.separator();

            dirty |= signal_list(ui, m, &node_names, &value_tables);
        });

        if delete_msg {
            self.dbc.messages.remove(idx);
            self.selected_msg = if self.dbc.messages.is_empty() {
                None
            } else {
                Some(idx.min(self.dbc.messages.len() - 1))
            };
            self.dirty = true;
        } else if dirty {
            self.dirty = true;
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("rsdbc — CAN DBC editor");
            ui.add_space(8.0);
            ui.label("Select a message on the left, or:");
            ui.add_space(8.0);
            if ui.button("📋 Load example database").clicked() {
                self.load_text("example.dbc".to_string(), EXAMPLE_DBC);
            }
            ui.add_space(4.0);
            ui.label("…or drag a .dbc file anywhere onto this window.");
        });
    }
}

// --- signal list & widgets -------------------------------------------------

/// A global value table flattened to (name, entries) for use as a template.
type ValueTableRef = (String, Vec<(i64, String)>);

/// Render the editable list of signals. Returns true if anything changed.
fn signal_list(
    ui: &mut egui::Ui,
    m: &mut Message,
    nodes: &[String],
    tables: &[ValueTableRef],
) -> bool {
    let mut dirty = false;
    let mut delete: Option<usize> = None;
    let mut move_up: Option<usize> = None;

    for (i, s) in m.signals.iter_mut().enumerate() {
        let header = format!(
            "{}  ·  {}|{} @{}{}",
            s.name,
            s.start_bit,
            s.size,
            if s.byte_order == ByteOrder::LittleEndian { "1" } else { "0" },
            if s.value_type == ValueType::Signed { "-" } else { "+" },
        );
        egui::CollapsingHeader::new(header)
            .id_salt(("sig", i))
            .show(ui, |ui| {
                dirty |= signal_fields(ui, s, nodes, tables);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if i > 0 && ui.small_button("⬆ Move up").clicked() {
                        move_up = Some(i);
                    }
                    if ui.small_button("🗑 Delete signal").clicked() {
                        delete = Some(i);
                    }
                });
            });
    }

    if let Some(i) = delete {
        m.signals.remove(i);
        dirty = true;
    }
    if let Some(i) = move_up {
        m.signals.swap(i - 1, i);
        dirty = true;
    }
    dirty
}

/// All editable fields for a single signal. Returns true if anything changed.
fn signal_fields(
    ui: &mut egui::Ui,
    s: &mut Signal,
    nodes: &[String],
    tables: &[ValueTableRef],
) -> bool {
    let mut dirty = false;
    egui::Grid::new("sig_fields")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Name");
            dirty |= ui.text_edit_singleline(&mut s.name).changed();
            ui.end_row();

            ui.label("Start bit");
            dirty |= ui
                .add(egui::DragValue::new(&mut s.start_bit).range(0..=63))
                .changed();
            ui.end_row();

            ui.label("Length (bits)");
            dirty |= ui
                .add(egui::DragValue::new(&mut s.size).range(1..=64))
                .changed();
            ui.end_row();

            ui.label("Byte order");
            egui::ComboBox::from_id_salt("order")
                .selected_text(s.byte_order.label())
                .show_ui(ui, |ui| {
                    dirty |= ui
                        .selectable_value(
                            &mut s.byte_order,
                            ByteOrder::LittleEndian,
                            ByteOrder::LittleEndian.label(),
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut s.byte_order,
                            ByteOrder::BigEndian,
                            ByteOrder::BigEndian.label(),
                        )
                        .changed();
                });
            ui.end_row();

            ui.label("Value type");
            egui::ComboBox::from_id_salt("vtype")
                .selected_text(s.value_type.label())
                .show_ui(ui, |ui| {
                    dirty |= ui
                        .selectable_value(
                            &mut s.value_type,
                            ValueType::Unsigned,
                            ValueType::Unsigned.label(),
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut s.value_type,
                            ValueType::Signed,
                            ValueType::Signed.label(),
                        )
                        .changed();
                });
            ui.end_row();

            ui.label("Factor");
            dirty |= ui.add(egui::DragValue::new(&mut s.factor).speed(0.001)).changed();
            ui.end_row();

            ui.label("Offset");
            dirty |= ui.add(egui::DragValue::new(&mut s.offset).speed(0.001)).changed();
            ui.end_row();

            ui.label("Minimum");
            dirty |= ui.add(egui::DragValue::new(&mut s.min)).changed();
            ui.end_row();

            ui.label("Maximum");
            dirty |= ui.add(egui::DragValue::new(&mut s.max)).changed();
            ui.end_row();

            ui.label("Unit");
            dirty |= ui.text_edit_singleline(&mut s.unit).changed();
            ui.end_row();

            ui.label("Multiplexing");
            dirty |= mux_editor(ui, s);
            ui.end_row();

            ui.label("Receivers");
            let mut recv = s.receivers.join(",");
            if ui.text_edit_singleline(&mut recv).changed() {
                s.receivers = recv
                    .split(',')
                    .map(|r| r.trim().to_string())
                    .filter(|r| !r.is_empty())
                    .collect();
                if s.receivers.is_empty() {
                    s.receivers.push("Vector__XXX".to_string());
                }
                dirty = true;
            }
            ui.end_row();

            ui.label("Comment");
            let mut comment = s.comment.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut comment).changed() {
                s.comment = if comment.is_empty() { None } else { Some(comment) };
                dirty = true;
            }
            ui.end_row();
        });

    let _ = nodes; // receivers are free-text; node list kept for future combo use.

    ui.add_space(4.0);
    dirty |= value_descriptions_editor(ui, s, tables);
    dirty
}

fn mux_editor(ui: &mut egui::Ui, s: &mut Signal) -> bool {
    let mut dirty = false;
    ui.horizontal(|ui| {
        let current = match &s.multiplexer {
            Multiplexer::None => "Normal",
            Multiplexer::Multiplexor => "Multiplexor (M)",
            Multiplexer::Multiplexed(_) => "Multiplexed (m)",
        };
        egui::ComboBox::from_id_salt("mux")
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(matches!(s.multiplexer, Multiplexer::None), "Normal").clicked() {
                    s.multiplexer = Multiplexer::None;
                    dirty = true;
                }
                if ui
                    .selectable_label(matches!(s.multiplexer, Multiplexer::Multiplexor), "Multiplexor (M)")
                    .clicked()
                {
                    s.multiplexer = Multiplexer::Multiplexor;
                    dirty = true;
                }
                if ui
                    .selectable_label(matches!(s.multiplexer, Multiplexer::Multiplexed(_)), "Multiplexed (m)")
                    .clicked()
                {
                    s.multiplexer = Multiplexer::Multiplexed(0);
                    dirty = true;
                }
            });
        if let Multiplexer::Multiplexed(n) = &mut s.multiplexer {
            ui.label("switch value:");
            dirty |= ui.add(egui::DragValue::new(n)).changed();
        }
    });
    dirty
}

/// An editable table of `(value, label)` rows. Returns true if changed.
fn edit_value_pairs(ui: &mut egui::Ui, salt: &str, pairs: &mut Vec<(i64, String)>) -> bool {
    let mut dirty = false;
    let mut remove: Option<usize> = None;
    egui::Grid::new(salt).num_columns(3).show(ui, |ui| {
        for (i, (val, label)) in pairs.iter_mut().enumerate() {
            dirty |= ui.add(egui::DragValue::new(val)).changed();
            dirty |= ui
                .add(egui::TextEdit::singleline(label).desired_width(180.0))
                .changed();
            if ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        pairs.remove(i);
        dirty = true;
    }
    if ui.button("➕ Add value").clicked() {
        let next = pairs.last().map(|(v, _)| v + 1).unwrap_or(0);
        pairs.push((next, String::new()));
        dirty = true;
    }
    dirty
}

fn value_descriptions_editor(ui: &mut egui::Ui, s: &mut Signal, tables: &[ValueTableRef]) -> bool {
    let mut dirty = false;
    egui::CollapsingHeader::new(format!("Value descriptions ({})", s.value_descriptions.len()))
        .id_salt("vals")
        .show(ui, |ui| {
            // Reuse a global value table as a template by copying its entries
            // into this signal's inline VAL_ descriptions (the portable link).
            if !tables.is_empty() {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("apply_table")
                        .selected_text("Apply table…")
                        .show_ui(ui, |ui| {
                            for (name, values) in tables {
                                if ui.selectable_label(false, name).clicked() {
                                    s.value_descriptions = values.clone();
                                    dirty = true;
                                }
                            }
                        });
                    if !s.value_descriptions.is_empty()
                        && ui.small_button("Clear").on_hover_text("Remove all values").clicked()
                    {
                        s.value_descriptions.clear();
                        dirty = true;
                    }
                });
            }
            dirty |= edit_value_pairs(ui, "valdesc", &mut s.value_descriptions);
        });
    dirty
}

// --- bit layout visualisation ----------------------------------------------

/// Flat DBC bit positions (byte*8 + bit-in-byte) a signal occupies.
fn occupied_positions(s: &Signal) -> Vec<u64> {
    let mut out = Vec::with_capacity(s.size as usize);
    match s.byte_order {
        ByteOrder::LittleEndian => {
            for i in 0..s.size {
                out.push(s.start_bit + i);
            }
        }
        ByteOrder::BigEndian => {
            // Motorola sawtooth: move toward the LSB, wrapping into the next byte.
            let mut pos = s.start_bit as i64;
            for _ in 0..s.size {
                if pos < 0 {
                    break;
                }
                out.push(pos as u64);
                if pos % 8 == 0 {
                    pos += 15;
                } else {
                    pos -= 1;
                }
            }
        }
    }
    out
}

const PALETTE: [egui::Color32; 10] = [
    egui::Color32::from_rgb(0x4e, 0x79, 0xa7),
    egui::Color32::from_rgb(0xf2, 0x8e, 0x2b),
    egui::Color32::from_rgb(0x59, 0xa1, 0x4f),
    egui::Color32::from_rgb(0xe1, 0x57, 0x59),
    egui::Color32::from_rgb(0x76, 0xb7, 0xb2),
    egui::Color32::from_rgb(0xed, 0xc9, 0x48),
    egui::Color32::from_rgb(0xb0, 0x7a, 0xa1),
    egui::Color32::from_rgb(0xff, 0x9d, 0xa7),
    egui::Color32::from_rgb(0x9c, 0x75, 0x5f),
    egui::Color32::from_rgb(0xba, 0xb0, 0xac),
];

fn bit_layout(ui: &mut egui::Ui, m: &Message) {
    // Map each flat bit position to the owning signal index (last writer wins).
    let total_bits = (m.size.max(1) * 8) as usize;
    let mut owner = vec![usize::MAX; total_bits.max(64)];
    for (si, s) in m.signals.iter().enumerate() {
        for p in occupied_positions(s) {
            if (p as usize) < owner.len() {
                owner[p as usize] = si;
            }
        }
    }

    egui::CollapsingHeader::new("🧩 Bit layout")
        .default_open(true)
        .show(ui, |ui| {
            let cell = egui::vec2(34.0, 22.0);
            let bytes = m.size.max(1) as usize;
            egui::Grid::new("bitgrid")
                .spacing([2.0, 2.0])
                .min_col_width(cell.x)
                .show(ui, |ui| {
                    // Header: bit 7 .. 0
                    ui.label("");
                    for b in (0..8).rev() {
                        ui.label(egui::RichText::new(format!("{b}")).weak().monospace());
                    }
                    ui.end_row();

                    for byte in 0..bytes {
                        ui.label(egui::RichText::new(format!("B{byte}")).weak().monospace());
                        for b in (0..8).rev() {
                            let flat = byte * 8 + b;
                            let si = owner.get(flat).copied().unwrap_or(usize::MAX);
                            let (color, text, hover) = if si == usize::MAX {
                                (
                                    ui.visuals().faint_bg_color,
                                    String::new(),
                                    "free".to_string(),
                                )
                            } else {
                                let c = PALETTE[si % PALETTE.len()];
                                (c, format!("{flat}"), m.signals[si].name.clone())
                            };
                            let (rect, resp) =
                                ui.allocate_exact_size(cell, egui::Sense::hover());
                            ui.painter().rect_filled(rect, 3.0, color);
                            if !text.is_empty() {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    text,
                                    egui::FontId::monospace(10.0),
                                    egui::Color32::from_black_alpha(200),
                                );
                            }
                            resp.on_hover_text(hover);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (si, s) in m.signals.iter().enumerate() {
                    let c = PALETTE[si % PALETTE.len()];
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, c);
                    ui.label(&s.name);
                }
            });
        });
}

/// Lowest bit index not used by any signal in the message.
fn next_free_bit(m: &Message) -> u64 {
    let mut used = [false; 64];
    for s in &m.signals {
        for p in occupied_positions(s) {
            if (p as usize) < 64 {
                used[p as usize] = true;
            }
        }
    }
    used.iter().position(|&u| !u).unwrap_or(0) as u64
}
