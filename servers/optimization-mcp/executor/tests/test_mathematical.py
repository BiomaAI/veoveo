import unittest
from unittest.mock import MagicMock, patch

with patch.dict("sys.modules", {"numpy": MagicMock()}):
    from veoveo_cuopt_executor.mathematical import (
        _optional_array,
        _requires_barrier,
    )


class _Array:
    def __init__(self, values: list[float]) -> None:
        self.values = values

    def tolist(self) -> list[float]:
        return self.values


class _Solution:
    def __init__(self, values: list[float]) -> None:
        self.values = values

    def values_method(self) -> _Array:
        return _Array(self.values)


class MathematicalTests(unittest.TestCase):
    def test_only_linear_models_keep_the_pdlp_path(self) -> None:
        self.assertFalse(
            _requires_barrier(
                {
                    "quadratic_objective": None,
                    "quadratic_constraints": [],
                }
            )
        )

    def test_every_quadratic_form_requires_barrier(self) -> None:
        self.assertTrue(
            _requires_barrier(
                {
                    "quadratic_objective": {
                        "values": [1.0],
                        "indices": [0],
                        "offsets": [0, 1],
                    },
                    "quadratic_constraints": [],
                }
            )
        )

    def test_optional_non_finite_duals_are_omitted(self) -> None:
        self.assertEqual(
            _optional_array(_Solution([float("nan")]), "values_method"),
            [],
        )
        self.assertEqual(
            _optional_array(_Solution([1.0, 2.0]), "values_method"),
            [1.0, 2.0],
        )
        self.assertTrue(
            _requires_barrier(
                {
                    "quadratic_objective": None,
                    "quadratic_constraints": [{"constraint_id": "cone"}],
                }
            )
        )


if __name__ == "__main__":
    unittest.main()
