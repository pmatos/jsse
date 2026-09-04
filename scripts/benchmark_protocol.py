#!/usr/bin/env python3
"""Shared host-control and statistics helpers for performance benchmarks."""

import math
import os
import platform
import shutil
import subprocess
from pathlib import Path
from statistics import median


DEFAULT_LOADAVG_PATH = Path("/proc/loadavg")
DEFAULT_CPU_SYSFS_ROOT = Path("/sys/devices/system/cpu")
DEFAULT_INSTABILITY_THRESHOLD = 0.05


class LoadAverageUnavailable(RuntimeError):
    """Raised when the host load average cannot be measured reliably."""


class BusyHostError(RuntimeError):
    """Raised when the host is too busy for a controlled measurement."""

    def __init__(self, loadavg1: float, threshold: float):
        self.loadavg1 = loadavg1
        self.threshold = threshold
        super().__init__(
            f"loadavg1 {loadavg1:.2f} exceeds idle threshold {threshold:.2f}"
        )


def read_load_averages(path: Path = DEFAULT_LOADAVG_PATH) -> tuple[float, float, float]:
    """Read the one-, five-, and fifteen-minute load averages from procfs."""
    try:
        fields = path.read_text(encoding="utf-8").split()
        values = tuple(float(field) for field in fields[:3])
    except (OSError, ValueError) as exc:
        raise LoadAverageUnavailable(
            f"cannot read load averages from {path}: {exc}"
        ) from exc

    if len(values) != 3 or any(
        value < 0 or not math.isfinite(value) for value in values
    ):
        raise LoadAverageUnavailable(f"invalid load averages in {path}")
    return values


def require_idle(
    threshold: float, path: Path = DEFAULT_LOADAVG_PATH
) -> tuple[float, float, float]:
    """Return current load averages, or reject a host above the threshold."""
    loads = read_load_averages(path)
    if loads[0] > threshold:
        raise BusyHostError(loads[0], threshold)
    return loads


def get_available_cpus() -> list[int]:
    """Return logical CPUs on which this process is allowed to run."""
    try:
        return sorted(os.sched_getaffinity(0))
    except (AttributeError, OSError):
        return list(range(os.cpu_count() or 1))


def detect_cpu_topology(
    sysfs_root: Path = DEFAULT_CPU_SYSFS_ROOT,
    available_cpus: list[int] | None = None,
) -> dict:
    """Classify maximum CPU frequencies for every available logical CPU."""
    cpus = sorted(
        set(available_cpus if available_cpus is not None else get_available_cpus())
    )
    frequencies = {}
    unreadable = []

    for cpu in cpus:
        path = sysfs_root / f"cpu{cpu}" / "cpufreq" / "cpuinfo_max_freq"
        try:
            frequency = int(path.read_text(encoding="utf-8").strip())
            if frequency <= 0:
                raise ValueError("frequency must be positive")
            frequencies[cpu] = frequency
        except (OSError, ValueError):
            unreadable.append(cpu)

    base = {
        "available_cpus": cpus,
        "max_frequencies_khz": frequencies,
        "fast_cores": [],
    }
    if not cpus:
        return base | {
            "classification": "unreadable",
            "reason": "process has no available logical CPUs",
        }
    if unreadable:
        return base | {
            "classification": "unreadable",
            "reason": "missing maximum-frequency data for CPUs "
            + format_cpu_list(unreadable),
        }

    distinct = sorted(set(frequencies.values()))
    if len(distinct) == 1:
        return base | {
            "classification": "uniform",
            "reason": f"all available CPUs report {distinct[0]} kHz maximum",
        }

    max_frequency = distinct[-1]
    fast_cores = [cpu for cpu in cpus if frequencies[cpu] == max_frequency]
    return base | {
        "classification": "heterogeneous",
        "fast_cores": fast_cores,
        "max_frequency_khz": max_frequency,
        "reason": f"selected maximum-frequency cluster at {max_frequency} kHz",
    }


def format_cpu_list(cpus: list[int]) -> str:
    """Format logical CPU identifiers using taskset's compact range syntax."""
    ordered = sorted(set(cpus))
    if not ordered:
        return ""

    ranges = []
    start = previous = ordered[0]
    for cpu in ordered[1:]:
        if cpu == previous + 1:
            previous = cpu
            continue
        ranges.append(str(start) if start == previous else f"{start}-{previous}")
        start = previous = cpu
    ranges.append(str(start) if start == previous else f"{start}-{previous}")
    return ",".join(ranges)


def choose_cpu_pinning(topology: dict, taskset_path: str | None = None) -> dict:
    """Choose a taskset prefix only for a complete heterogeneous topology."""
    resolved_taskset = (
        taskset_path if taskset_path is not None else shutil.which("taskset")
    )
    if topology["classification"] != "heterogeneous":
        return {
            "applied": False,
            "command_prefix": [],
            "cpu_list": None,
            "reason": topology["reason"],
        }
    if not resolved_taskset:
        return {
            "applied": False,
            "command_prefix": [],
            "cpu_list": None,
            "reason": "heterogeneous topology detected but taskset is unavailable",
        }

    cpu_list = format_cpu_list(topology["fast_cores"])
    return {
        "applied": True,
        "command_prefix": [resolved_taskset, "--cpu-list", cpu_list],
        "cpu_list": cpu_list,
        "reason": f"pinned to maximum-frequency CPUs {cpu_list}",
    }


def summarize_repeats(
    values: list[float], instability_threshold: float = DEFAULT_INSTABILITY_THRESHOLD
) -> dict:
    """Summarize repeat values and identify a greater-than-threshold range."""
    if not values or any(value <= 0 or not math.isfinite(value) for value in values):
        raise ValueError("repeat values must be non-empty, finite, and positive")

    minimum = min(values)
    maximum = max(values)
    relative_range = maximum / minimum - 1
    return {
        "n": len(values),
        "median": median(values),
        "min": minimum,
        "max": maximum,
        "relative_range": relative_range,
        "unstable": maximum > minimum * (1 + instability_threshold),
        "instability_threshold": instability_threshold,
    }


def _command_output(command: list[str]) -> str | None:
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            env=os.environ | {"LC_ALL": "C"},
            text=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    output = result.stdout.strip()
    return output or None


def read_cpu_model() -> str | None:
    """Read lscpu's model name, with /proc/cpuinfo as a fallback."""
    lscpu = shutil.which("lscpu")
    if lscpu:
        output = _command_output([lscpu])
        if output:
            for line in output.splitlines():
                key, separator, value = line.partition(":")
                if separator and key.strip() == "Model name":
                    return value.strip() or None

    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            key, separator, value = line.partition(":")
            if separator and key.strip() in {"model name", "Processor"}:
                return value.strip() or None
    except OSError:
        pass
    return None


def read_nproc() -> int:
    """Read the logical CPU count using nproc, with affinity as a fallback."""
    nproc = shutil.which("nproc")
    if nproc:
        output = _command_output([nproc])
        if output:
            try:
                value = int(output)
                if value > 0:
                    return value
            except ValueError:
                pass
    return len(get_available_cpus())


def collect_host_fingerprint(
    topology: dict,
    pinning: dict,
    start_load: tuple[float, float, float] | None,
    load_error: str | None = None,
) -> dict:
    """Build JSON-serializable host metadata for later comparability audits."""
    loadavg = None
    if start_load is not None:
        loadavg = {
            "one_minute": start_load[0],
            "five_minutes": start_load[1],
            "fifteen_minutes": start_load[2],
        }

    return {
        "hostname": platform.node(),
        "cpu_model": read_cpu_model(),
        "nproc": read_nproc(),
        "loadavg_start": loadavg,
        "loadavg_error": load_error,
        "available_cpus": topology["available_cpus"],
        "cpu_topology": topology["classification"],
        "cpu_max_freq_khz": topology["max_frequencies_khz"],
        "fast_cores": topology["fast_cores"],
        "pinning": {
            "applied": pinning["applied"],
            "cpu_list": pinning["cpu_list"],
            "reason": pinning["reason"],
        },
    }
