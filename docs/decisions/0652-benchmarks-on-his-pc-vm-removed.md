# 0652 The benchmarks run on his PC on his own omp; the VM is removed

- Status: v1 (decided 2026-11-18; for his review)
- Kind: process
- Phase/slice: fork build, phase 7
- Date: 2026-11-18

## Question
The benchmarks ran in the Hyper-V VM `jocasta-clean` (0481, 0497), on a fresh user whose omp knew nothing of his. Do they keep running there, or on his PC, and in which omp home?

## Facts
Verified 2026-11-18.
- `D:/vm` holds 107 GB: the VM's disk (22.0 GB), its checkpoint (67.7 GB) and the Windows ISO (7.7 GB). D: has 96 GB free.
- Results the VM made before they were copied out sit in its `C:\jc\results` (`D:/vm/README.md`).
- His PC: 31.5 GB of RAM against the VM's 8–18 GB. It has omp 18.22.2, Go 1.26.2, Python 3.14.2, Node 24.21.0 (mise), Claude Code 2.1.321 and opencode 1.18.22. It had no pnpm, yarn or Python 3.13.
- The 30 main tasks are 10 Go, 10 Python, 5 JavaScript and 5 TypeScript. The pilot (0482) had arms passing 2–3 of 5 tasks, at $0.10–2.81 and 0.5–60 minutes a trial.

## Options
- **The PC, his default omp home.** Pros: his skills, memory and settings are part of what is measured, as he runs it. No VM. Cons: A0 is no longer omp alone, and wall times are noisier while he uses the PC.
- **The PC, a `bench` omp profile.** Pros: clean like the VM. Cons: a sign-in of its own, and it measures an omp he doesn't run.
- **Keep the VM.** Pros: the clean machine of 0481. Cons: 107 GB, an 8 GB guest, a second toolchain to keep in step.

## Recommendation
The PC with a `bench` profile.

## Choice
His answer of 2026-11-18: the PC, on his default omp home ("we need skills and stuff for fine tuning"). The VM is deleted after its results are copied out. Corepack (pnpm, yarn) and Python 3.13 are installed on the PC. As built (`docs/research/fork-build.md` choices 85–88):
- **His omp folder.** His skills, settings and the MCP servers his `mcp.json` turns on reach every omp arm alike. The bench no longer refuses those servers; it still refuses a folder that would hide a layered arm's own `jocasta` server. Memory stays off in every arm as 0482 has it, so no trial learns from an earlier trial of the same task.
- **Installs.** Corepack's pnpm and yarn shims are enabled in mise's Node 24.21.0. Python 3.13.18 is installed through mise, with its GitHub attestation checked, and is not made the global Python.
- **Pausing.** Every run of the benchmark and of the settings trials pauses while a `PAUSE` file is in its run folder: the trial already running finishes, and no other trial starts until the file is deleted. Stopping the run and running the same command again still resumes it.
- **The VM.** `D:/vm/remove-vm.ps1`, run by him as administrator, copies `C:\jc\results` to `D:/thesis/data/vm-results/`, then removes the VM with its checkpoint, its disks and the ISO.

## Reason
His words: "move the benchmarks from the VM to my PC", "on default omp home, we need skills and stuff for fine tuning", "just be sure to make it pausable".

## Consequences
- Amends 0481: the clean-machine VM's checks stay as done, and there is no VM after this.
- Amends 0497: nothing more is installed inside a VM.
- Amends 0533: the benchmarks run again on his PC. The rows from the VM stay as history and are not compared with the PC's.
- Amends 0587: only the layered arms' check stays.
- A6 can't run on his omp folder while its `mcp.json` has setup's `jocasta` entry. A main run with A6 needs that entry moved aside for the run, or a `--profile` (`fork-build.md` For him 19).
- The model study on the tasks (`fork-build.md` For him 17) is the first run on the PC.
