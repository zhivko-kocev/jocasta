import type { Database } from "bun:sqlite";

/** An inbox row (me.db v1, 0302): a note in a project's next work, or a reminder at a time. */
export interface InboxItem {
	id: number;
	text: string;
	project_id: string | null;
	due_at: string | null;
	created_at: string;
	done_at: string | null;
}

/** The inbox (0302) in `me.db`. */
export class Inbox {
	constructor(
		private readonly db: Database,
		private readonly now: () => Date = () => new Date(),
	) {}

	/** Files an item: a project's next work (no time), or `today` at a time (with or without a project). */
	add(text: string, where: { projectId?: string; dueAt?: Date }): InboxItem {
		if (!where.projectId && !where.dueAt) throw new Error("an inbox item needs a project or a time");
		return this.db
			.query<InboxItem, [string, string | null, string | null, string]>(
				"INSERT INTO inbox (text, project_id, due_at, created_at) VALUES (?, ?, ?, ?) RETURNING *",
			)
			.get(text, where.projectId ?? null, where.dueAt?.toISOString() ?? null, this.now().toISOString()) as InboxItem;
	}

	get(id: number): InboxItem | undefined {
		return this.db.query<InboxItem, [number]>("SELECT * FROM inbox WHERE id = ?").get(id) ?? undefined;
	}

	/** Marks an item done; false when there is no open item with that id. */
	done(id: number): boolean {
		const result = this.db
			.query("UPDATE inbox SET done_at = ? WHERE id = ? AND done_at IS NULL")
			.run(this.now().toISOString(), id);
		return result.changes > 0;
	}

	/** A project's open notes: its next work. */
	notes(projectId: string): InboxItem[] {
		return this.db
			.query<InboxItem, [string]>(
				"SELECT * FROM inbox WHERE done_at IS NULL AND due_at IS NULL AND project_id = ? ORDER BY id",
			)
			.all(projectId);
	}

	/** Open reminders due between `from` and `to`, by time. */
	reminders(from: Date, to: Date): InboxItem[] {
		return this.db
			.query<InboxItem, [string, string]>(
				"SELECT * FROM inbox WHERE done_at IS NULL AND due_at >= ? AND due_at < ? ORDER BY due_at",
			)
			.all(from.toISOString(), to.toISOString());
	}

	/** Open reminders whose time has come. */
	due(): InboxItem[] {
		return this.db
			.query<InboxItem, [string]>("SELECT * FROM inbox WHERE done_at IS NULL AND due_at <= ? ORDER BY due_at")
			.all(this.now().toISOString());
	}
}
