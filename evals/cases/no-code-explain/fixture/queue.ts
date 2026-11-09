/** A first-in, first-out queue that holds at most `capacity` items. */
export class BoundedQueue<T> {
	private readonly items: T[] = [];

	constructor(private readonly capacity: number) {
		if (capacity < 1) throw new RangeError("capacity must be at least 1");
	}

	get size(): number {
		return this.items.length;
	}

	/** Adds `item` at the back; when the queue is full, the oldest item is dropped to make room and returned. */
	push(item: T): T | undefined {
		const dropped = this.items.length >= this.capacity ? this.items.shift() : undefined;
		this.items.push(item);
		return dropped;
	}

	/** Removes and returns the item at the front, or undefined when the queue is empty. */
	shift(): T | undefined {
		return this.items.shift();
	}
}
