//! End-to-end tests for WASM component plugins: build the example guests
//! with the real toolchain, load them through the host, and call them.

mod common;

use loadr_core::ProtocolResponse;
use loadr_plugin_api::{PluginAssertion, PluginExtractor, WasmAssertion, WasmExtractor};

fn extractor_path() -> std::path::PathBuf {
    common::build_wasm_guest("wasm-extractor", "loadr_wasm_extractor.wasm")
}

fn assertion_path() -> std::path::PathBuf {
    common::build_wasm_guest("wasm-assertion", "loadr_wasm_assertion.wasm")
}

#[test]
fn extractor_describe_and_extract() {
    let plugin = WasmExtractor::load(&extractor_path()).expect("load extractor");
    let info = plugin.info();
    assert_eq!(info.name, "upper-boundary");
    assert_eq!(info.kind, "extractor");
    assert!(!info.version.is_empty());
    assert!(!info.description.is_empty());

    let response = ProtocolResponse {
        status: 200,
        body: bytes::Bytes::from_static(b"prefix <<csrf-token-99>> suffix"),
        ..Default::default()
    };
    let config = serde_json::json!({"left": "<<", "right": ">>"});

    // The example uppercases the match — distinguishable from a builtin
    // boundary extractor.
    let value = plugin
        .extract(&response, &config)
        .expect("extract succeeds");
    assert_eq!(value.as_deref(), Some("CSRF-TOKEN-99"));

    // No match -> None.
    let miss = plugin
        .extract(&response, &serde_json::json!({"left": "[[", "right": "]]"}))
        .expect("extract succeeds");
    assert_eq!(miss, None);

    // Trait object name comes from describe().
    assert_eq!(PluginExtractor::name(&plugin), "upper-boundary");
}

#[test]
fn extractor_repeated_calls_reuse_instance() {
    let plugin = WasmExtractor::load(&extractor_path()).expect("load extractor");
    let config = serde_json::json!({"left": "(", "right": ")"});
    for i in 0..10 {
        let body = format!("x (value-{i}) y");
        let response = ProtocolResponse {
            body: bytes::Bytes::from(body),
            ..Default::default()
        };
        let value = plugin.extract(&response, &config).expect("extract");
        assert_eq!(value, Some(format!("VALUE-{i}")));
    }
}

#[test]
fn assertion_verdicts_both_ways() {
    let plugin = WasmAssertion::load(&assertion_path()).expect("load assertion");
    let info = plugin.info();
    assert_eq!(info.name, "max-body-size");
    assert_eq!(info.kind, "assertion");

    let mut response = ProtocolResponse {
        status: 200,
        body: bytes::Bytes::from(vec![b'a'; 10]),
        ..Default::default()
    };
    response.timings.duration_ms = 12.5;

    let config = serde_json::json!({"max_body_bytes": 16});
    let (pass, detail) = plugin.check(&response, &config).expect("check");
    assert!(pass, "10 bytes <= 16: {detail}");
    assert!(detail.contains("10"), "detail mentions size: {detail}");

    response.body = bytes::Bytes::from(vec![b'a'; 64]);
    let (pass, detail) = plugin.check(&response, &config).expect("check");
    assert!(!pass, "64 bytes > 16 must fail");
    assert!(detail.contains("exceeds"), "detail explains: {detail}");

    // Broken config fails the check with a reason instead of trapping.
    let (pass, detail) = plugin
        .check(&response, &serde_json::json!({"nope": true}))
        .expect("check");
    assert!(!pass);
    assert!(detail.contains("invalid config"), "{detail}");
}

#[test]
fn probe_info_reads_meta_without_kind_knowledge() {
    let info = loadr_plugin_api::probe_info(&assertion_path()).expect("probe");
    assert_eq!(info.name, "max-body-size");
    assert_eq!(info.kind, "assertion");
}

#[test]
fn kind_mismatch_is_rejected() {
    // Loading the assertion component as an extractor must fail cleanly.
    let err = WasmExtractor::load(&assertion_path()).expect_err("wrong world");
    let msg = err.to_string();
    assert!(!msg.is_empty());
}

#[test]
fn missing_component_errors_cleanly() {
    let err = WasmExtractor::load(std::path::Path::new("/nonexistent/plugin.wasm"))
        .expect_err("missing file");
    assert!(
        matches!(err, loadr_plugin_api::PluginError::Load { .. }),
        "{err}"
    );
}
