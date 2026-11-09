import { statfsSync } from "node:fs";
import { cpus, freemem, totalmem } from "node:os";
import type { Machine } from "../../frontier/protocol.ts";

/** All cores' busy and total time so far, ms. */
export interface CpuTimes {
	busy: number;
	total: number;
}

export function cpuTimes(): CpuTimes {
	let busy = 0;
	let total = 0;
	for (const { times } of cpus()) {
		const all = times.user + times.nice + times.sys + times.irq + times.idle;
		total += all;
		busy += all - times.idle;
	}
	return { busy, total };
}

/** `part` of `total` rounded to whole percents of `total`, so a figure only changes on a whole percent (0429). */
const roundToPercent = (part: number, total: number) =>
	total > 0 ? Math.round((Math.round((part / total) * 100) / 100) * total) : 0;

/**
 * One sample (0429): CPU busy fraction since `before` (all cores, to whole percents), RAM in use, and the free space
 * of the disk holding `dir`.
 */
export function sampleMachine(before: CpuTimes, now: CpuTimes, dir: string): Machine {
	const elapsed = now.total - before.total;
	const cpu = elapsed > 0 ? Math.round(((now.busy - before.busy) / elapsed) * 100) / 100 : 0;
	const ramTotal = totalmem();
	const disk = statfsSync(dir);
	const diskTotal = disk.blocks * disk.bsize;
	return {
		cpu: Math.min(1, Math.max(0, cpu)),
		ram: { used: roundToPercent(ramTotal - freemem(), ramTotal), total: ramTotal },
		disk: { free: roundToPercent(disk.bavail * disk.bsize, diskTotal), total: diskTotal },
	};
}
