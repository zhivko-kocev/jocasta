//! The follow-up study (0543, amended by 0545, 0575, 0605, 0608 and 0620), run after the main run: each comparison is
//! one arm paired against its own baseline (A6, or A1 for the prewalk and thinking arms) on the same 10 main tasks × 2
//! reps, each baseline itself run again in the same seeded order so the pairs share the hours and the build. Reported
//! as exploratory (20 trials per comparison show only large differences); each part is kept by the keep rule
//! (`keep.rs`). pitchfork is no comparison (0560), and neither are keep-alive pings (0575): a trial has no idle gap for
//! them to fill.

use crate::evals::tasks::{BenchTask, TaskLanguage, TaskSet, shuffle};
use crate::lib::args::BenchUsageError;
use jocasta::shared::errors::Result;
use jocasta::shared::projects::locale_compare;

pub const FOLLOWUP_TASKS: usize = 10;
pub const FOLLOWUP_REPS: u64 = 2;

pub struct FollowupComparison {
	/// The arm's variant in `evals/variants/`.
	pub arm: &'static str,
	/// The arm it is paired against, run again in the study: `a6`, or `a1` for the prewalk and thinking arms (0608, 0620).
	pub baseline: &'static str,
	/// What differs from the baseline.
	pub change: &'static str,
	pub records: &'static str,
}

pub const FOLLOWUP_COMPARISONS: [FollowupComparison; 6] = [
	FollowupComparison {
		arm: "f-keep-context",
		baseline: "a6",
		change: "omp's \"Approve and keep context\" in place of \"Approve and execute\"",
		records: "0545",
	},
	FollowupComparison {
		arm: "f-keep-smol",
		baseline: "a6",
		change: "omp's \"Approve and keep context\", the work that follows done on the `smol` role",
		records: "0608",
	},
	FollowupComparison {
		arm: "f-ponytail-if",
		baseline: "a6",
		change: "Ponytail with its \"Running tests\" rules inside `<important if=\"you are running or writing tests\">`",
		records: "0545",
	},
	FollowupComparison {
		arm: "f-prewalk-clean",
		baseline: "a1",
		change: "prewalk on (`omp --prewalk`) on A1",
		records: "0608",
	},
	FollowupComparison {
		arm: "f-advisor",
		baseline: "a6",
		change: "omp's advisor on, on the `advisor` role's model and level, every third update",
		records: "0620",
	},
	FollowupComparison {
		arm: "f-think-auto",
		baseline: "a1",
		change: "`--thinking auto` in place of `high` on A1",
		records: "0620",
	},
];

/// The study's baselines, in the order they run first: A6 and A1.
pub const FOLLOWUP_BASELINES: [&str; 2] = ["a6", "a1"];

/// The study's arms: the baselines, then each comparison's arm.
pub const FOLLOWUP_ARMS: [&str; 8] =
	["a6", "a1", "f-keep-context", "f-keep-smol", "f-ponytail-if", "f-prewalk-clean", "f-advisor", "f-think-auto"];

/// The language strata the follow-up's tasks keep in proportion to the main set.
const STRATA: [&[TaskLanguage]; 3] =
	[&[TaskLanguage::Python], &[TaskLanguage::TypeScript, TaskLanguage::JavaScript], &[TaskLanguage::Go]];

/// The study's tasks: [`FOLLOWUP_TASKS`] of the main set, each language stratum's share by largest remainder (ties to
/// the earlier stratum), each stratum's tasks (in id order) shuffled with `seed`.
pub fn pick_followup_tasks(tasks: &[BenchTask], seed: u32) -> Result<Vec<BenchTask>> {
	let count = FOLLOWUP_TASKS;
	let main: Vec<&BenchTask> = tasks.iter().filter(|task| task.set == TaskSet::Main).collect();
	if main.len() < count {
		return Err(BenchUsageError::err(format!(
			"the follow-up study needs {count} main tasks; the selection has {}",
			main.len()
		)));
	}
	let strata: Vec<Vec<BenchTask>> = STRATA
		.iter()
		.map(|languages| {
			let mut stratum: Vec<BenchTask> =
				main.iter().filter(|task| languages.contains(&task.language)).map(|task| (*task).clone()).collect();
			stratum.sort_by(|a, b| locale_compare(&a.instance_id, &b.instance_id));
			stratum
		})
		.collect();
	let exact: Vec<f64> = strata.iter().map(|stratum| (stratum.len() * count) as f64 / main.len() as f64).collect();
	let mut shares: Vec<usize> = exact.iter().map(|value| value.floor() as usize).collect();
	let mut order: Vec<(usize, f64)> = exact.iter().enumerate().map(|(i, value)| (i, value - value.floor())).collect();
	order.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
	let mut k = 0;
	while shares.iter().sum::<usize>() < count {
		shares[order[k % order.len()].0] += 1;
		k += 1;
	}
	Ok(strata.iter().enumerate().flat_map(|(i, stratum)| shuffle(stratum, seed).into_iter().take(shares[i])).collect())
}
