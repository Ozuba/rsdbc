//! A pragmatic, dependency-free parser for the subset of the DBC format that
//! matters for editing: version, nodes, value tables, messages, signals,
//! comments and value descriptions. Anything else is preserved verbatim.

use super::model::*;

/// Parse a `.dbc` document into a [`Dbc`]. Unrecognised, well-formed lines are
/// kept in [`Dbc::extra`]; the parse only fails on structurally broken input it
/// cannot make sense of, and even then returns best-effort partial data.
pub fn parse(input: &str) -> Result<Dbc, String> {
    let mut dbc = Dbc::default();

    // Normalise line endings and collect into an indexable buffer so we can
    // consume multi-line constructs (the `NS_` block, continued statements).
    let raw_lines: Vec<&str> = input.split('\n').map(|l| l.trim_end_matches('\r')).collect();

    // Deferred attachments: comments and value descriptions reference messages
    // and signals that may appear earlier or later, so resolve them after the
    // structural pass.
    let mut pending_comments: Vec<Comment> = Vec::new();
    let mut pending_values: Vec<ValDef> = Vec::new();

    let mut i = 0;
    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // `NS_` introduces an indented block of new-symbol entries.
        if trimmed.starts_with("NS_") {
            i += 1;
            while i < raw_lines.len() {
                let l = raw_lines[i];
                if l.trim().is_empty() {
                    break;
                }
                // Block entries are indented; a non-indented line ends the block.
                if !l.starts_with([' ', '\t']) {
                    break;
                }
                dbc.new_symbols.push(l.trim().to_string());
                i += 1;
            }
            continue;
        }

        // Bit-timing section: not editable, emitted as a fixed line on save.
        if trimmed.starts_with("BS_:") || trimmed.starts_with("BS_ :") {
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("VERSION ") {
            dbc.version = unquote(rest.trim()).unwrap_or_default();
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("BU_:") {
            for tok in rest.split_whitespace() {
                dbc.nodes.push(Node {
                    name: tok.to_string(),
                    comment: None,
                });
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("BO_ ") {
            match parse_message(trimmed) {
                Ok(msg) => dbc.messages.push(msg),
                Err(e) => return Err(format!("line {}: {e}", i + 1)),
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("SG_ ") {
            let msg = dbc
                .messages
                .last_mut()
                .ok_or_else(|| format!("line {}: SG_ without preceding BO_", i + 1))?;
            match parse_signal(trimmed) {
                Ok(sig) => msg.signals.push(sig),
                Err(e) => return Err(format!("line {}: {e}", i + 1)),
            }
            i += 1;
            continue;
        }

        // The remaining constructs are `;`-terminated and may span several
        // physical lines (multi-line comments). Join them first.
        if needs_semicolon(trimmed) {
            let (statement, consumed) = join_statement(&raw_lines, i);
            i += consumed;
            let stmt = statement.trim();

            if let Some(rest) = stmt.strip_prefix("VAL_TABLE_ ") {
                if let Some(vt) = parse_value_table(rest) {
                    dbc.value_tables.push(vt);
                } else {
                    dbc.extra.push(stmt.to_string());
                }
            } else if let Some(rest) = stmt.strip_prefix("VAL_ ") {
                if let Some(v) = parse_val(rest) {
                    pending_values.push(v);
                } else {
                    dbc.extra.push(stmt.to_string());
                }
            } else if let Some(rest) = stmt.strip_prefix("CM_ ") {
                if let Some(c) = parse_comment(rest) {
                    pending_comments.push(c);
                } else {
                    dbc.extra.push(stmt.to_string());
                }
            } else {
                dbc.extra.push(stmt.to_string());
            }
            continue;
        }

        // Anything else: preserve verbatim.
        dbc.extra.push(trimmed.to_string());
        i += 1;
    }

    attach_comments(&mut dbc, pending_comments);
    attach_values(&mut dbc, pending_values);

    Ok(dbc)
}

// --- structural parsers ----------------------------------------------------

fn parse_message(line: &str) -> Result<Message, String> {
    // BO_ <id> <name>: <size> <transmitter>
    let rest = line.strip_prefix("BO_ ").unwrap_or(line);
    let mut parts = rest.split_whitespace();
    let id_str = parts.next().ok_or("BO_ missing id")?;
    let id: u32 = id_str.parse().map_err(|_| format!("bad message id '{id_str}'"))?;
    let name_tok = parts.next().ok_or("BO_ missing name")?;
    let name = name_tok.trim_end_matches(':').to_string();
    let size_str = parts.next().ok_or("BO_ missing size")?;
    let size: u64 = size_str.parse().map_err(|_| format!("bad dlc '{size_str}'"))?;
    let transmitter = parts.next().unwrap_or("Vector__XXX").to_string();

    Ok(Message {
        id,
        name,
        size,
        transmitter,
        signals: Vec::new(),
        comment: None,
    })
}

fn parse_signal(line: &str) -> Result<Signal, String> {
    let rest = line.strip_prefix("SG_ ").unwrap_or(line);
    // Split the header (name + optional mux) from the spec at the " : " colon.
    let colon = rest.find(':').ok_or("SG_ missing ':'")?;
    let (head, spec) = rest.split_at(colon);
    let spec = &spec[1..]; // drop ':'

    let mut head_toks = head.split_whitespace();
    let name = head_toks.next().ok_or("SG_ missing name")?.to_string();
    let multiplexer = match head_toks.next() {
        None => Multiplexer::None,
        Some("M") => Multiplexer::Multiplexor,
        Some(m) if m.starts_with('m') => {
            let digits: String = m[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            digits
                .parse::<u64>()
                .map(Multiplexer::Multiplexed)
                .unwrap_or(Multiplexer::None)
        }
        Some(_) => Multiplexer::None,
    };

    // <start>|<size>@<order><sign> (<factor>,<offset>) [<min>|<max>] "<unit>" <recv...>
    let paren = spec.find('(').ok_or("SG_ missing '('")?;
    let bitspec = spec[..paren].trim();
    let bar = bitspec.find('|').ok_or("SG_ missing '|'")?;
    let start_bit: u64 = bitspec[..bar].trim().parse().map_err(|_| "bad start bit")?;
    let at = bitspec.find('@').ok_or("SG_ missing '@'")?;
    let size: u64 = bitspec[bar + 1..at].trim().parse().map_err(|_| "bad size")?;
    let order_sign = &bitspec[at + 1..];
    let byte_order = if order_sign.starts_with('0') {
        ByteOrder::BigEndian
    } else {
        ByteOrder::LittleEndian
    };
    let value_type = if order_sign.contains('-') {
        ValueType::Signed
    } else {
        ValueType::Unsigned
    };

    let close_paren = spec.find(')').ok_or("SG_ missing ')'")?;
    let (factor, offset) = {
        let inner = &spec[paren + 1..close_paren];
        let comma = inner.find(',').ok_or("SG_ bad (factor,offset)")?;
        (
            inner[..comma].trim().parse().map_err(|_| "bad factor")?,
            inner[comma + 1..].trim().parse().map_err(|_| "bad offset")?,
        )
    };

    let after = &spec[close_paren + 1..];
    let lb = after.find('[').ok_or("SG_ missing '['")?;
    let rb = after.find(']').ok_or("SG_ missing ']'")?;
    let (min, max) = {
        let inner = &after[lb + 1..rb];
        let bar = inner.find('|').ok_or("SG_ bad [min|max]")?;
        (
            inner[..bar].trim().parse().map_err(|_| "bad min")?,
            inner[bar + 1..].trim().parse().map_err(|_| "bad max")?,
        )
    };

    let tail = &after[rb + 1..];
    let (unit, rest_after_unit) = match read_first_quoted(tail) {
        Some((u, r)) => (u, r),
        None => (String::new(), tail),
    };
    let receivers: Vec<String> = rest_after_unit
        .split([' ', '\t', ','])
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();
    let receivers = if receivers.is_empty() {
        vec!["Vector__XXX".to_string()]
    } else {
        receivers
    };

    Ok(Signal {
        name,
        multiplexer,
        start_bit,
        size,
        byte_order,
        value_type,
        factor,
        offset,
        min,
        max,
        unit,
        receivers,
        comment: None,
        value_descriptions: Vec::new(),
    })
}

fn parse_value_table(rest: &str) -> Option<ValueTable> {
    let rest = rest.trim().trim_end_matches(';').trim();
    if rest.is_empty() {
        return None;
    }
    // A table may have no entries (`VAL_TABLE_ Name ;`), so the name is not
    // necessarily followed by whitespace.
    let (name, after) = match rest.find(char::is_whitespace) {
        Some(p) => (rest[..p].to_string(), &rest[p..]),
        None => (rest.to_string(), ""),
    };
    Some(ValueTable {
        name,
        values: parse_value_pairs(after),
    })
}

struct ValDef {
    message_id: u32,
    signal: String,
    values: Vec<(i64, String)>,
}

fn parse_val(rest: &str) -> Option<ValDef> {
    let rest = rest.trim().trim_end_matches(';').trim();
    let mut it = rest.splitn(3, char::is_whitespace);
    let id: u32 = it.next()?.parse().ok()?;
    let signal = it.next()?.to_string();
    let pairs = it.next().unwrap_or("");
    Some(ValDef {
        message_id: id,
        signal,
        values: parse_value_pairs(pairs),
    })
}

enum Comment {
    Network(String),
    Node(String, String),
    Message(u32, String),
    Signal(u32, String, String),
}

fn parse_comment(rest: &str) -> Option<Comment> {
    let rest = rest.trim();
    let body = rest.strip_suffix(';').unwrap_or(rest).trim();
    if let Some(r) = body.strip_prefix("BU_ ") {
        let (node, after) = take_word(r.trim());
        let text = read_first_quoted(after).map(|(s, _)| s)?;
        Some(Comment::Node(node, text))
    } else if let Some(r) = body.strip_prefix("BO_ ") {
        let (ids, after) = take_word(r.trim());
        let id: u32 = ids.parse().ok()?;
        let text = read_first_quoted(after).map(|(s, _)| s)?;
        Some(Comment::Message(id, text))
    } else if let Some(r) = body.strip_prefix("SG_ ") {
        let (ids, after) = take_word(r.trim());
        let id: u32 = ids.parse().ok()?;
        let (sig, after2) = take_word(after.trim());
        let text = read_first_quoted(after2).map(|(s, _)| s)?;
        Some(Comment::Signal(id, sig, text))
    } else {
        // Network comment: just a quoted string.
        let text = read_first_quoted(body).map(|(s, _)| s)?;
        Some(Comment::Network(text))
    }
}

fn attach_comments(dbc: &mut Dbc, comments: Vec<Comment>) {
    for c in comments {
        match c {
            Comment::Network(t) => dbc.comment = Some(t),
            Comment::Node(name, t) => {
                if let Some(n) = dbc.nodes.iter_mut().find(|n| n.name == name) {
                    n.comment = Some(t);
                }
            }
            Comment::Message(id, t) => {
                if let Some(m) = dbc.messages.iter_mut().find(|m| m.id == id) {
                    m.comment = Some(t);
                }
            }
            Comment::Signal(id, sig, t) => {
                if let Some(m) = dbc.messages.iter_mut().find(|m| m.id == id) {
                    if let Some(s) = m.signals.iter_mut().find(|s| s.name == sig) {
                        s.comment = Some(t);
                    }
                }
            }
        }
    }
}

fn attach_values(dbc: &mut Dbc, vals: Vec<ValDef>) {
    for v in vals {
        if let Some(m) = dbc.messages.iter_mut().find(|m| m.id == v.message_id) {
            if let Some(s) = m.signals.iter_mut().find(|s| s.name == v.signal) {
                s.value_descriptions = v.values;
            }
        }
    }
}

// --- small lexing helpers --------------------------------------------------

/// Keywords for statements terminated by `;` (and thus possibly multi-line).
fn needs_semicolon(trimmed: &str) -> bool {
    const KW: [&str; 18] = [
        "CM_", "VAL_TABLE_", "VAL_", "BA_DEF_DEF_REL_", "BA_DEF_DEF_", "BA_DEF_REL_", "BA_DEF_",
        "BA_REL_", "BA_", "SIG_GROUP_", "SIG_VALTYPE_", "BO_TX_BU_", "EV_DATA_", "ENVVAR_DATA_",
        "SG_MUL_VAL_", "FILTER", "CAT_", "SGTYPE_",
    ];
    KW.iter().any(|k| trimmed.starts_with(k))
}

/// Accumulate physical lines from `start` until the statement's terminating
/// `;` (the first one outside a quoted string). The statement is truncated at
/// that `;`, discarding any trailing `// line comment`. Returns the joined
/// statement and the number of physical lines consumed.
fn join_statement(lines: &[&str], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut consumed = 0;
    let mut i = start;
    while i < lines.len() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(lines[i]);
        consumed += 1;
        if let Some(pos) = unquoted_semicolon(&out) {
            out.truncate(pos + 1);
            break;
        }
        i += 1;
    }
    (out, consumed)
}

/// Byte offset of the first `;` that lies outside a double-quoted string,
/// honouring `\"` escapes. `;`, `"` and `\` are ASCII so byte indexing is safe.
fn unquoted_semicolon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_quote => {
                i += 2;
                continue;
            }
            b'"' => in_quote = !in_quote,
            b';' if !in_quote => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Strip surrounding double quotes from a token if present.
fn unquote(s: &str) -> Option<String> {
    read_first_quoted(s).map(|(content, _)| content)
}

/// Split off the first whitespace-delimited word, returning (word, remainder).
fn take_word(s: &str) -> (String, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(pos) => (s[..pos].to_string(), &s[pos..]),
        None => (s.to_string(), ""),
    }
}

/// Read the first double-quoted string in `s`, honouring `\"` and `\\` escapes.
/// Returns the unescaped content and the slice following the closing quote.
/// Operates on `char`s so multi-byte UTF-8 (e.g. unit symbols) is preserved.
fn read_first_quoted(s: &str) -> Option<(String, &str)> {
    let start = s.find('"')?;
    let inner_start = start + 1;
    let mut out = String::new();
    let mut chars = s[inner_start..].char_indices();
    while let Some((idx, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, escaped)) = chars.next() {
                out.push(escaped);
            }
            continue;
        }
        if c == '"' {
            let after = inner_start + idx + c.len_utf8();
            return Some((out, &s[after..]));
        }
        out.push(c);
    }
    // Unterminated quote: take the rest.
    Some((out, ""))
}

/// Parse a sequence of `<number> "<label>"` pairs.
fn parse_value_pairs(s: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        // Read a number token.
        let (num_tok, after_num) = take_word(rest);
        if num_tok.is_empty() {
            break;
        }
        let value: i64 = match num_tok.parse() {
            Ok(v) => v,
            Err(_) => break,
        };
        // Read the quoted label.
        match read_first_quoted(after_num) {
            Some((label, after_label)) => {
                out.push((value, label));
                rest = after_label.trim();
            }
            None => break,
        }
    }
    out
}
