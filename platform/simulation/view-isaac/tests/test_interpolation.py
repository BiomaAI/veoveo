from __future__ import annotations

import math
import unittest
from unittest.mock import patch

from veoveo_simulation_view.contracts import InterpolationPolicy
from veoveo_simulation_view.interpolation import (
    InterpolationResetReason,
    InterpolationRuntimeState,
    PoseInterpolator,
)
from veoveo_simulation_view.pose import (
    EntityPose,
    PoseMirror,
    PosePollResult,
    PoseSampleKind,
    PoseSnapshot,
)


class ManualClock:
    def __init__(self) -> None:
        self.now_ns = 0

    def __call__(self) -> int:
        return self.now_ns

    def advance(self, nanoseconds: int) -> None:
        self.now_ns += nanoseconds


def snapshot(
    sequence: int,
    timestamp_ns: int,
    position: tuple[float, float, float],
    orientation: tuple[float, float, float, float] = (
        0.0,
        0.0,
        0.0,
        1.0,
    ),
) -> PoseSnapshot:
    return PoseSnapshot(
        session_id="session-1",
        epoch_id="epoch-1",
        sequence=sequence,
        simulation_timestamp_ns=timestamp_ns,
        frame_uri="frames://world/synthetic/revision/r1",
        frame_digest=f"sha256:{'1' * 64}",
        entity_table_revision=1,
        entity_table_digest=f"sha256:{'2' * 64}",
        entities=(
            EntityPose(
                entity_id="aircraft-1",
                position_enu_m=position,
                orientation_xyzw=orientation,
                active=True,
                visible=True,
            ),
        ),
    )


def accepted(value: PoseSnapshot, skipped: int = 0) -> PosePollResult:
    return PosePollResult(
        kind=PoseSampleKind.ACCEPTED,
        snapshot=value,
        skipped_samples=skipped,
    )


class PoseInterpolatorTest(unittest.TestCase):
    def setUp(self) -> None:
        self.clock = ManualClock()
        self.timeline = PoseInterpolator(
            InterpolationPolicy.LINEAR,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            clock_ns=self.clock,
        )

    def test_linear_position_midpoint_uses_one_source_interval_delay(
        self,
    ) -> None:
        self.timeline.observe(accepted(snapshot(1, 0, (0.0, 2.0, 4.0))))
        self.clock.advance(50_000_000)
        self.timeline.observe(accepted(snapshot(2, 50_000_000, (10.0, 6.0, 0.0))))

        self.clock.advance(25_000_000)
        frame = self.timeline.render()

        assert frame is not None
        self.assertEqual(frame.simulation_timestamp_ns, 25_000_000)
        self.assertEqual(
            frame.entities[0].position_enu_m,
            (5.0, 4.0, 2.0),
        )
        diagnostics = self.timeline.diagnostics()
        self.assertEqual(diagnostics.state, InterpolationRuntimeState.INTERPOLATING)
        self.assertEqual(diagnostics.interpolation_alpha, 0.5)
        self.assertEqual(diagnostics.interpolation_delay_ns, 50_000_000)

    def test_shortest_arc_slerp_is_normalized(self) -> None:
        self.timeline.observe(accepted(snapshot(1, 0, (0.0, 0.0, 0.0))))
        self.clock.advance(50_000_000)
        self.timeline.observe(
            accepted(
                snapshot(
                    2,
                    50_000_000,
                    (0.0, 0.0, 0.0),
                    (0.0, 0.0, -1.0, 0.0),
                )
            )
        )
        self.clock.advance(25_000_000)

        frame = self.timeline.render()

        assert frame is not None
        quaternion = frame.entities[0].orientation_xyzw
        self.assertAlmostEqual(
            math.sqrt(sum(value * value for value in quaternion)),
            1.0,
        )
        self.assertAlmostEqual(quaternion[2], -math.sqrt(0.5))
        self.assertAlmostEqual(quaternion[3], math.sqrt(0.5))

    def test_equivalent_quaternion_sign_does_not_rotate(self) -> None:
        self.timeline.observe(accepted(snapshot(1, 0, (0.0, 0.0, 0.0))))
        self.clock.advance(50_000_000)
        self.timeline.observe(
            accepted(
                snapshot(
                    2,
                    50_000_000,
                    (0.0, 0.0, 0.0),
                    (0.0, 0.0, 0.0, -1.0),
                )
            )
        )
        self.clock.advance(25_000_000)

        frame = self.timeline.render()

        assert frame is not None
        self.assertEqual(
            frame.entities[0].orientation_xyzw,
            (0.0, 0.0, 0.0, 1.0),
        )

    def test_linear_never_extrapolates_past_current_sample(self) -> None:
        self.timeline.observe(accepted(snapshot(1, 0, (0.0, 0.0, 0.0))))
        self.clock.advance(50_000_000)
        self.timeline.observe(accepted(snapshot(2, 50_000_000, (10.0, 0.0, 0.0))))
        self.clock.advance(500_000_000)

        frame = self.timeline.render()

        assert frame is not None
        self.assertEqual(frame.simulation_timestamp_ns, 50_000_000)
        self.assertEqual(frame.entities[0].position_enu_m[0], 10.0)
        self.assertEqual(
            self.timeline.diagnostics().state,
            InterpolationRuntimeState.HOLDING,
        )

    def test_hold_latest_preserves_exact_source_pose(self) -> None:
        timeline = PoseInterpolator(
            InterpolationPolicy.HOLD_LATEST,
            maximum_cadence_hz=120,
            stale_after_ms=500,
            clock_ns=self.clock,
        )
        source = snapshot(7, 350_000_000, (1.0, 2.0, 3.0))
        timeline.observe(accepted(source))
        self.clock.advance(100_000_000)

        frame = timeline.render()

        assert frame is not None
        self.assertEqual(frame.source_sequence, source.sequence)
        self.assertEqual(
            frame.simulation_timestamp_ns,
            source.simulation_timestamp_ns,
        )
        self.assertIs(frame.entities, source.entities)
        self.assertEqual(
            timeline.diagnostics().state,
            InterpolationRuntimeState.HOLD_LATEST,
        )

    def test_discontinuities_reset_history_and_bound_diagnostics(self) -> None:
        first = snapshot(1, 0, (0.0, 0.0, 0.0))
        self.timeline.observe(accepted(first))
        self.timeline.observe(PosePollResult(kind=PoseSampleKind.REPEATED))
        self.timeline.observe(accepted(snapshot(4, 150_000_000, (3.0, 0.0, 0.0)), 2))

        frame = self.timeline.render()
        diagnostics = self.timeline.diagnostics()

        assert frame is not None
        self.assertEqual(frame.source_sequence, 4)
        self.assertIsNone(diagnostics.previous_source_sequence)
        self.assertEqual(diagnostics.current_source_sequence, 4)
        self.assertEqual(diagnostics.discontinuity_reset_count, 2)
        self.assertEqual(diagnostics.repeated_source_sample_count, 1)
        self.assertEqual(diagnostics.skipped_source_sample_count, 2)
        self.assertEqual(
            diagnostics.last_reset_reason,
            InterpolationResetReason.SEQUENCE_GAP,
        )

    def test_stale_and_authorization_resets_require_repriming(self) -> None:
        self.timeline.observe(accepted(snapshot(1, 0, (0.0, 0.0, 0.0))))
        self.timeline.reset(InterpolationResetReason.STALE)
        self.assertIsNone(self.timeline.render())
        self.timeline.observe(accepted(snapshot(2, 50_000_000, (1.0, 0.0, 0.0))))
        self.timeline.reset(InterpolationResetReason.AUTHORIZATION_REVISION_CHANGED)

        self.assertIsNone(self.timeline.render())
        self.assertEqual(
            self.timeline.diagnostics().last_reset_reason,
            InterpolationResetReason.AUTHORIZATION_REVISION_CHANGED,
        )

    def test_pose_mirror_reports_repeated_and_out_of_order_samples(self) -> None:
        class Reader:
            def __init__(self) -> None:
                self.generation = 2

            def latest(self) -> tuple[int, bytes]:
                self.generation += 1
                return self.generation, b"encoded"

        first = snapshot(5, 250_000_000, (0.0, 0.0, 0.0))
        mirror = PoseMirror.__new__(PoseMirror)
        mirror._binding = object()
        mirror._reader = Reader()
        mirror._generation = 2
        mirror._latest = first
        mirror._accepted_at = 0.0

        with patch(
            "veoveo_simulation_view.pose.decode_snapshot",
            return_value=snapshot(5, 250_000_000, (1.0, 0.0, 0.0)),
        ):
            repeated = mirror.poll()
        with patch(
            "veoveo_simulation_view.pose.decode_snapshot",
            return_value=snapshot(4, 300_000_000, (1.0, 0.0, 0.0)),
        ):
            reversed_sequence = mirror.poll()
        with patch(
            "veoveo_simulation_view.pose.decode_snapshot",
            return_value=snapshot(6, 200_000_000, (1.0, 0.0, 0.0)),
        ):
            reversed_time = mirror.poll()

        assert repeated is not None
        assert reversed_sequence is not None
        assert reversed_time is not None
        self.assertEqual(repeated.kind, PoseSampleKind.REPEATED)
        self.assertEqual(reversed_sequence.kind, PoseSampleKind.SEQUENCE_REVERSED)
        self.assertEqual(
            reversed_time.kind,
            PoseSampleKind.TIMESTAMP_NOT_INCREASING,
        )
        self.assertIs(mirror.latest, first)


if __name__ == "__main__":
    unittest.main()
