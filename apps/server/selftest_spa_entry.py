#!/usr/bin/env python3
"""M8 staged SPA entry regression tests; no production DB or port assumptions."""
from __future__ import annotations

import importlib.util
import json
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("rip_test_rp_server", ROOT / "apps/server/rp_server.py")
assert SPEC and SPEC.loader
rp = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rp)


class SpaEntryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="rip-spa-entry-")
        cls.static = Path(cls.temp.name)
        (cls.static / "assets").mkdir()
        (cls.static / "index.html").write_text("<!doctype html><title>RIP staged SPA</title>", encoding="utf-8")
        (cls.static / "assets" / "app-123456.js").write_text("/* fixture */", encoding="utf-8")
        rp.STATIC_DIR = str(cls.static)
        rp.SPA_FALLBACK = True
        rp.AUTH_ENABLED = True
        rp.AUTH_TOKEN = "spa-entry-test-token"
        rp.API_AUDIT_LOG = ""
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), rp.RpHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_address[1]}"

    @classmethod
    def tearDownClass(cls) -> None:
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)
        cls.temp.cleanup()

    def request(self, method: str, path: str, *, accept: str = "", origin: str = "", auth: bool = False):
        headers = {}
        if accept:
            headers["Accept"] = accept
        if origin:
            headers["Origin"] = origin
        if auth:
            headers["Authorization"] = "Bearer spa-entry-test-token"
        req = Request(self.base + path, data=b"{}" if method == "POST" else None, headers=headers, method=method)
        try:
            return urlopen(req, timeout=5)
        except HTTPError as exc:
            return exc

    def test_deep_link_falls_back_to_index_only_for_html_navigation(self) -> None:
        response = self.request("GET", "/projects/prj-deep-link", accept="text/html")
        body = response.read().decode()
        response.close()
        self.assertEqual(response.status, 200)
        self.assertIn("RIP staged SPA", body)
        self.assertEqual(response.headers.get("Cache-Control"), "no-store")
        self.assertIn("Content-Security-Policy", response.headers)
        self.assertIn("sha256-bCGf+2x3SGtiR7jTFx4Abmpdv2F5IX83N1F+KitZMUo=", response.headers["Content-Security-Policy"])
        self.assertEqual(response.headers.get("X-Frame-Options"), "DENY")

    def test_missing_asset_remains_404(self) -> None:
        response = self.request("GET", "/assets/missing.js", accept="text/html")
        payload = json.loads(response.read())
        response.close()
        self.assertEqual(response.status, 404)
        self.assertEqual(payload["error"]["code"], "not_found")

    def test_api_404_is_json_and_never_rewritten_to_spa(self) -> None:
        response = self.request("GET", "/api/not-a-route", accept="text/html", auth=True)
        payload = json.loads(response.read())
        response.close()
        self.assertEqual(response.status, 404)
        self.assertEqual(response.headers.get("Content-Type", "").split(";")[0], "application/json")
        self.assertEqual(payload["error"]["code"], "no_route")

    def test_cross_origin_write_is_rejected(self) -> None:
        response = self.request("POST", "/api/projects", origin="https://attacker.invalid")
        payload = json.loads(response.read())
        response.close()
        self.assertEqual(response.status, 403)
        self.assertEqual(payload["error"]["code"], "origin_mismatch")

    def test_same_origin_write_passes_origin_guard(self) -> None:
        origin = self.base
        response = self.request("POST", "/api/not-a-write-route", origin=origin)
        payload = json.loads(response.read())
        response.close()
        self.assertEqual(response.status, 405)
        self.assertEqual(payload["error"]["code"], "readonly")


if __name__ == "__main__":
    unittest.main(verbosity=2)
