//! `omp_yaml` against `Bun.YAML.stringify(value, null, 2)` (with `omp-settings.ts`'s `: +$` clean-up) over a broad corpus:
//! reserved words, number-like strings, leading indicators, `: ` and ` #` inside, quotes, backslashes, control
//! characters, empty strings, unicode, multi-line strings, nested maps and lists, empty containers, null and booleans,
//! and keys that need quotes. `omp_yaml_corpus.json` holds Bun 1.4.3's own output, one case per entry
//! (`{ name, json, yaml }`), made by a throwaway Bun script.

use serde_json::Value;

use super::*;

#[test]
fn omp_yaml_is_bun_yaml_stringify_over_the_whole_corpus() {
	let cases: Vec<Value> = serde_json::from_str(include_str!("omp_yaml_corpus.json")).expect("the corpus");
	assert!(cases.len() > 2000);
	let mut wrong = Vec::new();
	for case in &cases {
		let (name, json, yaml) =
			(case["name"].as_str().unwrap(), case["json"].as_str().unwrap(), case["yaml"].as_str().unwrap());
		let value: Value = serde_json::from_str(json).expect("the case's JSON");
		let got = omp_yaml(&value);
		// Bun writes a byte order mark past the first character plain; ours quotes it so the text reads back.
		let inner_bom = value.to_string().chars().skip(1).any(|c| c == '\u{feff}') && !yaml.contains("\"\u{feff}");
		if got != yaml && !inner_bom {
			wrong.push(format!("{name}: {json}\n  bun:  {yaml:?}\n  rust: {got:?}"));
		}
	}
	assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}

#[test]
fn every_string_in_the_corpus_reads_back_as_the_same_string_through_our_own_yaml_parser() {
	let cases: Vec<Value> = serde_json::from_str(include_str!("omp_yaml_corpus.json")).expect("the corpus");
	let mut wrong = Vec::new();
	for case in cases.iter().filter(|case| case["name"].as_str().is_some_and(|name| name.starts_with("value "))) {
		let value: Value = serde_json::from_str(case["json"].as_str().unwrap()).unwrap();
		let back = crate::shared::yaml::parse_value(&omp_yaml(&value));
		if back.as_ref() != Ok(&value) {
			wrong.push(format!("{value} -> {back:?}"));
		}
	}
	assert!(wrong.is_empty(), "{} do not round-trip:\n{}", wrong.len(), wrong.join("\n"));
}
