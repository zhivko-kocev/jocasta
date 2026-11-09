import type { Database } from "bun:sqlite";

/** Where an event came from (0298). */
export type EventSource = "github" | "azure" | "calendar" | "thread" | "machine" | "inbox";
export type EventPriority = "urgent" | "normal" | "low";

/** A new event: its `sourceId` deduplicates it (0298). */
export interface NewEvent {
	sourceId: string;
	source: EventSource;
	/** What it is, for `today` and resolving: `review`, `pipeline`, `meeting-soon`, `disk`, `reminder`, … */
	kind: string;
	priority: EventPriority;
	/** One plain line. */
	text: string;
	url?: string;
	projectId?: string;
	threadId?: string;
}

/**
 * One round of a poller that owns kinds of events (SHR-8): its current events, the source ids it could see (open events
 * of its kinds outside that scope aren't resolved), and the failures that shrank the scope without ending the round.
 */
export interface PollRound {
	events: NewEvent[];
	observed: (sourceId: string) => boolean;
	failures: string[];
}

/** An `events` row (catalog.db v5). */
export interface EventRow {
	id: number;
	source_id: string;
	source: EventSource;
	kind: string;
	priority: EventPriority;
	text: string;
	url: string | null;
	project_id: string | null;
	thread_id: string | null;
	created_at: string;
	raised_at: string | null;
	reminded_at: string | null;
	resolved_at: string | null;
}

/**
 * The event queue in `catalog.db` (0298): one row per source id. A source id seen again while open changes nothing; one
 * whose cause had gone away and came back opens a fresh row, so it is raised again.
 */
export class EventQueue {
	constructor(
		private readonly db: Database,
		private readonly now: () => Date = () => new Date(),
	) {}

	/** Adds an event; returns its row when it is new, undefined when its source id is already open. */
	add(event: NewEvent): EventRow | undefined {
		const old = this.bySource(event.sourceId);
		if (old && old.resolved_at === null) return undefined;
		if (old) this.db.query("DELETE FROM events WHERE id = ?").run(old.id);
		return this.db
			.query<EventRow, Record<string, string | null>>(
				`INSERT INTO events (source_id, source, kind, priority, text, url, project_id, thread_id, created_at)
				 VALUES ($source_id, $source, $kind, $priority, $text, $url, $project_id, $thread_id, $created_at) RETURNING *`,
			)
			.get({
				source_id: event.sourceId,
				source: event.source,
				kind: event.kind,
				priority: event.priority,
				text: event.text,
				url: event.url ?? null,
				project_id: event.projectId ?? null,
				thread_id: event.threadId ?? null,
				created_at: this.now().toISOString(),
			}) as EventRow;
	}

	bySource(sourceId: string): EventRow | undefined {
		return this.db.query<EventRow, [string]>("SELECT * FROM events WHERE source_id = ?").get(sourceId) ?? undefined;
	}

	/** Open events, oldest first. */
	open(): EventRow[] {
		return this.db.query<EventRow, []>("SELECT * FROM events WHERE resolved_at IS NULL ORDER BY id").all();
	}

	markRaised(id: number): void {
		this.db.query("UPDATE events SET raised_at = ? WHERE id = ?").run(this.now().toISOString(), id);
	}

	markReminded(id: number): void {
		this.db.query("UPDATE events SET reminded_at = ? WHERE id = ?").run(this.now().toISOString(), id);
	}

	resolve(sourceId: string): void {
		this.db
			.query("UPDATE events SET resolved_at = ? WHERE source_id = ? AND resolved_at IS NULL")
			.run(this.now().toISOString(), sourceId);
	}

	/**
	 * A poller's current set for one source and kinds: everything open of those kinds that isn't in `current` has gone
	 * away (a review done, a pipeline green again), but only inside `observed`, the scope the poller actually saw this
	 * round (SHR-8): an event outside it (a listing that failed or was cut short) stays open. Returns the new events.
	 */
	sync(
		source: EventSource,
		kinds: readonly string[],
		current: readonly NewEvent[],
		observed: (sourceId: string) => boolean = () => true,
	): EventRow[] {
		const seen = new Set(current.map((e) => e.sourceId));
		for (const row of this.open()) {
			if (row.source !== source || !kinds.includes(row.kind) || seen.has(row.source_id)) continue;
			if (observed(row.source_id)) this.resolve(row.source_id);
		}
		return current.flatMap((event) => this.add(event) ?? []);
	}

	/** Raised, unresolved urgent and normal events 30 minutes old or more, not yet reminded (0303). */
	dueReminders(afterMs: number): EventRow[] {
		const before = new Date(this.now().getTime() - afterMs).toISOString();
		return this.db
			.query<EventRow, [string]>(
				`SELECT * FROM events WHERE resolved_at IS NULL AND raised_at IS NOT NULL AND reminded_at IS NULL
				 AND priority IN ('urgent','normal') AND raised_at <= ? ORDER BY id`,
			)
			.all(before);
	}
}
