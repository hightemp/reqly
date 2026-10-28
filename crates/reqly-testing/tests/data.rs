use reqly_testing::data::{DataFormat, parse_data};
use serde_json::json;

#[test]
fn csv_rows_become_string_variables() {
    let rows = parse_data(
        DataFormat::Csv,
        "user,password,note\nada,s3cret,\"hello, world\"\nbob,\"p\"\"w\",\n",
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["user"], json!("ada"));
    assert_eq!(rows[0]["note"], json!("hello, world"));
    assert_eq!(rows[1]["password"], json!("p\"w"));
    assert_eq!(rows[1]["note"], json!(""));
}

#[test]
fn csv_tolerates_bom_crlf_and_blank_lines() {
    let rows = parse_data(DataFormat::Csv, "\u{feff}a,b\r\n1,2\r\n\r\n3,4\r\n").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["a"], json!("3"));
}

#[test]
fn csv_rows_with_the_wrong_width_are_an_error() {
    let err = parse_data(DataFormat::Csv, "a,b\n1,2,3\n").unwrap_err();
    assert_eq!(err.code, "runner.data");
}

#[test]
fn json_is_an_array_of_objects() {
    let rows = parse_data(DataFormat::Json, r#"[{"id":1,"tags":["x"]},{"id":2}]"#).unwrap();
    assert_eq!(rows[0]["id"], json!(1));
    assert_eq!(rows[0]["tags"], json!(["x"]));
    assert_eq!(rows[1]["id"], json!(2));

    for bad in [r#"{"id":1}"#, "[1,2]", "nope"] {
        assert_eq!(
            parse_data(DataFormat::Json, bad).unwrap_err().code,
            "runner.data"
        );
    }
}

#[test]
fn format_from_file_name() {
    assert_eq!(DataFormat::from_path("users.CSV"), Some(DataFormat::Csv));
    assert_eq!(
        DataFormat::from_path("dir/users.json"),
        Some(DataFormat::Json)
    );
    assert_eq!(DataFormat::from_path("users.txt"), None);
}
