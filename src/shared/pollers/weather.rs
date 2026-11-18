//! The weather on the dashboard (0429) from Open-Meteo: keyless JSON, free for non-commercial use, CC-BY 4.0. The place
//! is `dashboard.weather` in `jocasta.yaml`: a name ("Example City", "Example City, Example Country") resolved once
//! through Open-Meteo's geocoding, or `latitude, longitude`. The Rust port of `weather.ts`.

use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use crate::frontier::protocol::Weather;
use crate::shared::errors::{Error, Result};
use crate::shared::settings::{Settings, SettingsError, check_settings};

pub const GEOCODING_ENDPOINT: &str = "https://geocoding-api.open-meteo.com/v1/search";
pub const FORECAST_ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";

/// A resolved place.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
	pub name: String,
	pub latitude: f64,
	pub longitude: f64,
}

/// WMO weather interpretation codes as Open-Meteo documents them, in sentence case.
fn weather_summary(code: f64) -> &'static str {
	if code.fract() != 0.0 {
		return "Unknown";
	}
	match code as i64 {
		0 => "Clear sky",
		1 => "Mainly clear",
		2 => "Partly cloudy",
		3 => "Overcast",
		45 => "Fog",
		48 => "Rime fog",
		51 => "Light drizzle",
		53 => "Drizzle",
		55 => "Dense drizzle",
		56 => "Light freezing drizzle",
		57 => "Freezing drizzle",
		61 => "Light rain",
		63 => "Rain",
		65 => "Heavy rain",
		66 => "Light freezing rain",
		67 => "Freezing rain",
		71 => "Light snow",
		73 => "Snow",
		75 => "Heavy snow",
		77 => "Snow grains",
		80 => "Light showers",
		81 => "Showers",
		82 => "Violent showers",
		85 => "Light snow showers",
		86 => "Snow showers",
		95 => "Thunderstorm",
		96 => "Thunderstorm with light hail",
		99 => "Thunderstorm with hail",
		_ => "Unknown",
	}
}

/// `dashboard.weather` (0429): the place, or `None` when unset. A bad value fails, naming the file, line and key (0584),
/// and turns the weather off.
pub fn read_weather_place(settings: &Settings) -> Result<Option<String>, SettingsError> {
	check_settings(settings, "dashboard.weather")?;
	let place = settings.get("dashboard").and_then(|dashboard| dashboard.get("weather")).and_then(Value::as_str);
	Ok(place.map(str::trim).filter(|place| !place.is_empty()).map(str::to_owned))
}

/// `encodeURIComponent`: everything but letters, digits and `-_.!~*'()` is percent-encoded.
fn encode_component(text: &str) -> String {
	let mut out = String::new();
	for byte in text.bytes() {
		if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
			out.push(byte as char);
		} else {
			out.push_str(&format!("%{byte:02X}"));
		}
	}
	out
}

/// `URLSearchParams`' encoding: like [`encode_component`] but a space is `+`, and only `*-._` stay.
fn encode_form(text: &str) -> String {
	let mut out = String::new();
	for byte in text.bytes() {
		match byte {
			b' ' => out.push('+'),
			byte if byte.is_ascii_alphanumeric() || b"*-._".contains(&byte) => out.push(byte as char),
			byte => out.push_str(&format!("%{byte:02X}")),
		}
	}
	out
}

/// Fetches one Open-Meteo answer; every failure is one plain line naming the endpoint.
async fn get_json(url: &str, what: &str) -> Result<Value> {
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(30))
		.build()
		.map_err(|_| Error::other(format!("Weather: Open-Meteo's {what} couldn't be reached")))?;
	let response = client
		.get(url)
		.send()
		.await
		.map_err(|_| Error::other(format!("Weather: Open-Meteo's {what} couldn't be reached")))?;
	if !response.status().is_success() {
		return Err(Error::other(format!("Weather: Open-Meteo's {what} answered HTTP {}", response.status().as_u16())));
	}
	let not_json = || Error::other(format!("Weather: Open-Meteo's {what} answer isn't JSON"));
	let text = response.text().await.map_err(|_| not_json())?;
	serde_json::from_str(&text).map_err(|_| not_json())
}

fn unknown_shape(what: &str) -> Error {
	Error::other(format!("Weather: Open-Meteo's {what} answer has an unknown shape"))
}

/// A string member that is absent or a string (TypeBox's `Type.Optional(Type.String())`); `None` on a wrong type.
fn optional_string(object: &serde_json::Map<String, Value>, key: &str) -> Option<Option<String>> {
	match object.get(key) {
		None => Some(None),
		Some(Value::String(text)) => Some(Some(text.clone())),
		Some(_) => None,
	}
}

/// One geocoding match, checked.
struct Match {
	name: String,
	latitude: f64,
	longitude: f64,
	country: Option<String>,
	admin1: Option<String>,
}

fn geocoding_match(value: &Value) -> Option<Match> {
	let object = value.as_object()?;
	Some(Match {
		name: object.get("name")?.as_str()?.to_owned(),
		latitude: object.get("latitude")?.as_f64()?,
		longitude: object.get("longitude")?.as_f64()?,
		country: optional_string(object, "country")?,
		admin1: optional_string(object, "admin1")?,
	})
}

/// The geocoding answer's matches, shape-checked: `results` is optional.
fn geocoding_results(body: &Value) -> Option<Vec<Match>> {
	match body.as_object()?.get("results") {
		None => Some(Vec::new()),
		Some(Value::Array(results)) => results.iter().map(geocoding_match).collect(),
		Some(_) => None,
	}
}

static COORDINATES: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^(-?[0-9]{1,2}(?:\.[0-9]+)?)\s*,\s*(-?[0-9]{1,3}(?:\.[0-9]+)?)$").expect("the coordinates pattern")
});

/// Resolves `dashboard.weather`: coordinates as given, else the first geocoding match of the name before the first
/// comma whose country or region matches what follows it ("Example City, Example Country").
pub async fn find_place(place: &str) -> Result<Place> {
	find_place_at(place, GEOCODING_ENDPOINT).await
}

/// [`find_place`] asking `endpoint` instead of Open-Meteo's geocoding (the tests' in-process server).
pub async fn find_place_at(place: &str, endpoint: &str) -> Result<Place> {
	if let Some(found) = COORDINATES.captures(place) {
		let latitude: f64 = found[1].parse().unwrap_or(f64::NAN);
		let longitude: f64 = found[2].parse().unwrap_or(f64::NAN);
		if latitude.abs() <= 90.0 && longitude.abs() <= 180.0 {
			return Ok(Place { name: place.to_owned(), latitude, longitude });
		}
	}
	let mut parts = place.split(',').map(str::trim);
	let name = parts.next().unwrap_or("");
	let qualifier = parts.collect::<Vec<_>>().join(", ").to_lowercase();
	let body =
		get_json(&format!("{endpoint}?name={}&count=10&language=en&format=json", encode_component(name)), "geocoding")
			.await?;
	let results = geocoding_results(&body).ok_or_else(|| unknown_shape("geocoding"))?;
	let matches = |found: &Match| {
		[&found.country, &found.admin1]
			.iter()
			.any(|part| part.as_ref().is_some_and(|part| part.to_lowercase() == qualifier))
	};
	let found = if qualifier.is_empty() { results.first() } else { results.iter().find(|found| matches(found)) };
	let Some(found) = found else {
		return Err(Error::other(format!(
			"Weather: Open-Meteo knows no place \"{place}\"; set dashboard.weather to another name"
		)));
	};
	let name = [Some(found.name.as_str()), found.country.as_deref()]
		.into_iter()
		.flatten()
		.filter(|part| !part.is_empty())
		.collect::<Vec<_>>()
		.join(", ");
	Ok(Place { name, latitude: found.latitude, longitude: found.longitude })
}

/// The forecast answer's fields, shape-checked: the UTC offset, the current time, temperature and code, today's high
/// and low.
struct Forecast {
	utc_offset_seconds: f64,
	time: String,
	temperature: f64,
	code: f64,
	high: f64,
	low: f64,
}

fn first_number(daily: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
	daily.get(key)?.as_array()?.iter().map(Value::as_f64).collect::<Option<Vec<_>>>()?.first().copied()
}

fn forecast_of(body: &Value) -> Option<Forecast> {
	let object = body.as_object()?;
	let current = object.get("current")?.as_object()?;
	let daily = object.get("daily")?.as_object()?;
	Some(Forecast {
		utc_offset_seconds: object.get("utc_offset_seconds")?.as_f64()?,
		time: current.get("time")?.as_str()?.to_owned(),
		temperature: current.get("temperature_2m")?.as_f64()?,
		code: current.get("weather_code")?.as_f64()?,
		high: first_number(daily, "temperature_2m_max")?,
		low: first_number(daily, "temperature_2m_min")?,
	})
}

/// `current.time` (the place's local time without an offset, read as UTC) in epoch ms; `None` when it isn't a time.
fn parse_local(time: &str) -> Option<i64> {
	["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S%.f"]
		.iter()
		.find_map(|format| chrono::NaiveDateTime::parse_from_str(time, format).ok())
		.map(|local| local.and_utc().timestamp_millis())
}

/// The weather now and today's high and low at a place (0429).
pub async fn poll_weather(place: &Place) -> Result<Weather> {
	poll_weather_at(place, FORECAST_ENDPOINT).await
}

/// [`poll_weather`] asking `endpoint` instead of Open-Meteo's forecast (the tests' in-process server).
pub async fn poll_weather_at(place: &Place, endpoint: &str) -> Result<Weather> {
	let query = [
		("latitude", place.latitude.to_string()),
		("longitude", place.longitude.to_string()),
		("current", "temperature_2m,weather_code".to_owned()),
		("daily", "temperature_2m_max,temperature_2m_min".to_owned()),
		("timezone", "auto".to_owned()),
		("forecast_days", "1".to_owned()),
	]
	.iter()
	.map(|(key, value)| format!("{key}={}", encode_form(value)))
	.collect::<Vec<_>>()
	.join("&");
	let body = get_json(&format!("{endpoint}?{query}"), "forecast").await?;
	let forecast = forecast_of(&body).ok_or_else(|| unknown_shape("forecast"))?;
	// `current.time` is the place's local time without an offset; `utc_offset_seconds` makes it a moment.
	let at = parse_local(&forecast.time)
		.and_then(|local| {
			chrono::DateTime::from_timestamp_millis(local - (forecast.utc_offset_seconds * 1000.0) as i64)
		})
		.map(|moment| moment.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
		.unwrap_or_default();
	Ok(Weather {
		place: place.name.clone(),
		temp_c: forecast.temperature,
		code: forecast.code as i64,
		summary: weather_summary(forecast.code).to_owned(),
		high_c: forecast.high,
		low_c: forecast.low,
		at,
	})
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use serde_json::json;
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	use super::*;

	/// An in-process HTTP server answering from a table of path prefixes `(prefix, status, body)` and recording each
	/// request target; nothing reaches the network. Its address is the endpoint base.
	struct Fake {
		base: String,
		asked: Arc<Mutex<Vec<String>>>,
	}

	impl Fake {
		async fn serve(answers: Vec<(&'static str, u16, String)>) -> Self {
			let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listening");
			let base = format!("http://{}", listener.local_addr().expect("its address"));
			let asked = Arc::new(Mutex::new(Vec::new()));
			let seen = Arc::clone(&asked);
			tokio::spawn(async move {
				while let Ok((mut stream, _)) = listener.accept().await {
					let mut head = Vec::new();
					let mut chunk = [0u8; 1024];
					while !head.windows(4).any(|window| window == b"\r\n\r\n") {
						match stream.read(&mut chunk).await {
							Ok(0) | Err(_) => break,
							Ok(n) => head.extend_from_slice(&chunk[..n]),
						}
					}
					let text = String::from_utf8_lossy(&head).into_owned();
					let target = text.split_whitespace().nth(1).unwrap_or("").to_owned();
					seen.lock().unwrap().push(target.clone());
					let (status, body) = answers
						.iter()
						.find(|(prefix, ..)| target.starts_with(prefix))
						.map_or((404, String::new()), |(_, status, body)| (*status, body.clone()));
					let reply = format!(
						"HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
						body.len()
					);
					let _ = stream.write_all(reply.as_bytes()).await;
					let _ = stream.shutdown().await;
				}
			});
			Self { base, asked }
		}

		fn endpoint(&self, path: &str) -> String {
			format!("{}{path}", self.base)
		}

		fn last(&self) -> String {
			self.asked.lock().unwrap().last().cloned().unwrap_or_default()
		}
	}

	/// Open-Meteo's answers for a sample city (shaped like live answers, 2026-10-14; placeholder names).
	fn geocoding() -> String {
		json!({ "results": [
			{ "id": 1, "name": "Example City", "latitude": 12.34567, "longitude": 56.78901,
				"country": "Example Country", "admin1": "Example Region" },
			{ "id": 2, "name": "Example City", "latitude": -21.3, "longitude": 47.1, "country": "Other Country" },
		] })
		.to_string()
	}

	fn forecast() -> String {
		json!({
			"latitude": 12.35, "longitude": 56.79, "utc_offset_seconds": 10800, "timezone": "Etc/GMT-3",
			"current": { "time": "2026-10-14T16:15", "interval": 900, "temperature_2m": 11.9, "weather_code": 2 },
			"daily": { "time": ["2026-10-14"], "temperature_2m_max": [11.9], "temperature_2m_min": [6.7] },
		})
		.to_string()
	}

	fn settings_of(value: Value) -> Settings {
		match value {
			Value::Object(map) => Settings::from_map(map),
			other => panic!("not a mapping: {other}"),
		}
	}

	#[tokio::test]
	async fn weather_resolves_dashboard_weather_through_geocoding_then_the_forecast_hidden_until_set() {
		assert_eq!(read_weather_place(&settings_of(json!({}))).unwrap(), None);
		let set = settings_of(json!({ "dashboard": { "weather": " Example City, Example Country " } }));
		assert_eq!(read_weather_place(&set).unwrap().as_deref(), Some("Example City, Example Country"));
		let bad = settings_of(json!({ "dashboard": { "weather": 42 } }));
		assert!(read_weather_place(&bad).unwrap_err().message.contains("dashboard.weather"));
		// A blank place isn't one.
		assert!(read_weather_place(&settings_of(json!({ "dashboard": { "weather": "  " } }))).is_err());

		let fake = Fake::serve(vec![("/geo", 200, geocoding()), ("/forecast", 200, forecast())]).await;
		let geo = fake.endpoint("/geo");
		let place = find_place_at("Example City, Other Country", &geo).await.unwrap();
		assert_eq!(place, Place { name: "Example City, Other Country".to_owned(), latitude: -21.3, longitude: 47.1 });
		let city = find_place_at("Example City", &geo).await.unwrap();
		assert_eq!(
			city,
			Place { name: "Example City, Example Country".to_owned(), latitude: 12.34567, longitude: 56.78901 }
		);
		assert_eq!(fake.last(), "/geo?name=Example%20City&count=10&language=en&format=json");
		// Coordinates need no lookup.
		let nowhere = Fake::serve(vec![]).await;
		let coordinates = find_place_at("12.34, 56.78", &nowhere.endpoint("/geo")).await.unwrap();
		assert_eq!(coordinates, Place { name: "12.34, 56.78".to_owned(), latitude: 12.34, longitude: 56.78 });
		assert_eq!(nowhere.last(), "");
		let unknown = find_place_at("Example City, Nowhere", &geo).await.unwrap_err().to_string();
		assert!(unknown.contains("Open-Meteo knows no place \"Example City, Nowhere\""), "{unknown}");

		let weather = poll_weather_at(&city, &fake.endpoint("/forecast")).await.unwrap();
		assert_eq!(
			weather,
			Weather {
				place: "Example City, Example Country".to_owned(),
				temp_c: 11.9,
				code: 2,
				summary: "Partly cloudy".to_owned(),
				high_c: 11.9,
				low_c: 6.7,
				at: "2026-10-14T13:15:00.000Z".to_owned(),
			}
		);
		assert!(
			fake.last().contains("latitude=12.34567&longitude=56.78901&current=temperature_2m%2Cweather_code"),
			"{}",
			fake.last()
		);
		let odd = Fake::serve(vec![("/forecast", 200, json!({ "error": true, "reason": "x" }).to_string())]).await;
		let said = poll_weather_at(&city, &odd.endpoint("/forecast")).await.unwrap_err().to_string();
		assert!(said.contains("Weather: Open-Meteo's forecast answer has an unknown shape"), "{said}");
		let down = Fake::serve(vec![("/forecast", 500, String::new())]).await;
		let said = poll_weather_at(&city, &down.endpoint("/forecast")).await.unwrap_err().to_string();
		assert!(said.contains("Weather: Open-Meteo's forecast answered HTTP 500"), "{said}");
	}

	#[tokio::test]
	async fn weather_failures_name_the_endpoint_never_the_payload() {
		let city = Place { name: "Example City".to_owned(), latitude: 1.0, longitude: 2.0 };
		let text = Fake::serve(vec![("/forecast", 200, "<html>".to_owned())]).await;
		let said = poll_weather_at(&city, &text.endpoint("/forecast")).await.unwrap_err().to_string();
		assert!(said.contains("Weather: Open-Meteo's forecast answer isn't JSON"), "{said}");
		// Nothing listens: the address is free again once its listener is dropped.
		let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let address = gone.local_addr().unwrap();
		drop(gone);
		let said = poll_weather_at(&city, &format!("http://{address}/forecast")).await.unwrap_err().to_string();
		assert!(said.contains("Weather: Open-Meteo's forecast couldn't be reached"), "{said}");
	}
}
