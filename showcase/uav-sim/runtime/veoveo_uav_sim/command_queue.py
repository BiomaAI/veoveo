from __future__ import annotations

import queue
import threading
from dataclasses import dataclass, field
from typing import Any, Callable


@dataclass(slots=True)
class MainThreadCall:
    callback: Callable[[], Any]
    done: threading.Event = field(default_factory=threading.Event)
    result: Any = None
    error: BaseException | None = None


class MainThreadQueue:
    def __init__(self) -> None:
        self._queue: queue.Queue[MainThreadCall] = queue.Queue()

    def submit(self, callback: Callable[[], Any], timeout_seconds: float = 90.0) -> Any:
        call = MainThreadCall(callback)
        self._queue.put(call)
        if not call.done.wait(timeout_seconds):
            raise TimeoutError("simulator main thread did not accept the command")
        if call.error is not None:
            raise call.error
        return call.result

    def drain(self) -> None:
        while True:
            try:
                call = self._queue.get_nowait()
            except queue.Empty:
                return
            try:
                call.result = call.callback()
            except BaseException as error:
                call.error = error
            finally:
                call.done.set()
