from __future__ import annotations

import queue
import threading
import time
from collections import deque
from typing import Generic, TypeVar


T = TypeVar("T")


class NonBlockingEventQueue(Generic[T]):
    """Bounded newest-value queue whose producers never wait for consumers."""

    def __init__(self, capacity: int) -> None:
        if capacity < 1:
            raise ValueError("event queue capacity must be positive")
        self._capacity = capacity
        self._queue: deque[T] = deque()
        self._condition = threading.Condition()
        self._dropped = 0

    def offer(self, value: T) -> None:
        with self._condition:
            if len(self._queue) == self._capacity:
                self._queue.popleft()
                self._dropped += 1
            self._queue.append(value)
            self._condition.notify()

    def take(self, timeout_seconds: float) -> T:
        deadline = time.monotonic() + timeout_seconds
        with self._condition:
            while not self._queue:
                remaining = deadline - time.monotonic()
                if remaining <= 0.0:
                    raise queue.Empty
                self._condition.wait(remaining)
            return self._queue.popleft()

    def depth(self) -> int:
        with self._condition:
            return len(self._queue)

    def dropped(self) -> int:
        with self._condition:
            return self._dropped
