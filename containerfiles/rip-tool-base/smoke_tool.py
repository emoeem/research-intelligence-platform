#!/usr/bin/env python3
"""rip.smoke.native 容器内冒烟工具（P2-5 批次交付物）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求，stdout 一行 JSON 结果。
请求形状：{"run_id": "...", "input": {...}, "budget": {...}}；
结果形状：{"status": "result", "data": {...}}，错误帧
{"status": "error", "error": {"code": -32006, "message": "..."}}。

同时把完整结果落盘 /task/out/result.json（Adapter 收集为 Artifact，
sha256 与 artifact 表对账，--userns=keep-id 产物属主为宿主用户）。

自证字段（§5.4 约束清单的容器内视角证据）：
- uid：运行进程 uid（keep-id 下应等于宿主 uid，非 root）；
- cap_eff：/proc/self/status CapEff（--cap-drop ALL 下应为全零）；
- tmpfs_writable：/tmp 可写（--read-only 根文件系统 + --tmpfs /tmp）；
- rootfs_readonly：/（根）写入应失败（只读根文件系统）。
"""

import json
import os
import signal
import sys
import time


def _on_term(signum, _frame):
    """SIGTERM 处理器（D-TC-04 协作式取消的容器内侧）。

    容器 PID1 对未安装处理器的信号按默认忽略语义处理（实测 TERM 不生效），
    显式安装后 Adapter 的优雅停止窗口才能真正终止工具进程。
    """
    sys.exit(128 + signum)


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
    signal.signal(signal.SIGTERM, _on_term)
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
        # 睡眠前先落一个早期产物：超时/取消路径的 partial Artifact 登记载体
        # （ADR 5 第 3 条：部分产出照常登记，不静默丢弃）。
        try:
            with open("/task/out/early.txt", "w", encoding="utf-8") as fh:
                fh.write(f"run_id={request.get('run_id')}\nphase=before-sleep\n")
        except OSError:
            pass
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
        os.makedirs("/task/out", exist_ok=True)
        with open("/task/out/result.json", "w", encoding="utf-8") as fh:
            json.dump(out_payload, fh, ensure_ascii=False, sort_keys=True)
    except OSError as exc:
        print(json.dumps({
            "status": "error",
            "error": {"code": -32006, "message": f"/task/out write failed: {exc}"},
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
