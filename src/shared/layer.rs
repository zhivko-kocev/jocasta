//! The harness-OS layer's test-only switches (0576).

use std::collections::HashMap;

use crate::shared::env;

/// The harness-OS layer's test-only switch (0576): `JOCASTA_LAYER=off` leaves the layer out of thread panes (the
/// `check` and `report_result` tools, the task file's prompt parts, the code index), for K11's same-build comparison.
/// Like `JOCASTA_HOSTS` below it is for benchmarks and tests; anything else leaves the layer on.
pub const LAYER_ENV: &str = "JOCASTA_LAYER";

/// H2's `tick` condition (0576): `JOCASTA_HOSTS=off` makes a sandbox service ignore the states omp pushes over the
/// bridge (0675), so a thread's state comes from the surface's events and the refresh tick only. For benchmarks and
/// tests, like [`LAYER_ENV`].
pub const HOSTS_ENV: &str = "JOCASTA_HOSTS";

fn is_on(name: &str, environment: Option<&HashMap<String, String>>) -> bool {
	let value = match environment {
		Some(map) => map.get(name).cloned(),
		None => env::var(name),
	};
	value.as_deref() != Some("off")
}

/// Whether the harness-OS layer is on in `environment` (the process's when `None`) (0576): only `off` turns it off.
pub fn layer_on(environment: Option<&HashMap<String, String>>) -> bool {
	is_on(LAYER_ENV, environment)
}

/// Whether the service takes omp's pushed states in `environment` (the process's when `None`) (0576, 0675): only `off`
/// turns it off.
pub fn hosts_on(environment: Option<&HashMap<String, String>>) -> bool {
	is_on(HOSTS_ENV, environment)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn only_off_turns_the_layer_and_the_hosts_off() {
		let mut given = HashMap::new();
		assert!(layer_on(Some(&given)) && hosts_on(Some(&given)));
		given.insert(LAYER_ENV.to_owned(), "off".to_owned());
		given.insert(HOSTS_ENV.to_owned(), "on".to_owned());
		assert!(!layer_on(Some(&given)) && hosts_on(Some(&given)));
		let mut temp = temp_jocasta();
		temp.set_var(LAYER_ENV, Some("off"));
		assert!(!layer_on(None));
		temp.set_var(LAYER_ENV, None);
		assert!(layer_on(None));
	}
}
