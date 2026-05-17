use std::path::Path;

use hief::mcp::tools::{
    validate_relative_tool_path, validate_required_param, validate_top_k_param,
    ToolValidationPayload, MCP_MAX_TOP_K,
};
use rmcp::model::ErrorCode;

fn decode_payload(err: &rmcp::ErrorData) -> ToolValidationPayload {
    serde_json::from_value(err.data.clone().expect("validation payload"))
        .expect("typed validation payload")
}

#[test]
fn test_top_k_above_bound_returns_typed_validation_error() {
    let err = validate_top_k_param("search_code", "top_k", Some(MCP_MAX_TOP_K + 1), 10)
        .expect_err("top_k above bound should fail");

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

    let payload = decode_payload(&err);
    assert_eq!(payload.error_class, "invalid_params");
    assert_eq!(payload.tool, "search_code");
    assert_eq!(payload.parameter, "top_k");
    assert_eq!(payload.reason, "out_of_range");
    assert!(payload.recoverable);

    let hint = payload.correction_hint.expect("correction hint");
    assert_eq!(hint.min, Some(1));
    assert_eq!(hint.max, Some(MCP_MAX_TOP_K));
}

#[test]
fn test_missing_required_parameter_returns_predictable_invalid_params_shape() {
    let err = validate_required_param("search_code", "query", None)
        .expect_err("missing required query should fail");

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

    let payload = decode_payload(&err);
    assert_eq!(payload.error_class, "invalid_params");
    assert_eq!(payload.tool, "search_code");
    assert_eq!(payload.parameter, "query");
    assert_eq!(payload.reason, "missing_required");
    assert!(payload.recoverable);
    assert!(payload.correction_hint.is_some());
}

#[test]
fn test_path_traversal_returns_security_error_without_echoing_input() {
    let err = validate_relative_tool_path(
        Path::new("/tmp/hief-project"),
        "related_files",
        "file",
        Some("../secrets.txt"),
    )
    .expect_err("traversal should fail");

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(!err.message.contains("../secrets.txt"));

    let payload = decode_payload(&err);
    assert_eq!(payload.error_class, "security_error");
    assert_eq!(payload.tool, "related_files");
    assert_eq!(payload.parameter, "file");
    assert_eq!(payload.reason, "path_traversal");

    let hint = payload.correction_hint.expect("security correction hint");
    assert_eq!(hint.example.as_deref(), Some("src/lib.rs"));
}

#[test]
fn test_valid_inputs_remain_unchanged() {
    let query = validate_required_param("search_code", "query", Some("symbol::lookup"))
        .expect("valid query");
    let top_k = validate_top_k_param("search_code", "top_k", Some(25), 10).expect("valid top_k");
    let path = validate_relative_tool_path(
        Path::new("/tmp/hief-project"),
        "related_files",
        "file",
        Some("src/lib.rs"),
    )
    .expect("valid path");

    assert_eq!(query, "symbol::lookup");
    assert_eq!(top_k, 25);
    assert_eq!(path, Path::new("/tmp/hief-project").join("src/lib.rs"));
}