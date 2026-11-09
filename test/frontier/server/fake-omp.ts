// A stand-in for omp's headless run in the service's tests (the weekly big-picture review, 0470): writes its argv to
// `argv.json` in its working folder, then answers with a drift line, or with `FAKE_OMP_REPLY` when set.
import { writeFileSync } from "node:fs";

writeFileSync("argv.json", JSON.stringify(process.argv.slice(2)));
process.stdout.write(`${process.env.FAKE_OMP_REPLY ?? "Looked at the week.\n\nDRIFT: no drift found"}\n`);
