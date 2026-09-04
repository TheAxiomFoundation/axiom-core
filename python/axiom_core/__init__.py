"""Thin, standard-library transport for the actual axiom-core Rust CLI.

Requests and receipts remain ordinary JSON dictionaries. The Rust facade and
its pinned native engine own validation and all execution semantics. Bundles
at this checkpoint are unsigned development artifacts; matching a digest is
not authentication or legal validation.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
from typing import Any

__all__ = ["AxiomCoreError", "build", "capabilities", "execute", "verify"]

PathArg = str | os.PathLike[str]


class AxiomCoreError(RuntimeError):
    """CLI or transport failure, retaining the complete CLI error response."""

    def __init__(
        self,
        code: str,
        message: str,
        *,
        response: Any = None,
        returncode: int | None = None,
        stderr: str = "",
    ) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message
        self.response = response
        self.returncode = returncode
        self.stderr = stderr


def _resolve_binary(binary: PathArg | None) -> str:
    if binary is not None:
        return os.fspath(binary)
    configured = os.environ.get("AXIOM_CORE_BIN")
    if configured:
        return configured
    return str(Path(__file__).resolve().parents[2] / "target" / "debug" / "axiom-core")


def _invoke(
    arguments: list[str],
    *,
    request: dict[str, Any] | None = None,
    binary: PathArg | None,
    timeout: float,
) -> dict[str, Any]:
    # Stdin keeps household values out of process arguments and temporary files.
    # No request field is selected, renamed, defaulted, or silently discarded.
    body = None if request is None else json.dumps(request, allow_nan=False)
    try:
        completed = subprocess.run(
            [_resolve_binary(binary), *arguments],
            input=body,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise AxiomCoreError(
            "transport_timeout", f"axiom-core exceeded the {timeout:g}-second timeout"
        ) from error
    except OSError as error:
        raise AxiomCoreError("transport_unavailable", str(error)) from error

    if completed.returncode:
        try:
            response = json.loads(completed.stderr)
        except (ValueError, TypeError):
            response = None
        detail = response.get("error") if isinstance(response, dict) else None
        code = detail.get("code") if isinstance(detail, dict) else None
        message = detail.get("message") if isinstance(detail, dict) else None
        raise AxiomCoreError(
            code if isinstance(code, str) else "cli_error",
            message if isinstance(message, str) else "axiom-core exited without a structured error",
            response=response,
            returncode=completed.returncode,
            stderr=completed.stderr,
        )

    try:
        response = json.loads(completed.stdout)
    except ValueError as error:
        raise AxiomCoreError(
            "transport_protocol", "axiom-core returned invalid JSON", stderr=completed.stderr
        ) from error
    if not isinstance(response, dict):
        raise AxiomCoreError(
            "transport_protocol", "axiom-core returned a non-object JSON response",
            response=response, stderr=completed.stderr,
        )
    return response


def execute(
    bundle_path: PathArg,
    expected_bundle_sha256: str,
    request: dict[str, Any],
    *,
    binary: PathArg | None = None,
    timeout: float = 30,
) -> dict[str, Any]:
    """Execute the unchanged native request and return the entire Rust receipt.

    The Rust invocation verifies the expected bundle digest before execution.
    ``binary`` overrides ``AXIOM_CORE_BIN``; otherwise the repository's debug
    binary is used. ``timeout`` is a subprocess timeout in seconds.
    """
    return _invoke(
        ["run", "--bundle", os.fspath(bundle_path), "--expect", expected_bundle_sha256],
        request=request, binary=binary, timeout=timeout,
    )


def build(
    spec_path: PathArg,
    bundle_path: PathArg,
    *,
    binary: PathArg | None = None,
    timeout: float = 30,
) -> dict[str, Any]:
    """Compile an explicit source snapshot with the pinned engine into a bundle."""
    return _invoke(
        ["build", "--spec", os.fspath(spec_path), "--out", os.fspath(bundle_path)],
        binary=binary, timeout=timeout,
    )


def verify(
    bundle_path: PathArg,
    expected_bundle_sha256: str,
    *,
    binary: PathArg | None = None,
    timeout: float = 30,
) -> dict[str, Any]:
    """Verify bundle integrity with Rust; this does not authenticate its author."""
    return _invoke(
        ["verify", "--bundle", os.fspath(bundle_path), "--expect", expected_bundle_sha256],
        binary=binary, timeout=timeout,
    )


def capabilities(
    *, binary: PathArg | None = None, timeout: float = 30,
) -> dict[str, Any]:
    """Return the CLI's full capabilities response without a mirrored schema."""
    return _invoke(["capabilities"], binary=binary, timeout=timeout)
