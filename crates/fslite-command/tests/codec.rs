use fslite_command::{Command, CommandOutput};
use fslite_core::{ReadOptions, StatOptions, VirtualPath, WriteOptions};

#[test]
fn stat_round_trips_through_json() {
    let command = Command::Stat {
        path: VirtualPath::parse("/a.txt").unwrap(),
        options: StatOptions::default(),
    };
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}

#[test]
fn write_encodes_its_payload_as_base64_not_a_number_array() {
    let command = Command::Write {
        path: VirtualPath::parse("/a.txt").unwrap(),
        bytes: b"\x00\x01binary".to_vec(),
        options: WriteOptions::default(),
    };
    let json = serde_json::to_value(&command).unwrap();
    let bytes_field = &json["write"]["bytes"];
    assert!(
        bytes_field.is_string(),
        "expected base64 string, got {bytes_field:?}"
    );
}

#[test]
fn read_options_round_trip_with_a_byte_range() {
    let command = Command::Read {
        path: VirtualPath::parse("/a.txt").unwrap(),
        options: ReadOptions::default().range(Some(fslite_core::ByteRange::new(0, 10))),
    };
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}

#[test]
fn command_output_content_round_trips_bytes_as_base64() {
    let output = CommandOutput::Content {
        logical_length: 5,
        revision: fslite_core::Revision::INITIAL,
        range: fslite_core::ByteRange::new(0, 5),
        bytes: b"hello".to_vec(),
    };
    let json = serde_json::to_value(&output).unwrap();
    assert!(json["content"]["bytes"].is_string());
    let back: CommandOutput = serde_json::from_value(json).unwrap();
    assert_eq!(output, back);
}

#[test]
fn batch_wraps_core_batch_operations_verbatim() {
    let ops = vec![fslite_core::BatchOperation::Mkdir {
        path: VirtualPath::parse("/a").unwrap(),
        options: Default::default(),
    }];
    let command = Command::Batch(ops.clone());
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}
