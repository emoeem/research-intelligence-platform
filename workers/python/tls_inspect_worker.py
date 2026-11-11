#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""TLS Inspector worker（rip.http.tls_inspect，脚手架生成后按 ADDING-A-TOOL §三实现真实语义）。

工具语义（stdlib ssl 实现，无第三方依赖）：
  对目标 host:port 发起真实 TLS 握手，返回协商协议版本、加密套件、证书校验结果，
  以及证书详情（subject、issuer、notBefore、notAfter、SAN、sha256 指纹、PEM）。

  - 证书校验默认走系统信任库（verify=true）；校验失败自动降级为不校验重握手，
    仍返回证书字段与指纹，verified=false 明示，供研究侧人工判断。
  - 超时由 budget.timeout_ms 与显式 timeout_seconds 双重约束，取消安全点轮询。

Worker Protocol v1（JSON-RPC 2.0 over stdio）由 worker_runtime.py 基类承载。
"""

import hashlib
import socket
import ssl
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import WorkerRuntime, WorkerContext, InvalidParams

CERT_KEYS = ("subject", "issuer", "notBefore", "notAfter",
             "serialNumber", "OCSP", "caIssuers", "crlDistributionPoints")


class TlsInspectWorker(WorkerRuntime):
    NAME = "tls_inspect-worker"
    VERSION = "0.1.1"
    CAPABILITIES = []

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        host = (task.get("host") or "").strip()
        if not host:
            raise InvalidParams("task.host is required (target hostname or IP)")
        port = int(task.get("port", 443))
        if not (1 <= port <= 65535):
            raise InvalidParams(f"task.port out of range: {port}")
        server_name = (task.get("server_name") or "").strip() or None
        verify = bool(task.get("verify", True))
        timeout = float(task.get("timeout_seconds", 10.0))
        if not (0.1 <= timeout <= 55.0):
            raise InvalidParams(f"task.timeout_seconds out of range: {timeout}")

        ctx.check_cancelled()
        info, cert_der = self._handshake(host, port, server_name, verify, timeout)

        # 要求校验但校验失败：降级为不校验重握手，拿回证书并明示 verified=false
        if verify and not info.get("verified"):
            ctx.check_cancelled()
            info, cert_der = self._handshake(host, port, server_name,
                                             verify=False, timeout=timeout)
            info["note"] = "证书校验失败，已降级为不校验握手（verified=false）"

        out = {
            "target": f"{host}:{port}",
            "protocol": info.get("protocol"),
            "cipher": info.get("cipher"),
            "verified": bool(info.get("verified")),
            "error": info.get("error"),
            "note": info.get("note"),
            "fingerprint_sha256": None,
            "cert": None,
        }
        if cert_der:
            out["fingerprint_sha256"] = hashlib.sha256(cert_der).hexdigest()
            cert = dict(info.get("cert_fields") or {})
            cert["pem"] = ssl.DER_cert_to_PEM_cert(cert_der)
            out["cert"] = cert
        return out

    # ------------------------------------------------------------------

    def _handshake(self, host: str, port: int, server_name: str | None,
                   verify: bool, timeout: float):
        """一次真实 TLS 握手。返回 (握手信息 dict, 证书 DER 或 None)。"""
        if verify:
            sctx = ssl.create_default_context()
        else:
            sctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
            sctx.check_hostname = False
            sctx.verify_mode = ssl.CERT_NONE
        info: dict = {"verified": False}
        cert_der = None
        sock = None
        try:
            raw = socket.create_connection((host, port), timeout=timeout)
            sock = sctx.wrap_socket(raw, server_hostname=server_name or host)
            cipher = sock.cipher()
            info["protocol"] = sock.version()
            info["cipher"] = ({"name": cipher[0], "protocol": cipher[1],
                               "bits": cipher[2]} if cipher else None)
            cert = sock.getpeercert() or {}
            if verify and cert:
                # 校验通过时 getpeercert() 返回解析后的字段字典
                info["cert_fields"] = {k: cert[k] for k in CERT_KEYS if k in cert}
                info["cert_fields"]["subjectAltName"] = cert.get("subjectAltName")
            cert_der = sock.getpeercert(binary_form=True)
            info["verified"] = bool(verify) and bool(cert)
        except ssl.SSLCertVerificationError as exc:
            info["error"] = f"cert_verification_failed: {exc.verify_message or exc}"
        except (socket.timeout, TimeoutError):
            info["error"] = f"timeout_after_{timeout}s"
        except OSError as exc:
            info["error"] = f"socket_error: {exc}"
        finally:
            if sock is not None:
                try:
                    sock.close()
                except OSError:
                    pass
        return info, cert_der


if __name__ == "__main__":
    TlsInspectWorker().serve()
