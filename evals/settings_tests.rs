use serde_json::{Value, json};

use super::settings::{Class, Pins, arm_name, arm_values, classify, overlay_of, parse_arm, set_keys, valid_values};
use super::variants::default_variant;

fn pins() -> Pins {
	Pins {
		pinned: set_keys("bash:\n  allow: []\nauth:\n  broker: {}\nretry:\n  fallbackChains: {}\n"),
		per_pane: set_keys("compaction:\n  thresholdTokens: 300000\nadvisor:\n  enabled: true\n"),
	}
}

#[test]
fn a_key_an_overlay_sets_is_judged_per_pane_before_any_other_rule() {
	let pins = pins();
	assert_eq!(classify("compaction.thresholdTokens", "number", &pins), Class::J);
	assert_eq!(classify("advisor.enabled", "boolean", &pins), Class::J);
	assert_eq!(classify("plan.enabled", "boolean", &pins), Class::J);
	assert_eq!(classify("bash.allow", "array", &pins), Class::P);
	assert_eq!(classify("auth.broker.url", "string", &pins), Class::P);
	assert_eq!(classify("providers.cacheWarming", "enum", &pins), Class::P);
	assert_eq!(classify("advisor.reviewInterval", "number", &pins), Class::Q);
	assert_eq!(classify("defaultThinkingLevel", "enum", &pins), Class::Q);
	assert_eq!(classify("lsp.serverTimeout", "string", &pins), Class::F);
	assert_eq!(classify("compaction.reserveTokens", "number", &pins), Class::C);
	// A key that only starts like a set one is not set by it.
	assert_eq!(classify("compaction.thresholdTokensExtra", "number", &pins), Class::C);
}

#[test]
fn each_arm_tries_every_value_but_the_default() {
	assert_eq!(arm_values("boolean", Some(&json!(true)), &[]), vec![json!(false)]);
	let enum_values = ["unicode".to_owned(), "nerd".to_owned(), "ascii".to_owned()];
	assert_eq!(arm_values("enum", Some(&json!("nerd")), &enum_values), vec![json!("unicode"), json!("ascii")]);
	assert_eq!(arm_values("number", Some(&json!(5)), &[]), vec![json!(3), json!(10)]);
	assert_eq!(arm_values("number", Some(&json!(0.4)), &[]), vec![json!(0.2), json!(0.8)]);
	assert_eq!(arm_values("number", Some(&json!(0)), &[]), Vec::<Value>::new());
	assert_eq!(arm_values("number", None, &[]), Vec::<Value>::new());
	// 1 halves to 1 when rounded: only the doubled value is an arm.
	assert_eq!(arm_values("number", Some(&json!(1)), &[]), vec![json!(2)]);
}

#[test]
fn an_enums_values_are_read_from_omps_refusal() {
	let said = "Error: Invalid value: __probe__. Valid values: unicode, nerd, ascii\n";
	assert_eq!(valid_values(said), vec!["unicode", "nerd", "ascii"]);
	assert!(valid_values("Error: Unknown setting").is_empty());
}

#[test]
fn an_arm_of_several_settings_names_and_parses_back_and_layers_without_dropping_siblings() {
	let arm = vec![("compaction.reserveTokens".to_owned(), json!(8000)), ("tui.style".to_owned(), json!("a=b"))];
	let name = arm_name(&arm);
	assert_eq!(parse_arm(&name).unwrap(), arm);
	assert_eq!(arm_name(&[]), "base");
	assert!(parse_arm("base").unwrap().is_empty());
	let mut variant = default_variant();
	variant.settings = Some(json!({ "compaction": { "thresholdTokens": 300000 } }).as_object().unwrap().clone());
	let overlay = Value::Object(overlay_of(&variant, &arm));
	assert_eq!(overlay["compaction"], json!({ "thresholdTokens": 300000, "reserveTokens": 8000 }));
	assert_eq!(overlay["memory"]["backend"], "off");
	assert_eq!(overlay["tui"]["style"], "a=b");
}
