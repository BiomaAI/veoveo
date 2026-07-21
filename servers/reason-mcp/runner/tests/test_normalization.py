import json
from fractions import Fraction

from reason_runner.prompting import normalize_events, truncate_text
from reason_runner.protocol import IndexRange
from reason_runner.video import frame_index, uniform_indices


def test_uniform_indices_cover_the_span_evenly() -> None:
    assert uniform_indices(3, 16) == [0, 1, 2]
    assert uniform_indices(90, 4) == [0, 30, 59, 89]
    assert uniform_indices(0, 4) == []
    assert uniform_indices(90, 1) == [0]


def test_frame_index_reconstructs_the_source_timeline() -> None:
    # 1 GHz media timescale: pts is already nanoseconds.
    assert frame_index(pts=250, time_base=Fraction(1, 1_000_000_000), decode_start_index=100) == 350
    # 30 fps stream time base still lands on exact nanoseconds.
    assert (
        frame_index(pts=3, time_base=Fraction(1, 30), decode_start_index=0) == 100_000_000
    )


def test_normalize_events_drops_what_the_server_would_reject() -> None:
    span = IndexRange(start=0, end=100)
    raw = json.dumps(
        {
            "events": [
                {
                    "range": {"start": 40, "end": 50},
                    "label": "second",
                    "description": "in range, cited track",
                    "track_ids": [7, 9],
                },
                {
                    "range": {"start": 10, "end": 20},
                    "label": "first",
                    "description": "in range",
                },
                {
                    "range": {"start": 90, "end": 200},
                    "label": "outside",
                    "description": "beyond the requested range",
                },
                {
                    "range": {"start": 30, "end": 20},
                    "label": "inverted",
                    "description": "start after end",
                },
                {"range": {"start": 5, "end": 6}, "label": "  ", "description": "blank label"},
            ]
        }
    )
    answer = normalize_events(raw, span, max_events=10, grounded_tracks={7})
    assert [event.label for event in answer.events] == ["first", "second"]
    assert answer.events[1].track_ids == [7]


def test_truncate_text_respects_utf8_boundaries() -> None:
    assert truncate_text("  plain  ", 100) == "plain"
    truncated = truncate_text("héllo wörld", 6)
    assert len(truncated.encode("utf-8")) <= 6
