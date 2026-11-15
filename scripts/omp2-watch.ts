// `mise run omp2-watch` (0608): is there an omp² release yet? Prints npm's dist-tags for omp and the date and hash of
// the last commit on the `omp2` branch of its repository (github.com/can1357/oh-my-pi, from the package's own
// package.json). Two read-only GETs, no token. A failed call prints one line naming it and exits 1.
export {};

const PACKAGE = "@oh-my-pi/pi-coding-agent";
const REPO = "can1357/oh-my-pi";
const BRANCH = "omp2";

/** GETs `url` as JSON; on any failure prints one line saying which call failed and exits 1. */
async function getJson(call: string, url: string): Promise<unknown> {
	try {
		const res = await fetch(url, { headers: { accept: "application/json", "user-agent": "jocasta-omp2-watch" } });
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
		return await res.json();
	} catch (error) {
		console.error(`omp2-watch: ${call} failed: ${error instanceof Error ? error.message : String(error)}`);
		process.exit(1);
	}
}

const tags = await getJson("npm dist-tags", `https://registry.npmjs.org/-/package/${PACKAGE}/dist-tags`);
const commit = (await getJson(`GitHub ${BRANCH} branch`, `https://api.github.com/repos/${REPO}/commits/${BRANCH}`)) as {
	sha?: string;
	commit?: { committer?: { date?: string } };
};

console.log(`npm dist-tags for ${PACKAGE}: ${JSON.stringify(tags)}`);
console.log(`${REPO} branch ${BRANCH}: last commit ${commit.commit?.committer?.date ?? "?"} ${commit.sha ?? "?"}`);
