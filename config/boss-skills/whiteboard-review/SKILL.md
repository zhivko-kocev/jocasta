---
name: whiteboard-review
description: Showing him a whiteboard review in his browser with `show`. Use when he asks to see a review, a diagram or an explanation on the whiteboard.
---

# Showing a whiteboard review

1. Create the review first with whiteboard's `session_create` (read its `session_get_instructions` before the first one).
2. Pass the new review's `sessionId` to `show(review)`.
3. `show` refuses an id whiteboard doesn't have, and says why: tell him in one line, never retry in a loop.
