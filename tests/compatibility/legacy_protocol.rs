use axum::http::{HeaderMap, HeaderValue};
use pvlog_compatibility::{
    LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, csv_record, format_legacy_date, format_legacy_time, parse_csv_record,
    parse_legacy_auth, parse_legacy_bool, parse_legacy_date, parse_legacy_time,
};
use serde::Deserialize;
use std::error::Error;

#[derive(Deserialize)]
struct Golden {
    dates: Vec<DateCase>,
    times: Vec<TimeCase>,
    booleans: Vec<BooleanCase>,
    csv: Vec<CsvCase>,
    success: SuccessCases,
    errors: Vec<ErrorCase>,
}

#[derive(Deserialize)]
struct DateCase {
    input: String,
    valid: bool,
    formatted: Option<String>,
}

#[derive(Deserialize)]
struct TimeCase {
    input: String,
    valid: bool,
    formatted: Option<String>,
}

#[derive(Deserialize)]
struct BooleanCase {
    input: String,
    value: bool,
}

#[derive(Deserialize)]
struct CsvCase {
    fields: Vec<Option<String>>,
    output: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuccessCases {
    added_output: String,
    added_status: String,
    deleted_status: String,
}

#[derive(Deserialize)]
struct ErrorCase {
    kind: String,
    status: u16,
    detail: String,
    body: String,
}

fn golden() -> Result<Golden, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../fixtures/pvoutput/legacy-format-golden.json"
    ))
}

#[test]
fn parses_query_and_form_parameters_and_legacy_authentication() -> Result<(), Box<dyn Error>> {
    let query = LegacyParameters::parse(b"key=secret%2Bvalue&sid=42&d=20240229")?;
    let auth = parse_legacy_auth(LegacyMethod::Get, &HeaderMap::new(), &query)?;
    assert_eq!(auth.api_key, "secret+value");
    assert_eq!(auth.system_id, 42);
    assert_eq!(query.required("d")?, "20240229");

    let mut headers = HeaderMap::new();
    headers.insert("x-pvoutput-apikey", HeaderValue::from_static("header-key"));
    headers.insert("x-pvoutput-systemid", HeaderValue::from_static("7"));
    let form = LegacyParameters::parse(b"d=20240229&v1=1000")?;
    let auth = parse_legacy_auth(LegacyMethod::Post, &headers, &form)?;
    assert_eq!(auth.api_key, "header-key");
    assert_eq!(auth.system_id, 7);

    assert_eq!(
        LegacyParameters::parse(b"d=1&d=2"),
        Err(LegacyProtocolError::DuplicateParameter)
    );
    assert_eq!(
        parse_legacy_auth(LegacyMethod::Post, &headers, &query),
        Err(LegacyProtocolError::QueryAuthenticationOnPost)
    );
    Ok(())
}

#[test]
fn golden_dates_times_booleans_and_csv_are_stable() -> Result<(), Box<dyn Error>> {
    let golden = golden()?;
    for case in golden.dates {
        let parsed = parse_legacy_date(&case.input);
        assert_eq!(parsed.is_ok(), case.valid, "date {}", case.input);
        if let (Ok(value), Some(expected)) = (parsed, case.formatted) {
            assert_eq!(format_legacy_date(value), expected);
        }
    }
    for case in golden.times {
        let parsed = parse_legacy_time(&case.input);
        assert_eq!(parsed.is_ok(), case.valid, "time {}", case.input);
        if let (Ok(value), Some(expected)) = (parsed, case.formatted) {
            assert_eq!(format_legacy_time(value), expected);
        }
    }
    for case in golden.booleans {
        assert_eq!(parse_legacy_bool(&case.input)?, case.value);
    }
    for case in golden.csv {
        let output = csv_record(case.fields.iter().map(|field| field.as_deref()));
        assert_eq!(output, case.output);
        assert_eq!(
            parse_csv_record(&output)?,
            case.fields
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect::<Vec<_>>()
        );
    }
    Ok(())
}

#[test]
fn golden_success_and_error_text_remains_wire_compatible() -> Result<(), Box<dyn Error>> {
    let golden = golden()?;
    assert_eq!(
        LegacySuccess::AddedOutput.body(),
        golden.success.added_output
    );
    assert_eq!(
        LegacySuccess::AddedStatus.body(),
        golden.success.added_status
    );
    assert_eq!(
        LegacySuccess::DeletedStatus.body(),
        golden.success.deleted_status
    );
    for case in golden.errors {
        let kind = match case.kind.as_str() {
            "bad_request" => LegacyErrorKind::BadRequest,
            "unauthorized" => LegacyErrorKind::Unauthorized,
            "forbidden" => LegacyErrorKind::Forbidden,
            "method_not_allowed" => LegacyErrorKind::MethodNotAllowed,
            _ => return Err("unknown golden error kind".into()),
        };
        let error = LegacyError {
            kind,
            detail: case.detail,
        };
        assert_eq!(kind.status(), case.status);
        assert_eq!(error.body(), case.body);
    }
    Ok(())
}
