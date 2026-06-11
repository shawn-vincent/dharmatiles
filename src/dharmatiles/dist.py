"""Small distribution DSL for tile spec parameters.

Spec authors can pass plain numbers for fixed values, or ``D[...]`` objects
for sampled values:

    r=1.0
    r=D[0.8:2.2]
    r=D[0.8:2.2].power(1.5)
    r=D[5:2, 10:1]
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol, TypeAlias, TypeVar

import numpy as np


T = TypeVar("T", int, float)
Sample: TypeAlias = T | "Distribution[T]"


class Distribution(Protocol[T]):
    """A value that can be sampled from a random generator."""

    def sample(self, rng: np.random.Generator, size=None):
        ...

    def bounds(self) -> tuple[T, T]:
        ...


@dataclass(frozen=True)
class Constant(Distribution[T]):
    value: T

    def sample(self, rng: np.random.Generator, size=None):
        if size is None:
            return self.value
        return np.full(size, self.value)

    def bounds(self) -> tuple[T, T]:
        return self.value, self.value


@dataclass(frozen=True)
class UniformRange(Distribution[float]):
    low: float
    high: float

    def sample(self, rng: np.random.Generator, size=None):
        return rng.uniform(self.low, self.high, size)

    def bounds(self) -> tuple[float, float]:
        return self.low, self.high

    def power(self, exponent: float) -> "PowerRange":
        return PowerRange(self.low, self.high, exponent)


@dataclass(frozen=True)
class PowerRange(Distribution[float]):
    low: float
    high: float
    exponent: float

    def sample(self, rng: np.random.Generator, size=None):
        u = rng.uniform(0.0, 1.0, size) ** self.exponent
        return self.low + (self.high - self.low) * u

    def bounds(self) -> tuple[float, float]:
        return self.low, self.high


@dataclass(frozen=True)
class Triangular(Distribution[float]):
    low: float
    mode: float
    high: float

    def sample(self, rng: np.random.Generator, size=None):
        return rng.triangular(self.low, self.mode, self.high, size)

    def bounds(self) -> tuple[float, float]:
        return self.low, self.high


@dataclass(frozen=True)
class WeightedChoice(Distribution[T]):
    items: tuple[tuple[T, float], ...]

    def __post_init__(self) -> None:
        if not self.items:
            raise ValueError("WeightedChoice needs at least one item")
        if any(weight <= 0 for _, weight in self.items):
            raise ValueError("WeightedChoice weights must be positive")

    def sample(self, rng: np.random.Generator, size=None):
        values = np.array([value for value, _weight in self.items])
        weights = np.array([weight for _value, weight in self.items], dtype=float)
        probs = weights / weights.sum()
        return rng.choice(values, size=size, p=probs)

    def bounds(self) -> tuple[T, T]:
        values = [value for value, _weight in self.items]
        return min(values), max(values)


class _DistributionBuilder:
    """Implements the ``D[...]`` spec syntax."""

    def __getitem__(self, key):
        if isinstance(key, slice):
            if key.start is None or key.stop is None or key.step is not None:
                raise TypeError("D[low:high] expects exactly two bounds")
            return UniformRange(float(key.start), float(key.stop))

        if isinstance(key, tuple) and all(isinstance(item, slice) for item in key):
            pairs = []
            for item in key:
                if item.start is None or item.stop is None or item.step is not None:
                    raise TypeError("D[value:weight, ...] expects value:weight pairs")
                pairs.append((item.start, float(item.stop)))
            return WeightedChoice(tuple(pairs))

        raise TypeError("use D[low:high] or D[value:weight, ...]")

    def triangular(self, low: float, mode: float, high: float) -> Triangular:
        return Triangular(low, mode, high)

    def fixed(self, value: T) -> Constant[T]:
        return Constant(value)


D = _DistributionBuilder()


def as_distribution(value: Sample[T]) -> Distribution[T]:
    if hasattr(value, "sample") and hasattr(value, "bounds"):
        return value
    return Constant(value)


def sample(value: Sample[T], rng: np.random.Generator, size=None):
    return as_distribution(value).sample(rng, size)


def bounds(value: Sample[T]) -> tuple[T, T]:
    return as_distribution(value).bounds()

