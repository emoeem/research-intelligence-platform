"""Unit tests for the Phase 7 external source monitor worker."""
from __future__ import annotations

import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rip3_source_monitor as monitor


class Handler(BaseHTTPRequestHandler):
    etag = '"v1"'
    status = 200
    body = b"phase7-source-body"

    def do_HEAD(self):
        self.send_response(self.status)
        if self.status == 200:
            self.send_header("ETag", self.etag)
        self.end_headers()

    def do_GET(self):
        self.send_response(self.status)
        if self.status == 200:
            self.send_header("Content-Length", str(len(self.body)))
        self.end_headers()
        if self.command != "HEAD" and self.status == 200:
            self.wfile.write(self.body)

    def log_message(self, *_args):
        pass


class SourceMonitorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.url = f"http://127.0.0.1:{cls.server.server_port}/source"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.thread.join(timeout=2)

    def test_etag_and_link_status_are_observed(self):
        with patch.object(
            monitor,
            "resolve_public_url",
            return_value=("127.0.0.1", self.server.server_port, "127.0.0.1"),
        ):
            status, etag, modified, digest = monitor.source_hash(self.url, {}, 2)
        self.assertEqual(status, 200)
        self.assertEqual(etag, '"v1"')
        self.assertIsNone(modified)
        self.assertIsNone(digest)

    def test_head_fallback_get_hashes_content(self):
        original = Handler.do_HEAD
        Handler.do_HEAD = lambda self: (self.send_response(405), self.end_headers())
        try:
            with patch.object(
                monitor,
                "resolve_public_url",
                return_value=("127.0.0.1", self.server.server_port, "127.0.0.1"),
            ):
                status, etag, _modified, digest = monitor.source_hash(self.url, {}, 2)
            self.assertEqual(status, 200)
            self.assertIsNone(etag)
            self.assertEqual(len(digest), 64)
        finally:
            Handler.do_HEAD = original

    def test_private_address_is_rejected_by_default(self):
        with self.assertRaises(ValueError):
            monitor.validate_public_url(self.url)

    def test_redirect_to_private_host_is_checked_before_following(self):
        class Redirect:
            status_code = 302
            headers = {"Location": "http://127.0.0.1:5432/private"}
            def close(self):
                pass

        redirect = monitor.PinnedResponse(
            302,
            {"Location": "http://127.0.0.1:5432/private"},
            b"",
        )

        def resolve(url):
            if url.startswith("https://public.example"):
                return ("public.example", 443, "93.184.216.34")
            raise ValueError("private target rejected")

        with patch.object(monitor, "resolve_public_url", side_effect=resolve):
            with patch.object(monitor, "pinned_request", return_value=redirect):
                with self.assertRaises(ValueError):
                    monitor.checked_request("HEAD", "https://public.example/source", {}, 1)


if __name__ == "__main__":
    unittest.main()
