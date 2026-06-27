"""Unified response contract. Every tool returns {ok, result, error}."""

from typing import Any


def ok(result: Any = None) -> dict:
    return {"ok": True, "result": result, "error": None}


def err(code: str, message: str, detail: str = "") -> dict:
    return {
        "ok": False,
        "result": None,
        "error": {"code": code, "message": message, "detail": detail},
    }


def dep_missing(tool_name: str, dep: str, install_hint: str) -> dict:
    return err(
        "DEPENDENCY_MISSING",
        f"{dep} not found. Install: {install_hint}",
        f"Tool '{tool_name}' requires {dep}",
    )


def timeout(tool_name: str, seconds: float) -> dict:
    return err("TIMEOUT", f"'{tool_name}' timed out after {seconds}s")


def provider_error(tool_name: str, msg: str) -> dict:
    return err("PROVIDER_ERROR", msg, f"Tool '{tool_name}' failed in provider")


def invalid_args(tool_name: str, msg: str) -> dict:
    return err("INVALID_ARGS", msg, f"Tool '{tool_name}' received invalid arguments")


def not_implemented(tool_name: str, detail: str = "") -> dict:
    return err(
        "NOT_IMPLEMENTED",
        f"'{tool_name}' is not available in this environment",
        detail,
    )
