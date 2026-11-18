//! What bare `jocasta` and a starting service read from the running one's `/health` (LC-1, LC-3, 0534).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::shared::proctree::pid_alive;
use crate::shared::service_file::ServiceFile;

/// What the service named by `jocasta-service.json` is doing (0254, 0434): `up` answers as that process; `closing`
/// answers but is closing Jocasta; `busy` doesn't answer in time while its process lives (still starting, or hung,
/// 0534); `down` otherwise (no file, nothing listening, another process answering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
	Up,
	Busy,
	Closing,
	Down,
}

/// "on" or "off" as `/health` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnOff {
	On,
	Off,
}

/// `/health`'s answer as bare `jocasta` and the benchmarks read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
	pub state: ServiceState,
	/// The test-only switches as the service has them (0576), while it answers: `JOCASTA_LAYER`, `JOCASTA_HOSTS`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub layer: Option<OnOff>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub hosts: Option<OnOff>,
}

impl ServiceHealth {
	fn of(state: ServiceState) -> Self {
		Self { state, layer: None, hosts: None }
	}
}

fn on_off(value: Option<&Value>) -> Option<OnOff> {
	match value?.as_str()? {
		"on" => Some(OnOff::On),
		"off" => Some(OnOff::Off),
		_ => None,
	}
}

/// How long `/health` and the shutdown request are given by default.
const DEFAULT_TIMEOUT_MS: u64 = 2_000;

/// How long a connect to the loopback port may take: a listening service accepts at once, while Windows takes about
/// two seconds to refuse a port nothing listens on, which must not read as a service that is merely slow to answer.
const CONNECT_MS: u64 = 500;

/// A client for the loopback service: no proxy, whatever the environment says.
fn client(timeout: Duration) -> Option<reqwest::Client> {
	reqwest::Client::builder().no_proxy().connect_timeout(timeout.min(Duration::from_millis(CONNECT_MS))).build().ok()
}

/// Asks the service's `/health` (its token as a bearer, within `timeout_ms`, 2000 by default) what it is doing
/// ([`ServiceState`]).
pub async fn service_health(file: Option<&ServiceFile>, timeout_ms: Option<u64>) -> ServiceHealth {
	let Some(file) = file else { return ServiceHealth::of(ServiceState::Down) };
	let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
	let Some(client) = client(timeout) else { return ServiceHealth::of(ServiceState::Down) };
	let request =
		client.get(format!("http://127.0.0.1:{}/health", file.port)).bearer_auth(&file.token).timeout(timeout);
	let outcome = async {
		let response = request.send().await?;
		if response.status().as_u16() != 200 {
			return Ok(ServiceHealth::of(ServiceState::Down));
		}
		let body: Value = response.json().await?;
		Ok::<_, reqwest::Error>(health_of(&body, file))
	};
	match outcome.await {
		Ok(health) => health,
		// Busy: connected, then no answer in time; a connect that took too long is nothing listening.
		Err(error) => ServiceHealth::of(if error.is_timeout() && !error.is_connect() && pid_alive(file.pid) {
			ServiceState::Busy
		} else {
			ServiceState::Down
		}),
	}
}

/// What a `/health` body says about the service of `file`: down unless it answers as that process.
fn health_of(body: &Value, file: &ServiceFile) -> ServiceHealth {
	if body.get("pid").and_then(Value::as_u64) != Some(u64::from(file.pid)) {
		return ServiceHealth::of(ServiceState::Down);
	}
	ServiceHealth {
		state: if body.get("closing") == Some(&Value::Bool(true)) { ServiceState::Closing } else { ServiceState::Up },
		layer: on_off(body.get("layer")),
		hosts: on_off(body.get("hosts")),
	}
}

/// [`service_health`]'s state only.
pub async fn service_state(file: Option<&ServiceFile>, timeout_ms: Option<u64>) -> ServiceState {
	service_health(file, timeout_ms).await.state
}

/// Where bare `jocasta` tells the service its herdr client closed (0434, 0549): `POST` with the token as a bearer. The
/// service stops everything, or, with threads at work, asks first in a terminal it opens again.
pub const SHUTDOWN_PATH: &str = "/api/shutdown";

/// Tells the service named by `jocasta-service.json` that his client closed; true when it took it.
pub async fn ask_shutdown(file: &ServiceFile) -> bool {
	let Some(client) = client(Duration::from_millis(DEFAULT_TIMEOUT_MS)) else { return false };
	let sent = client
		.post(format!("http://127.0.0.1:{}{SHUTDOWN_PATH}", file.port))
		.bearer_auth(&file.token)
		.timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
		.send()
		.await;
	sent.is_ok_and(|response| response.status().is_success())
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use axum::Router;
	use axum::http::HeaderMap;
	use axum::response::{IntoResponse, Response};
	use axum::routing::{any, post};
	use serde_json::json;

	use super::*;
	use crate::shared::fsx::free_port;

	type Answer = Arc<dyn Fn(&HeaderMap) -> Response + Send + Sync>;

	/// A loopback server answering every request with `answer`, or never when it is `None`; its port.
	async fn service(answer: Option<Answer>) -> u16 {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		let app = Router::new().fallback(any(move |headers: HeaderMap| {
			let answer = answer.clone();
			async move {
				match answer {
					Some(answer) => answer(&headers),
					None => std::future::pending().await,
				}
			}
		}));
		tokio::spawn(async move { axum::serve(listener, app).await });
		port
	}

	fn file(port: u16, pid: u32) -> ServiceFile {
		ServiceFile { port, token: "t".to_owned(), pid }
	}

	fn json_answer(body: Value) -> Option<Answer> {
		Some(Arc::new(move |_| axum::Json(body.clone()).into_response()))
	}

	#[tokio::test]
	async fn a_service_whose_process_lives_but_doesnt_answer_in_time_is_busy_not_gone() {
		let port = service(None).await;
		assert_eq!(service_state(Some(&file(port, std::process::id())), Some(100)).await, ServiceState::Busy);
	}

	#[tokio::test]
	async fn nothing_listening_or_another_process_answering_is_down_and_no_service_file_is_down() {
		let pid = std::process::id();
		assert_eq!(service_state(Some(&file(free_port().unwrap(), pid)), None).await, ServiceState::Down);
		let port = service(json_answer(json!({ "pid": pid + 1 }))).await;
		assert_eq!(service_state(Some(&file(port, pid)), None).await, ServiceState::Down);
		assert_eq!(service_state(None, None).await, ServiceState::Down);
	}

	#[tokio::test]
	async fn the_service_answers_up_with_its_switches_or_closing_while_it_closes_jocasta() {
		let pid = std::process::id();
		let closing = Arc::new(Mutex::new(false));
		let flag = Arc::clone(&closing);
		let port = service(Some(Arc::new(move |_| {
			let mut body = json!({ "pid": pid, "layer": "off", "hosts": "on" });
			if *flag.lock().unwrap() {
				body["closing"] = json!(true);
			}
			axum::Json(body).into_response()
		})))
		.await;
		let auth: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
		let seen = Arc::clone(&auth);
		let other = service(Some(Arc::new(move |headers| {
			let value = headers.get("authorization").and_then(|v| v.to_str().ok()).map(str::to_owned);
			seen.lock().unwrap().push(value);
			axum::Json(json!({ "pid": pid })).into_response()
		})))
		.await;
		assert_eq!(
			service_health(Some(&file(port, pid)), None).await,
			ServiceHealth { state: ServiceState::Up, layer: Some(OnOff::Off), hosts: Some(OnOff::On) }
		);
		*closing.lock().unwrap() = true;
		assert_eq!(service_state(Some(&file(port, pid)), None).await, ServiceState::Closing);
		// The token goes as a bearer, never as a cookie (SEC-NET-4).
		assert_eq!(service_state(Some(&file(other, pid)), None).await, ServiceState::Up);
		assert_eq!(*auth.lock().unwrap(), [Some("Bearer t".to_owned())]);
	}

	#[tokio::test]
	async fn a_non_200_answer_is_down_and_the_health_body_serializes_as_the_typescript_has_it() {
		let pid = std::process::id();
		let port = service(Some(Arc::new(|_| axum::http::StatusCode::UNAUTHORIZED.into_response()))).await;
		assert_eq!(service_state(Some(&file(port, pid)), None).await, ServiceState::Down);
		assert_eq!(
			serde_json::to_value(ServiceHealth::of(ServiceState::Closing)).unwrap(),
			json!({ "state": "closing" })
		);
	}

	#[tokio::test]
	async fn ask_shutdown_posts_with_the_token_as_a_bearer_and_is_true_when_the_service_took_it() {
		let seen: Arc<Mutex<Vec<String>>> = Arc::default();
		let log = Arc::clone(&seen);
		let app = Router::new().route(
			SHUTDOWN_PATH,
			post(move |headers: HeaderMap| {
				let log = Arc::clone(&log);
				async move {
					log.lock().unwrap().push(headers["authorization"].to_str().unwrap().to_owned());
					"ok"
				}
			}),
		);
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		tokio::spawn(async move { axum::serve(listener, app).await });
		assert!(ask_shutdown(&file(port, 1)).await);
		assert_eq!(*seen.lock().unwrap(), ["Bearer t".to_owned()]);
		assert!(!ask_shutdown(&file(free_port().unwrap(), 1)).await);
	}
}
