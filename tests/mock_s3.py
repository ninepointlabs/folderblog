#!/usr/bin/env python3
"""A tiny local stand-in for an S3 bucket, for tests/e2e.sh (never the network).
PUT stores, HEAD/GET read back. POST /__fail and /__ok make PUTs fail or work.
Writes the port it listens on to the file named by its first argument."""
import sys, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

objects, state = {}, {"fail": False}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def reply(self, code, body=b"", headers=()):
        self.send_response(code)
        for k, v in headers: self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD": self.wfile.write(body)
    def key(self): return self.path.split("?")[0]
    def do_POST(self):
        if self.path in ("/__fail", "/__ok"):
            state["fail"] = self.path == "/__fail"
            return self.reply(200)
        self.reply(400)
    def do_PUT(self):
        if not self.headers.get("Authorization", "").startswith("AWS4-HMAC-SHA256 "):
            return self.reply(403, b"<Error><Code>AccessDenied</Code></Error>")
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if state["fail"]: return self.reply(500, b"<Error><Code>InternalError</Code></Error>")
        objects[self.key()] = body
        self.reply(200, headers=[("ETag", '"x"')])
    def do_HEAD(self):
        b = objects.get(self.key())
        if b is None: return self.reply(404)
        self.send_response(200); self.send_header("Content-Length", str(len(b))); self.end_headers()
    def do_GET(self):
        b = objects.get(self.key())
        self.reply(404) if b is None else self.reply(200, b)

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(srv.server_address[1]))
srv.serve_forever()
