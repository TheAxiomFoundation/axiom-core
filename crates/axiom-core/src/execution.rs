//! Strict request admission and receipts over the native Axiom execution API.
//!
//! Context hashes identify private execution configuration, not a household,
//! dataset, result, authenticated publisher, or legal determination. This layer
//! does not add a policy evaluator, static type soundness, or a runtime sandbox.

use axiom_rules_engine::api::{
    CompiledExecutionRequest, ExecutionMode, ExecutionQuery, ExecutionResponse, RulePin,
    execute_compiled_request,
};
use axiom_rules_engine::spec::DatasetBindingOptions;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

use crate::{EngineIdentity, Error, Result, WIRE_VERSION, digest, engine_identity};

const RECEIPT_FORMAT: &str = "axiom/execution-receipt/v0";

#[derive(Clone, Debug, Serialize)]
pub struct Scenario {
    /// Native pin values, sorted by their exact local derived-rule name.
    /// Decimal representations follow the pinned engine's wire serialization;
    /// this is not a claim of semantic equivalence between different literals.
    pub pins: Vec<RulePin>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionContext {
    pub bundle_sha256: String,
    pub artifact_sha256: String,
    pub engine: EngineIdentity,
    pub wire_version: &'static str,
    pub mode: ExecutionMode,
    pub scenario: Scenario,
    pub scenario_sha256: String,
    /// Native parsed queries in requested order. Entity identifiers make this
    /// private context even though dataset values are deliberately excluded.
    pub queries: Vec<ExecutionQuery>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionReceipt {
    pub format: &'static str,
    pub assurance: &'static str,
    pub context_sha256: String,
    pub context: ExecutionContext,
    /// Serialize the native engine response without projecting or recreating
    /// fields: all present and future native traces survive this boundary.
    pub result: ExecutionResponse,
}

/// Parse the pinned engine's request type without silently losing fields.
///
/// The shared JSON validator rejects duplicate keys before any Value parse can
/// discard them. Native raw-byte deserialization supplies type validation;
/// narrow enum guards close serde_ignored's tagged/flattened buffering gaps.
pub fn parse_request(raw: &str) -> Result<CompiledExecutionRequest> {
    crate::validate_json(raw)?;
    let value: Value =
        serde_json::from_str(raw).map_err(|error| Error::new("invalid_json", error.to_string()))?;
    reject_assessment_date(&value, "$")?;

    let mut deserializer = serde_json::Deserializer::from_str(raw);
    let mut ignored = Vec::new();
    let mut request: CompiledExecutionRequest =
        serde_ignored::deserialize(&mut deserializer, |path| ignored.push(path.to_string()))
            .map_err(|error| Error::new("invalid_request", error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| Error::new("invalid_json", error.to_string()))?;
    if !ignored.is_empty() {
        return Err(Error::new(
            "unknown_request_field",
            format!("unknown request fields: {}", ignored.join(", ")),
        ));
    }
    validate_buffered_enum_fields(&value)?;
    validate_intervals(&request)?;

    let mut names = BTreeSet::new();
    for pin in &request.pins {
        if !names.insert(pin.rule.as_str()) {
            return Err(Error::new(
                "duplicate_pin",
                format!("rule `{}` is pinned more than once", pin.rule),
            ));
        }
    }
    // With duplicate rules rejected, order has no effect on engine semantics.
    // Use the same normalized order in native execution and context hashing.
    request
        .pins
        .sort_by(|left, right| left.rule.cmp(&right.rule));
    Ok(request)
}

/// Execute an already verified development bundle through the pinned runtime.
/// Pins affect the cloned execution artifact only; stored bytes stay unchanged.
pub fn execute(
    bundle: &crate::bundle::VerifiedBundle,
    request_json: &str,
) -> Result<ExecutionReceipt> {
    let request = parse_request(request_json)?;
    let manifest = bundle.manifest();
    if manifest.engine != engine_identity()? {
        return Err(Error::new(
            "incompatible_engine",
            "bundle engine identity does not match the compiled runtime",
        ));
    }

    // Reuse the engine's strict entity binder. The execution API currently
    // binds in compatibility mode and exposes no options argument; a strict
    // preflight over the same unmodified input catalog closes that gap.
    let program = bundle
        .artifact()
        .program
        .to_program()
        .map_err(|error| Error::new("invalid_program", error.to_string()))?;
    request
        .dataset
        .to_dataset_for_program_with_options(&program, DatasetBindingOptions::strict())
        .map_err(|error| Error::new("invalid_dataset", error.to_string()))?;

    let context = make_context(
        bundle.digest(),
        &manifest.artifact_sha256,
        &manifest.engine,
        &request,
    )?;
    let context_sha256 = digest(&context)?;
    let result = execute_compiled_request(bundle.artifact().clone(), request)
        .map_err(|error| Error::new("execution_failed", error.to_string()))?;
    Ok(ExecutionReceipt {
        format: RECEIPT_FORMAT,
        assurance: "development_unsigned",
        context_sha256,
        context,
        result,
    })
}

fn make_context(
    bundle_sha256: &str,
    artifact_sha256: &str,
    engine: &EngineIdentity,
    request: &CompiledExecutionRequest,
) -> Result<ExecutionContext> {
    let scenario = Scenario {
        pins: request.pins.clone(),
    };
    Ok(ExecutionContext {
        bundle_sha256: bundle_sha256.to_owned(),
        artifact_sha256: artifact_sha256.to_owned(),
        engine: engine.clone(),
        wire_version: WIRE_VERSION,
        mode: request.mode.clone(),
        scenario_sha256: digest(&scenario)?,
        scenario,
        queries: request.queries.clone(),
    })
}

fn reject_assessment_date(value: &Value, path: &str) -> Result<()> {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let child_path = format!("{path}.{key}");
                if key == "assessment_date" {
                    return Err(Error::new(
                        "unsupported_assessment_date",
                        format!(
                            "{child_path} is unsupported, including null: the pinned engine does not implement assessment-time version selection"
                        ),
                    ));
                }
                reject_assessment_date(value, &child_path)?;
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                reject_assessment_date(value, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn require_keys(value: &Value, allowed: &[&str], path: &str) -> Result<()> {
    // Native deserialization precedes this function and has already checked
    // object shape, required keys, discriminators, and their value types.
    if let Some(fields) = value.as_object() {
        for key in fields.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(Error::new(
                    "unknown_request_field",
                    format!("unknown request field {path}.{key}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_buffered_enum_fields(value: &Value) -> Result<()> {
    if let Some(inputs) = value.pointer("/dataset/inputs").and_then(Value::as_array) {
        for (index, input) in inputs.iter().enumerate() {
            if let Some(scalar) = input.get("value") {
                require_keys(
                    scalar,
                    &["kind", "value"],
                    &format!("$.dataset.inputs[{index}].value"),
                )?;
            }
        }
    }
    if let Some(pins) = value.get("pins").and_then(Value::as_array) {
        for (index, pin) in pins.iter().enumerate() {
            if let Some(scalar) = pin.get("value") {
                require_keys(
                    scalar,
                    &["kind", "value"],
                    &format!("$.pins[{index}].value"),
                )?;
            }
        }
    }
    if let Some(queries) = value.get("queries").and_then(Value::as_array) {
        for (index, query) in queries.iter().enumerate() {
            if let Some(period) = query.get("period") {
                let allowed = if period.get("period_kind").and_then(Value::as_str) == Some("custom")
                {
                    &["period_kind", "start", "end", "name"][..]
                } else {
                    &["period_kind", "start", "end"][..]
                };
                require_keys(period, allowed, &format!("$.queries[{index}].period"))?;
            }
        }
    }
    Ok(())
}

fn validate_intervals(request: &CompiledExecutionRequest) -> Result<()> {
    for (index, query) in request.queries.iter().enumerate() {
        if query.period.start > query.period.end {
            return Err(Error::new(
                "invalid_interval",
                format!("queries[{index}].period ends before it starts"),
            ));
        }
    }
    for (index, input) in request.dataset.inputs.iter().enumerate() {
        if input.interval.start > input.interval.end {
            return Err(Error::new(
                "invalid_interval",
                format!("dataset.inputs[{index}].interval ends before it starts"),
            ));
        }
    }
    for (index, relation) in request.dataset.relations.iter().enumerate() {
        if relation.interval.start > relation.interval.end {
            return Err(Error::new(
                "invalid_interval",
                format!("dataset.relations[{index}].interval ends before it starts"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_rules_engine::compile::CompiledProgramArtifact;
    use axiom_rules_engine::spec::ScalarValueSpec;
    use serde_json::json;

    fn request_value() -> Value {
        json!({
            "mode": "explain",
            "dataset": {
                "inputs": [{
                    "name": "zz:policies/demo#input.income",
                    "entity": "Household", "entity_id": "household:1",
                    "interval": {"start":"2026-01-01", "end":"2026-01-31"},
                    "value": {"kind":"decimal", "value":"10.00"}
                }],
                "relations": [{
                    "name":"zz:policies/demo#relation.members",
                    "tuple":["person:1", "household:1"],
                    "interval":{"start":"2026-01-01", "end":"2026-01-31"}
                }]
            },
            "queries": [{
                "entity_id":"household:1",
                "period":{"period_kind":"month", "start":"2026-01-01", "end":"2026-01-31"},
                "outputs":["zz:policies/demo#benefit"]
            }],
            "pins":[{"rule":"benefit", "value":{"kind":"decimal", "value":"0"}}]
        })
    }

    fn parse(value: &Value) -> Result<CompiledExecutionRequest> {
        parse_request(&serde_json::to_string(value).unwrap())
    }

    #[test]
    fn keeps_native_pins_and_accepts_engine_decimal_integer_wire_form() {
        let mut value = request_value();
        value["pins"][0]["value"]["value"] = json!(0);
        let request = parse(&value).unwrap();
        assert_eq!(request.pins[0].rule, "benefit");
        assert_eq!(
            request.pins[0].value,
            ScalarValueSpec::Decimal { value: "0".into() }
        );
        assert_eq!(request.queries[0].outputs, ["zz:policies/demo#benefit"]);
    }

    #[test]
    fn rejects_unknown_fields_in_structs_tagged_scalars_and_flattened_periods() {
        let paths = [
            "",
            "/dataset",
            "/dataset/inputs/0",
            "/dataset/inputs/0/interval",
            "/dataset/inputs/0/value",
            "/dataset/relations/0",
            "/dataset/relations/0/interval",
            "/queries/0",
            "/queries/0/period",
            "/pins/0",
            "/pins/0/value",
        ];
        for path in paths {
            let mut value = request_value();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("misspelled".into(), json!({"nested": true}));
            let error = parse(&value).unwrap_err();
            assert_eq!(error.code, "unknown_request_field", "{path}: {error}");
            assert!(error.message.contains("misspelled"), "{path}: {error}");
        }
    }

    #[test]
    fn custom_period_name_is_allowed_only_on_custom_variant() {
        let mut value = request_value();
        value["queries"][0]["period"]["name"] = json!("synthetic-window");
        assert_eq!(parse(&value).unwrap_err().code, "unknown_request_field");
        value["queries"][0]["period"]["period_kind"] = json!("custom");
        assert!(parse(&value).is_ok());
    }

    #[test]
    fn rejects_assessment_date_even_when_null_or_nested() {
        for path in ["", "/queries/0", "/pins/0/value"] {
            for assessment in [Value::Null, json!("2026-01-31")] {
                let mut value = request_value();
                value
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert("assessment_date".into(), assessment);
                assert_eq!(
                    parse(&value).unwrap_err().code,
                    "unsupported_assessment_date"
                );
            }
        }
    }

    #[test]
    fn rejects_duplicate_json_keys_at_every_depth_and_trailing_json() {
        let duplicates = [
            r#"{"mode":"explain","mode":"fast","dataset":{},"queries":[]}"#,
            r#"{"mode":"explain","dataset":{"inputs":[],"inputs":[]},"queries":[]}"#,
            r#"{"mode":"explain","dataset":{},"queries":[],"pins":[{"rule":"a","value":{"kind":"integer","value":0,"value":1}}]}"#,
            r#"{"mode":"explain","dataset":{},"queries":[],"pins":[{"rule":"a","value":{"kind":"integer","value":0,"unknown":1,"unknown":2}}]}"#,
            r#"{"mode":"explain","dataset":{},"queries":[{"entity_id":"h","outputs":[],"period":{"period_kind":"month","period_kind":"tax_year","start":"2026-01-01","end":"2026-01-31"}}]}"#,
            r#"{"mode":"explain","dataset":{},"queries":[]} {}"#,
        ];
        for raw in duplicates {
            assert_eq!(
                parse_request(raw).unwrap_err().code,
                "invalid_json",
                "{raw}"
            );
        }
    }

    #[test]
    fn validates_every_interval_and_accepts_inclusive_single_day() {
        for path in [
            "/queries/0/period",
            "/dataset/inputs/0/interval",
            "/dataset/relations/0/interval",
        ] {
            let mut value = request_value();
            value.pointer_mut(path).unwrap()["end"] = json!("2025-12-31");
            assert_eq!(
                parse(&value).unwrap_err().code,
                "invalid_interval",
                "{path}"
            );
            value.pointer_mut(path).unwrap()["end"] = json!("2026-01-01");
            assert!(parse(&value).is_ok(), "{path}");
        }
    }

    #[test]
    fn rejects_repeated_rule_pins_and_sorts_unique_rules() {
        let mut value = request_value();
        let existing = value["pins"][0].clone();
        value["pins"].as_array_mut().unwrap().push(existing);
        assert_eq!(parse(&value).unwrap_err().code, "duplicate_pin");
        value["pins"][1]["rule"] = json!("a_first");
        let request = parse(&value).unwrap();
        assert_eq!(
            request
                .pins
                .iter()
                .map(|pin| pin.rule.as_str())
                .collect::<Vec<_>>(),
            ["a_first", "benefit"]
        );
    }

    #[test]
    fn context_binds_scenario_and_queries_but_is_not_dataset_identity() {
        let mut baseline = request_value();
        baseline.as_object_mut().unwrap().remove("pins");
        let request = parse(&baseline).unwrap();
        let context =
            make_context("bundle", "artifact", &engine_identity().unwrap(), &request).unwrap();
        assert!(context.scenario.pins.is_empty());
        assert_eq!(
            context.scenario_sha256,
            digest(&Scenario { pins: vec![] }).unwrap()
        );

        let mut different_dataset = baseline.clone();
        different_dataset["dataset"]["inputs"][0]["value"]["value"] = json!("999");
        let dataset_context = make_context(
            "bundle",
            "artifact",
            &engine_identity().unwrap(),
            &parse(&different_dataset).unwrap(),
        )
        .unwrap();
        assert_eq!(digest(&context).unwrap(), digest(&dataset_context).unwrap());

        let pinned = make_context(
            "bundle",
            "artifact",
            &engine_identity().unwrap(),
            &parse(&request_value()).unwrap(),
        )
        .unwrap();
        assert_ne!(context.scenario_sha256, pinned.scenario_sha256);
        assert_ne!(digest(&context).unwrap(), digest(&pinned).unwrap());
        let mut changed_query = baseline;
        changed_query["queries"][0]["entity_id"] = json!("household:2");
        let query_context = make_context(
            "bundle",
            "artifact",
            &engine_identity().unwrap(),
            &parse(&changed_query).unwrap(),
        )
        .unwrap();
        assert_ne!(digest(&context).unwrap(), digest(&query_context).unwrap());
    }

    #[test]
    fn pin_order_does_not_change_context_digest() {
        let mut value = request_value();
        value["pins"]
            .as_array_mut()
            .unwrap()
            .push(json!({"rule":"a_first","value":{"kind":"integer","value":1}}));
        let first = make_context(
            "bundle",
            "artifact",
            &engine_identity().unwrap(),
            &parse(&value).unwrap(),
        )
        .unwrap();
        value["pins"].as_array_mut().unwrap().reverse();
        let second = make_context(
            "bundle",
            "artifact",
            &engine_identity().unwrap(),
            &parse(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(digest(&first).unwrap(), digest(&second).unwrap());
    }

    #[test]
    fn native_execution_and_receipt_preserve_rounding_trace_and_pins() {
        // Synthetic arithmetic fixture, evaluated only by the real engine.
        let artifact = CompiledProgramArtifact::from_rulespec_str(
            r#"
format: rulespec/v1
rules:
  - name: benefit
    kind: derived
    entity: Household
    dtype: Money
    unit: USD
    rounding: half_up
    effective_from: 2026-01-01
    formula: "123.456"
"#,
        )
        .unwrap();
        let original_bytes = serde_json::to_vec(&artifact).unwrap();
        let base = json!({"mode":"explain","dataset":{},"queries":[{
            "entity_id":"household:1", "outputs":["benefit"],
            "period":{"period_kind":"month","start":"2026-01-01","end":"2026-01-31"}
        }]});
        let request = parse(&base).unwrap();
        let context =
            make_context("bundle", "artifact", &engine_identity().unwrap(), &request).unwrap();
        let result = execute_compiled_request(artifact.clone(), request).unwrap();
        let native_json = serde_json::to_value(&result).unwrap();
        let receipt = ExecutionReceipt {
            format: RECEIPT_FORMAT,
            assurance: "development_unsigned",
            context_sha256: digest(&context).unwrap(),
            context,
            result,
        };
        assert_eq!(
            serde_json::to_value(&receipt).unwrap()["result"],
            native_json
        );
        let trace = &native_json["results"][0]["trace"]["benefit"];
        assert_eq!(trace["entity_id"], "household:1");
        assert_eq!(trace["rounding"], "half_up");
        assert_eq!(trace["pre_rounding_value"]["value"], "123.456");
        assert_eq!(trace["executed_expression"], "123.456");

        let mut pinned = base;
        pinned["pins"] = json!([{"rule":"benefit","value":{"kind":"decimal","value":"0"}}]);
        let result = execute_compiled_request(artifact.clone(), parse(&pinned).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["results"][0]["outputs"]["benefit"]["value"]["value"],
            "0"
        );
        assert_eq!(serde_json::to_vec(&artifact).unwrap(), original_bytes);
    }
}
