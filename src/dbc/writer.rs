//! Serialise a [`Dbc`] back into canonical DBC text.

use super::model::*;
use std::fmt::Write as _;

/// Render a [`Dbc`] to a `.dbc` document string.
pub fn write(dbc: &Dbc) -> String {
    let mut out = String::new();

    // VERSION
    let _ = writeln!(out, "VERSION \"{}\"", escape(&dbc.version));
    out.push('\n');

    // NS_ new-symbol block.
    out.push_str("NS_ :\n");
    if dbc.new_symbols.is_empty() {
        for sym in DEFAULT_NS {
            let _ = writeln!(out, "\t{sym}");
        }
    } else {
        for sym in &dbc.new_symbols {
            let _ = writeln!(out, "\t{sym}");
        }
    }
    out.push('\n');

    // Bit timing (always empty in practice).
    out.push_str("BS_:\n");
    out.push('\n');

    // Nodes.
    out.push_str("BU_:");
    for n in &dbc.nodes {
        let _ = write!(out, " {}", n.name);
    }
    out.push('\n');

    // Global value tables.
    for vt in &dbc.value_tables {
        let _ = write!(out, "VAL_TABLE_ {}", vt.name);
        write_value_pairs(&mut out, &vt.values);
        out.push_str(" ;\n");
    }
    out.push('\n');

    // Messages and their signals.
    for m in &dbc.messages {
        let _ = writeln!(out, "BO_ {} {}: {} {}", m.id, m.name, m.size, m.transmitter);
        for s in &m.signals {
            write_signal(&mut out, s);
        }
        out.push('\n');
    }

    // Comments: network, nodes, messages, signals.
    if let Some(c) = &dbc.comment {
        let _ = writeln!(out, "CM_ \"{}\";", escape(c));
    }
    for n in &dbc.nodes {
        if let Some(c) = &n.comment {
            let _ = writeln!(out, "CM_ BU_ {} \"{}\";", n.name, escape(c));
        }
    }
    for m in &dbc.messages {
        if let Some(c) = &m.comment {
            let _ = writeln!(out, "CM_ BO_ {} \"{}\";", m.id, escape(c));
        }
    }
    for m in &dbc.messages {
        for s in &m.signals {
            if let Some(c) = &s.comment {
                let _ = writeln!(out, "CM_ SG_ {} {} \"{}\";", m.id, s.name, escape(c));
            }
        }
    }

    // Passthrough lines we do not model (attribute defs, signal groups, …).
    for line in &dbc.extra {
        out.push_str(line);
        out.push('\n');
    }

    // Signal value descriptions.
    for m in &dbc.messages {
        for s in &m.signals {
            if !s.value_descriptions.is_empty() {
                let _ = write!(out, "VAL_ {} {}", m.id, s.name);
                write_value_pairs(&mut out, &s.value_descriptions);
                out.push_str(" ;\n");
            }
        }
    }

    out
}

fn write_signal(out: &mut String, s: &Signal) {
    let _ = write!(out, " SG_ {}", s.name);
    match &s.multiplexer {
        Multiplexer::None => {}
        Multiplexer::Multiplexor => out.push_str(" M"),
        Multiplexer::Multiplexed(n) => {
            let _ = write!(out, " m{n}");
        }
    }
    let order = match s.byte_order {
        ByteOrder::LittleEndian => '1',
        ByteOrder::BigEndian => '0',
    };
    let sign = match s.value_type {
        ValueType::Unsigned => '+',
        ValueType::Signed => '-',
    };
    let _ = write!(
        out,
        " : {}|{}@{}{} ({},{}) [{}|{}] \"{}\"",
        s.start_bit,
        s.size,
        order,
        sign,
        fmt_f64(s.factor),
        fmt_f64(s.offset),
        fmt_f64(s.min),
        fmt_f64(s.max),
        escape(&s.unit),
    );
    if s.receivers.is_empty() {
        out.push_str(" Vector__XXX");
    } else {
        let _ = write!(out, " {}", s.receivers.join(","));
    }
    out.push('\n');
}

fn write_value_pairs(out: &mut String, pairs: &[(i64, String)]) {
    for (v, label) in pairs {
        let _ = write!(out, " {} \"{}\"", v, escape(label));
    }
}

/// Format a float the way DBC tools do: integers without a decimal point,
/// everything else in its shortest round-trippable form.
fn fmt_f64(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

const DEFAULT_NS: [&str; 30] = [
    "NS_DESC_",
    "CM_",
    "BA_DEF_",
    "BA_",
    "VAL_",
    "CAT_DEF_",
    "CAT_",
    "FILTER",
    "BA_DEF_DEF_",
    "EV_DATA_",
    "ENVVAR_DATA_",
    "SGTYPE_",
    "SGTYPE_VAL_",
    "BA_DEF_SGTYPE_",
    "BA_SGTYPE_",
    "SIG_TYPE_REF_",
    "VAL_TABLE_",
    "SIG_GROUP_",
    "SIG_VALTYPE_",
    "SIGTYPE_VALTYPE_",
    "BO_TX_BU_",
    "BA_DEF_REL_",
    "BA_REL_",
    "BA_DEF_DEF_REL_",
    "BU_SG_REL_",
    "BU_EV_REL_",
    "BU_BO_REL_",
    "SG_MUL_VAL_",
    "NS_DESC_",
    "CM_",
];
