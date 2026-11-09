#!/usr/bin/env python3
"""rip.smoke.native 容器内冒烟工具（P2-5 批次交付物）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求，stdout 一行 JSON 结果。
请求形状：{"run_id": "...", "input": {...}, "budget": {...}}；
结果形状：{"status": "result", "data": {...}}，错误帧
{"status": "error", "error": {"code": -32006, "message": "..."}}。

同时把完整结果落盘 /out/result.json（Adapter 收集为 Artifact，
sha256 与 artifact 表对账，--userns=keep-id 产物属主为宿主用户）。

自证字段（§5.4 约束清单的容器内视角证据）：
- uid：运行进程 uid（keep-id 下应等于宿主 uid，非 root）；
- cap_eff：/proc/self/status CapEff（--cap-drop ALL 下应为全零）；
- tmpfs_writable：/tmp 可写（--read-only 根文件系统 + --tmpfs /tmp）；
- rootfs_readonly：/（根）写入应失败（只读根文件系统）。
"""

import json
import os
import sys
import time


def read_cap_eff() -> str:
    try:
        with open("/proc/self/status", "r", encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("CapEff:"):
                    return line.split(":", 1)[1].strip()
    except OSError as exc:  # pragma: no cover - /proc 恒在
        return f"unreadable: {exc}"
    return "missing"


def rootfs_readonly() -> bool:
    """尝试写根路径（/），成功 False（根可写 = 约束失效），失败 True。"""
    try:
        with open("/.rip_smoke_write_probe", "w", encoding="utf-8") as fh:
            fh.write("x")
        os.unlink("/.rip_smoke_write_probe")
        return False
    except OSError:
        return True


def main() -> int:
    raw = sys.stdin.readline()
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(json.dumps({
            "status": "error",
            "error": {"code": -32700, "message": f"request parse failed: {exc}"},
        }, ensure_ascii=False))
        return 2

    inp = request.get("input") or {}
    sleep_ms = 0
    if isinstance(inp, dict):
        sleep_ms = int(inp.get("sleep_ms", 0) or 0)
    if sleep_ms:
        time.sleep(min(sleep_ms, 600_000) / 1000.0)

    echo = inp.get("echo") if isinstance(inp, dict) else None
    echo = echo if isinstance(echo, str) else json.dumps(inp, ensure_ascii=False)

    # /out 产物（Artifact 收集对账对象）；只读根下唯一可写挂载。
    out_payload = {
        "echo": echo,
        "uid": os.getuid(),
        "cap_eff": read_cap_eff(),
        "tmpfs_writable": True,
        "rootfs_readonly": rootfs_readonly(),
        "run_id": request.get("run_id"),
    }
    try:
        os.makedirs("/out", exist_ok=True)
        with open("/out/result.json", "w", encoding="utf-8") as fh:
            json.dump(out_payload, fh, ensure_ascii=False, sort_keys=True)
    except OSError as exc:
        print(json.dumps({
            "status": "error",
            "error": {"code": -32006, "message": f"/out write failed: {exc}"},
        }, ensure_ascii=False))
        return 1

    # /tmp tmpfs 自证（写入即删，不残留）。
    try:
        with open("/tmp/.rip_smoke_tmp_probe", "w", encoding="utf-8") as fh:
            fh.write("x")
        os.unlink("/tmp/.rip_smoke_tmp_probe")
    except OSError:
        out_payload["tmpfs_writable"] = False

    print(json.dumps({"status": "result", "data": out_payload}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
