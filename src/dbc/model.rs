//! In-memory representation of a CAN database (`.dbc`) file.
//!
//! The model is editor-oriented: comments and value descriptions are attached
//! directly to the objects they describe so the UI can edit them in place.
//! Constructs we do not model explicitly (attribute definitions, signal groups,
//! …) are preserved verbatim in [`Dbc::extra`] so a load/save round-trip never
//! discards data.

use serde::{Deserialize, Serialize};

/// Byte order of a signal on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ByteOrder {
    /// `@1` — Intel / little-endian.
    LittleEndian,
    /// `@0` — Motorola / big-endian.
    BigEndian,
}

impl ByteOrder {
    pub fn label(self) -> &'static str {
        match self {
            ByteOrder::LittleEndian => "Little (Intel)",
            ByteOrder::BigEndian => "Big (Motorola)",
        }
    }
}

/// Whether the raw value is interpreted as signed or unsigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueType {
    Unsigned,
    Signed,
}

impl ValueType {
    pub fn label(self) -> &'static str {
        match self {
            ValueType::Unsigned => "Unsigned",
            ValueType::Signed => "Signed",
        }
    }
}

/// Multiplexing role of a signal within its message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Multiplexer {
    /// Ordinary signal, always present.
    None,
    /// The multiplexor switch (`M`).
    Multiplexor,
    /// Multiplexed signal, present only when the switch equals this value (`m<n>`).
    Multiplexed(u64),
}

/// A single signal (`SG_`) inside a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub name: String,
    pub multiplexer: Multiplexer,
    pub start_bit: u64,
    pub size: u64,
    pub byte_order: ByteOrder,
    pub value_type: ValueType,
    pub factor: f64,
    pub offset: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub receivers: Vec<String>,
    pub comment: Option<String>,
    /// `VAL_` value descriptions: (raw value, label).
    pub value_descriptions: Vec<(i64, String)>,
}

impl Signal {
    pub fn new(name: impl Into<String>, start_bit: u64) -> Self {
        Signal {
            name: name.into(),
            multiplexer: Multiplexer::None,
            start_bit,
            size: 8,
            byte_order: ByteOrder::LittleEndian,
            value_type: ValueType::Unsigned,
            factor: 1.0,
            offset: 0.0,
            min: 0.0,
            max: 0.0,
            unit: String::new(),
            receivers: vec!["Vector__XXX".to_string()],
            comment: None,
            value_descriptions: Vec::new(),
        }
    }
}

/// A CAN message (`BO_`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Raw message id. Bit 31 set marks an extended (29-bit) frame.
    pub id: u32,
    pub name: String,
    /// Data length code in bytes.
    pub size: u64,
    pub transmitter: String,
    pub signals: Vec<Signal>,
    pub comment: Option<String>,
}

/// The extended-frame flag stored in bit 31 of a DBC message id.
pub const EXTENDED_FLAG: u32 = 0x8000_0000;

impl Message {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Message {
            id,
            name: name.into(),
            size: 8,
            transmitter: "Vector__XXX".to_string(),
            signals: Vec::new(),
            comment: None,
        }
    }

    /// True when the message uses a 29-bit extended identifier.
    pub fn is_extended(&self) -> bool {
        self.id & EXTENDED_FLAG != 0
    }

    /// The functional identifier with the extended flag masked off.
    pub fn raw_id(&self) -> u32 {
        self.id & !EXTENDED_FLAG
    }
}

/// A network node / ECU (`BU_`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub comment: Option<String>,
}

/// A global value table (`VAL_TABLE_`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueTable {
    pub name: String,
    pub values: Vec<(i64, String)>,
}

/// A complete CAN database.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dbc {
    pub version: String,
    /// `NS_` new-symbol entries, preserved verbatim.
    pub new_symbols: Vec<String>,
    pub nodes: Vec<Node>,
    pub value_tables: Vec<ValueTable>,
    pub messages: Vec<Message>,
    /// Network-level comment (`CM_ "...";`).
    pub comment: Option<String>,
    /// Lines we do not model, preserved verbatim and re-emitted on save.
    pub extra: Vec<String>,
}

impl Dbc {
    pub fn find_message(&self, id: u32) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    pub fn message_mut(&mut self, idx: usize) -> Option<&mut Message> {
        self.messages.get_mut(idx)
    }

    /// Total number of signals across all messages.
    pub fn signal_count(&self) -> usize {
        self.messages.iter().map(|m| m.signals.len()).sum()
    }
}
