//! Self-contained DBC model, parser and writer.

pub mod model;
pub mod parser;
pub mod writer;

pub use model::*;
pub use parser::parse;
pub use writer::write;

#[cfg(test)]
mod tests {
    use super::*;

    const SOCIALLEDGE: &str = include_str!("../../tests/data/socialledge.dbc");

    #[test]
    fn parses_basic_structure() {
        let dbc = parse(SOCIALLEDGE).unwrap();
        assert_eq!(dbc.nodes.len(), 5);
        assert_eq!(dbc.nodes[0].name, "DBG");
        assert_eq!(dbc.messages.len(), 5);

        // Multiplexed message.
        let sonars = dbc.find_message(200).unwrap();
        assert_eq!(sonars.name, "SENSOR_SONARS");
        assert_eq!(sonars.signals.len(), 10);
        let mux = &sonars.signals[0];
        assert_eq!(mux.name, "SENSOR_SONARS_mux");
        assert_eq!(mux.multiplexer, Multiplexer::Multiplexor);
        let left = sonars
            .signals
            .iter()
            .find(|s| s.name == "SENSOR_SONARS_left")
            .unwrap();
        assert_eq!(left.multiplexer, Multiplexer::Multiplexed(0));
        let no_filt = sonars
            .signals
            .iter()
            .find(|s| s.name == "SENSOR_SONARS_no_filt_left")
            .unwrap();
        assert_eq!(no_filt.multiplexer, Multiplexer::Multiplexed(1));

        // Signed signal with a negative offset.
        let steer = dbc
            .find_message(101)
            .unwrap()
            .signals
            .iter()
            .find(|s| s.name == "MOTOR_CMD_steer")
            .unwrap();
        assert_eq!(steer.value_type, ValueType::Signed);
        assert_eq!(steer.offset, -5.0);

        // VAL_ attached to the right signal.
        let cmd = &dbc.find_message(100).unwrap().signals[0];
        assert_eq!(cmd.value_descriptions.len(), 3);
        assert_eq!(
            cmd.value_descriptions[0],
            (2, "DRIVER_HEARTBEAT_cmd_REBOOT".to_string())
        );
    }

    #[test]
    fn comments_survive_trailing_line_comments() {
        // socialledge has `CM_ ...; // trailing` lines that must not swallow the
        // following comment statement.
        let dbc = parse(SOCIALLEDGE).unwrap();
        let motor = dbc.nodes.iter().find(|n| n.name == "MOTOR").unwrap();
        assert_eq!(motor.comment.as_deref(), Some("The motor controller of the car"));
        let driver = dbc.nodes.iter().find(|n| n.name == "DRIVER").unwrap();
        assert_eq!(
            driver.comment.as_deref(),
            Some("// The driver controller driving the car //")
        );
    }

    #[test]
    fn parses_signed_and_factor() {
        let src = r#"VERSION ""
BO_ 100 Test: 8 ECU
 SG_ Current_A : 48|16@1- (0.03278,0) [0|1] "A" Vector__XXX
"#;
        let dbc = parse(src).unwrap();
        let s = &dbc.messages[0].signals[0];
        assert_eq!(s.value_type, ValueType::Signed);
        assert!((s.factor - 0.03278).abs() < 1e-9);
        assert_eq!(s.unit, "A");
    }

    #[test]
    fn round_trip_is_stable() {
        // parse -> write -> parse must yield the same model.
        let dbc1 = parse(SOCIALLEDGE).unwrap();
        let text = write(&dbc1);
        let dbc2 = parse(&text).unwrap();
        assert_eq!(dbc1.messages, dbc2.messages);
        assert_eq!(dbc1.nodes, dbc2.nodes);
        assert_eq!(dbc1.value_tables, dbc2.value_tables);
        assert_eq!(dbc1.version, dbc2.version);
    }

    #[test]
    fn write_then_parse_idempotent() {
        let dbc1 = parse(SOCIALLEDGE).unwrap();
        let t1 = write(&dbc1);
        let dbc2 = parse(&t1).unwrap();
        let t2 = write(&dbc2);
        assert_eq!(t1, t2, "writer must be idempotent");
    }

    #[test]
    fn parses_value_tables_including_empty() {
        let src = r#"VERSION ""
VAL_TABLE_ Table3 16 "16" 7 "7" 2 "2" 0 "0" ;
VAL_TABLE_ Empty ;
VAL_TABLE_ Table1 1 "One" 0 "Zero" ;
"#;
        let dbc = parse(src).unwrap();
        assert_eq!(dbc.value_tables.len(), 3);
        assert_eq!(dbc.value_tables[0].name, "Table3");
        assert_eq!(dbc.value_tables[0].values.len(), 4);
        assert_eq!(dbc.value_tables[1].name, "Empty");
        assert!(dbc.value_tables[1].values.is_empty());
        assert_eq!(dbc.value_tables[2].values, vec![(1, "One".into()), (0, "Zero".into())]);
    }

    #[test]
    fn comments_attach_to_objects() {
        let src = r#"VERSION ""
BU_: ECU
BO_ 100 Test: 8 ECU
 SG_ Sig : 0|8@1+ (1,0) [0|0] "" Vector__XXX
CM_ "network level";
CM_ BU_ ECU "the controller";
CM_ BO_ 100 "a message";
CM_ SG_ 100 Sig "a signal";
"#;
        let dbc = parse(src).unwrap();
        assert_eq!(dbc.comment.as_deref(), Some("network level"));
        assert_eq!(dbc.nodes[0].comment.as_deref(), Some("the controller"));
        assert_eq!(dbc.messages[0].comment.as_deref(), Some("a message"));
        assert_eq!(
            dbc.messages[0].signals[0].comment.as_deref(),
            Some("a signal")
        );
    }
}
