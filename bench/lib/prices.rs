//! The benchmark's one price table (0538): LiteLLM's `model_prices_and_context_window.json`, fetched once and committed
//! under `bench/prices/` with its source (`source.json`: URL, commit, date, SHA-256). Every arm's tokens are priced with
//! it, so omp, Claude Code and opencode compare on one bill; omp's own `usage.cost` stays the product's figure. Cache
//! read and cache write are priced apart, a 1-hour cache write at its own rate, and models with a 200k tier at their
//! higher rates past 200k prompt tokens.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::lib::args::REPO_ROOT;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::ledger::RequestFields;

fn prices_dir() -> PathBuf {
	REPO_ROOT.join("bench").join("prices")
}

/// Where the committed table came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSource {
	pub url: String,
	pub commit: String,
	pub commit_date: String,
	pub fetched_at: String,
	pub sha256: String,
}

/// Rates past [`TIER_TOKENS`] prompt tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
	pub input: f64,
	pub output: f64,
	pub cache_read: f64,
	pub cache_write: f64,
}

/// One model's prices in dollars per token, and its context window in tokens.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
	/// The table's key it was found under.
	pub key: String,
	pub input: f64,
	pub output: f64,
	pub cache_read: f64,
	pub cache_write: f64,
	/// A 1-hour cache write; the 5-minute rate when the table has none.
	pub cache_write_1h: f64,
	/// Rates past [`TIER_TOKENS`] prompt tokens, when the table has them.
	pub above_200k: Option<Tier>,
	pub context_window: Option<u64>,
}

/// One request's tokens as the ledger has them; `cache_write_1h` is the part of `cache_write` written for an hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestTokens {
	pub input: u64,
	pub cache_read: u64,
	pub cache_write: u64,
	pub cache_write_1h: u64,
	pub output: u64,
}

impl From<&RequestFields> for RequestTokens {
	fn from(fields: &RequestFields) -> Self {
		Self {
			input: fields.input,
			cache_read: fields.cache_read,
			cache_write: fields.cache_write,
			cache_write_1h: fields.cache_write_1h,
			output: fields.output,
		}
	}
}

const TIER_TOKENS: u64 = 200_000;

pub struct PriceTable {
	json: Map<String, Value>,
	cache: Mutex<HashMap<String, Option<ModelPrice>>>,
}

fn num(entry: &Map<String, Value>, key: &str) -> Option<f64> {
	entry.get(key).and_then(Value::as_f64).filter(|value| value.is_finite())
}

fn to_price(key: &str, entry: &Map<String, Value>) -> Option<ModelPrice> {
	let input = num(entry, "input_cost_per_token")?;
	let output = num(entry, "output_cost_per_token")?;
	let cache_read = num(entry, "cache_read_input_token_cost").unwrap_or(input);
	let cache_write = num(entry, "cache_creation_input_token_cost").unwrap_or(input);
	let tier = num(entry, "input_cost_per_token_above_200k_tokens").map(|tier_input| Tier {
		input: tier_input,
		output: num(entry, "output_cost_per_token_above_200k_tokens").unwrap_or(output),
		cache_read: num(entry, "cache_read_input_token_cost_above_200k_tokens").unwrap_or(cache_read),
		cache_write: num(entry, "cache_creation_input_token_cost_above_200k_tokens").unwrap_or(cache_write),
	});
	Some(ModelPrice {
		key: key.to_owned(),
		input,
		output,
		cache_read,
		cache_write,
		cache_write_1h: num(entry, "cache_creation_input_token_cost_above_1hr").unwrap_or(cache_write),
		above_200k: tier,
		context_window: num(entry, "max_input_tokens").map(|window| window as u64),
	})
}

static BRACKETED_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]$").expect("a fixed pattern"));

/// The model names tried, in order: `<provider>/<model>`, the model as given, and its last path segment, each without a
/// bracketed suffix such as Claude Code's `[1m]`. No fuzzy matching: an unknown model stays unpriced and is reported.
pub fn candidate_keys(provider: Option<&str>, model: &str) -> Vec<String> {
	let bare = BRACKETED_SUFFIX.replace(model, "").trim().to_owned();
	let last = if bare.contains('/') { bare.rsplit('/').next().unwrap_or(&bare).to_owned() } else { bare.clone() };
	let mut keys: Vec<String> = Vec::new();
	let wanted = provider.map(|provider| format!("{provider}/{last}")).into_iter().chain([bare, last]);
	for key in wanted {
		if !keys.contains(&key) {
			keys.push(key);
		}
	}
	keys
}

/// A table from parsed JSON (tests build small ones).
pub fn price_table_from(json: Map<String, Value>) -> PriceTable {
	PriceTable { json, cache: Mutex::new(HashMap::new()) }
}

/// Where the committed table came from (`bench/prices/source.json`).
pub fn pinned_source() -> Result<PriceSource> {
	Ok(serde_json::from_str(&fs::read_to_string(prices_dir().join("source.json"))?)?)
}

impl PriceTable {
	/// The model's prices; `None` when the table doesn't have it.
	pub fn lookup(&self, provider: Option<&str>, model: &str) -> Option<ModelPrice> {
		let id = format!("{}\u{0}{model}", provider.unwrap_or(""));
		let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
		if let Some(found) = cache.get(&id) {
			return found.clone();
		}
		let found = candidate_keys(provider, model).iter().find_map(|key| {
			let entry = self.json.get(key)?.as_object()?;
			to_price(key, entry)
		});
		cache.insert(id, found.clone());
		found
	}
}

static LOADED: OnceLock<PriceTable> = OnceLock::new();

/// The committed table; fails when its SHA-256 isn't the one `source.json` pins. Read once per process.
pub fn load_price_table() -> Result<&'static PriceTable> {
	if let Some(table) = LOADED.get() {
		return Ok(table);
	}
	let dir = prices_dir();
	let source = pinned_source()?;
	let file = dir.join("model_prices_and_context_window.json");
	let bytes = fs::read(&file)?;
	let sha256: String = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
	if sha256 != source.sha256 {
		return Err(Error::other(format!(
			"{} has SHA-256 {sha256}, not the pinned {} (source.json)",
			file.display(),
			source.sha256
		)));
	}
	let Value::Object(json) = serde_json::from_slice(&bytes)? else {
		return Err(Error::other(format!("{} is not a JSON object", file.display())));
	};
	Ok(LOADED.get_or_init(|| price_table_from(json)))
}

/// One request's dollars at `price`; the 200k tier applies when the prompt (input + cache read + cache write) passes it.
pub fn price_request(price: &ModelPrice, tokens: &RequestTokens) -> f64 {
	let prompt = tokens.input + tokens.cache_read + tokens.cache_write;
	let tier = price.above_200k.filter(|_| prompt > TIER_TOKENS);
	let five_minute = tokens.cache_write.saturating_sub(tokens.cache_write_1h);
	tokens.input as f64 * tier.map_or(price.input, |t| t.input)
		+ tokens.output as f64 * tier.map_or(price.output, |t| t.output)
		+ tokens.cache_read as f64 * tier.map_or(price.cache_read, |t| t.cache_read)
		+ five_minute as f64 * tier.map_or(price.cache_write, |t| t.cache_write)
		+ tokens.cache_write_1h as f64 * price.cache_write_1h
}
