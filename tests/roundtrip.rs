//! Round-trip every DBC in the cantools `dbc_tests` corpus: parse -> write ->
//! parse must be semantically stable, and the writer must be idempotent.

use std::fs;
use std::path::Path;

use rsdbc::dbc::{parse, write};

#[test]
fn roundtrip_all_dbc_tests() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dbc_tests");
    if !dir.exists() {
        eprintln!("dbc_tests not present, skipping");
        return;
    }

    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("dbc"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();

    let mut failures = Vec::new();
    let mut checked = 0;

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // cp1252 is a non-UTF-8 fixture; read lossily so we still exercise it.
        let bytes = fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();

        let dbc1 = match parse(&text) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("{name}: parse error: {e}"));
                continue;
            }
        };
        let out1 = write(&dbc1);
        let dbc2 = match parse(&out1) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("{name}: re-parse error: {e}"));
                continue;
            }
        };

        if dbc1.messages != dbc2.messages {
            failures.push(format!("{name}: messages not stable"));
        }
        if dbc1.nodes != dbc2.nodes {
            failures.push(format!("{name}: nodes not stable"));
        }
        if dbc1.value_tables != dbc2.value_tables {
            failures.push(format!("{name}: value tables not stable"));
        }
        let out2 = write(&dbc2);
        if out1 != out2 {
            failures.push(format!("{name}: writer not idempotent"));
        }
        checked += 1;
    }

    eprintln!("checked {checked} files, {} failures", failures.len());
    for f in &failures {
        eprintln!("  FAIL {f}");
    }
    assert!(failures.is_empty(), "{} files failed round-trip", failures.len());
}
