"""Run after cargo build; requires tests/junit-requirements.txt, no network at test time."""
import collections
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from lxml import etree

BINARY = str(Path(sys.argv.pop(1)).resolve())
BASELINE = None
if "--baseline" in sys.argv:
    position = sys.argv.index("--baseline")
    BASELINE = str(Path(sys.argv[position + 1]).resolve())
    del sys.argv[position:position + 2]
SCHEMA_PATH = Path(__file__).parent / "schemas" / "junit-10.xsd"
SCHEMA_HASH = "a1a816f58d1bf95ebabf371994df0b9246dee66ea9572fbec4f9296f1b2c0ff6"
SCHEMA = etree.XMLSchema(etree.parse(str(SCHEMA_PATH)))


def git(root, *args):
    subprocess.run(["git", "-c", "user.name=fixture", "-c", "user.email=fixture@example.com", *args], cwd=root, check=True, capture_output=True)


class JunitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tessera-junit-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        git(self.root, "init", "-q")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self):
        git(self.root, "add", ".")
        git(self.root, "commit", "-q", "--allow-empty", "-m", "base")

    def call(self, *args):
        env = os.environ.copy()
        env.pop("TESSERA_DB", None)
        return subprocess.run([BINARY, *args], cwd=self.root, env=env, capture_output=True, timeout=30)

    def report(self, *scope, no_baseline=False):
        args = ["verify", *(scope or ("--changed",))]
        if no_baseline:
            args.append("--no-baseline")
        proc = self.call(*args, "--format", "junit")
        self.assertIn(proc.returncode, (0, 1), proc.stderr.decode())
        doc = etree.fromstring(proc.stdout)
        SCHEMA.assertValid(doc)
        self.assertEqual(doc.tag, "testsuite")
        self.assertEqual(doc.get("name"), "tessera")
        cases = doc.findall("testcase")
        self.assertEqual(int(doc.get("tests")), len(cases))
        self.assertEqual(int(doc.get("failures")), sum(c.find("failure") is not None for c in cases))
        self.assertEqual(int(doc.get("skipped")), sum(c.find("skipped") is not None for c in cases))
        keys = [(c.get("classname"), c.get("name")) for c in cases]
        self.assertEqual(keys, sorted(set(keys)))
        self.assertNotIn("V6", [key[0] for key in keys])
        normal = self.call("--json", *args)
        self.assertEqual(proc.returncode, normal.returncode)
        result = json.loads(normal.stdout)
        if BASELINE:
            env = os.environ.copy()
            env.pop("TESSERA_DB", None)
            before = subprocess.run([BASELINE, "--json", *args], cwd=self.root, env=env, capture_output=True, timeout=30)
            self.assertEqual(before.returncode, normal.returncode)
            self.assertEqual(json.loads(before.stdout), result)
        if result.get("files") == 0:
            self.assertEqual(cases, [])
            self.assertEqual(proc.returncode, 0)
            return proc, doc, result
        self.assertEqual(set(result), {"ok", "findings", "tests"})
        self.assertEqual(proc.returncode, 0 if result["ok"] else 1)
        props = {p.get("name"): p.get("value") for p in doc.findall("properties/property")}
        expected = {}
        counts = collections.Counter()
        for f in result["findings"]:
            if f["id"] == "V6":
                continue
            self.assertIn((f["id"], f["path"]), keys)
            counts[f["id"], f["path"]] += 1
            key = "tessera.finding|{}|{}|{}".format(f["id"], f["path"].encode().hex().upper(), counts[f["id"], f["path"]])
            expected[key + "|fingerprint"] = f["fingerprint"]
            expected[key + "|line"] = str(f["line"] or "")
        self.assertEqual(props, expected)
        self.assertEqual(proc.stdout, self.call(*args, "--format", "junit").stdout)
        return proc, doc, result

    def pairs(self, doc):
        return {(c.get("classname"), c.get("name")): c for c in doc.findall("testcase")}

    def simple(self):
        self.write("api.ts", "export function send(x: string) { return x; }\n")
        self.write("client.ts", "import { send } from './api';\nexport function client() { return send('ok'); }\n")
        self.commit()

    def test_existing_formats_remain_identical(self):
        if BASELINE is None:
            self.skipTest("pass --baseline BINARY for an immutable baseline comparison")
        self.simple()
        self.write("api.ts", "export function renamed(x: string) { return x; }\n")
        env = os.environ.copy()
        env.pop("TESSERA_DB", None)
        for format_name in ("human", "agent", "json", "sarif", "gitlab", "azure", "bitbucket", "ndjson", "markdown"):
            args = ["--quiet", "verify", "--changed", "--format", format_name]
            current = self.call(*args)
            before = subprocess.run([BASELINE, *args], cwd=self.root, env=env, capture_output=True, timeout=30)
            self.assertEqual(before.returncode, current.returncode, format_name)
            self.assertEqual(before.stdout, current.stdout, format_name)

    def test_v6_is_selected_but_excluded_from_xml(self):
        self.simple()
        self.write("test/client.test.ts", "import { client } from '../client';\nexport function testClient() { client(); }\n")
        self.commit()
        self.write("api.ts", "export function send(x: string) { return x.trim(); }\n")
        _, doc, result = self.report()
        self.assertIn("test/client.test.ts", result["tests"])
        self.assertNotIn("V6", [k[0] for k in self.pairs(doc)])

    def test_buildkite_recipe_preserves_verify_status_and_imports(self):
        import shutil
        if os.name == "nt" or shutil.which("sh") is None:
            self.skipTest("the documented Buildkite command uses a POSIX shell")
        docs = (Path(__file__).parent.parent / "docs" / "ci.md").read_text()
        recipe = docs.split("**Buildkite**", 1)[1].split("```sh\n", 1)[1].split("```", 1)[0]
        fake_bin = self.root / "bin"
        fake_bin.mkdir()
        tools = {
            "tessera": "#!/bin/sh\nprintf '%s\\n' '<testsuite name=\"tessera\" tests=\"0\" failures=\"0\" errors=\"0\"/>'\nexit \"$TEST_VERIFY_EXIT\"\n",
            "curl": "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TEST_UPLOAD_ARGS\"\nexit \"$TEST_UPLOAD_EXIT\"\n",
        }
        for name, content in tools.items():
            path = fake_bin / name
            path.write_text(content)
            path.chmod(0o755)
        for verify, upload, expected in ((0, 0, 0), (1, 0, 1), (3, 0, 3), (1, 7, 1), (0, 7, 7)):
            env = os.environ.copy()
            env.update({"PATH": str(fake_bin) + os.pathsep + env["PATH"], "TEST_VERIFY_EXIT": str(verify), "TEST_UPLOAD_EXIT": str(upload), "TEST_UPLOAD_ARGS": str(self.root / "upload-args"), "BUILDKITE_ANALYTICS_TOKEN": "synthetic-test-token"})
            for key in ("BUILD_ID", "BUILD_URL", "BRANCH", "COMMIT", "BUILD_NUMBER", "JOB_ID"):
                env["BUILDKITE_" + key] = "synthetic"
            proc = subprocess.run(["sh", "-eu", "-c", recipe], cwd=self.root, env=env, capture_output=True)
            self.assertEqual(proc.returncode, expected, proc.stderr.decode())
            args = (self.root / "upload-args").read_text()
            self.assertIn("data=@tessera-junit.xml", args)
            self.assertIn("format=junit", args)
            self.assertIn("https://analytics-api.buildkite.com/v1/uploads", args)

    def test_pinned_schema(self):
        self.assertEqual(hashlib.sha256(SCHEMA_PATH.read_bytes()).hexdigest(), SCHEMA_HASH)
        good = etree.fromstring(b'<testsuite name="tessera" tests="1" failures="0" errors="0"><testcase classname="V1" name="x"/></testsuite>')
        SCHEMA.assertValid(good)
        bad = etree.fromstring(b'<testsuite name="tessera" tests="1" failures="0" errors="0"><testcase name="x"><properties><property name="f" value="v"/></properties></testcase></testsuite>')
        self.assertFalse(SCHEMA.validate(bad))

    def test_successes_and_unchanged_affected_callers(self):
        self.simple()
        self.write("api.ts", "export function send(x: string) { return x.trim(); }\n")
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 0)
        pairs = self.pairs(doc)
        self.assertIn(("V2", "client.ts"), pairs)
        self.assertIn(("V3", "client.ts"), pairs)
        self.assertIn(("V4", "client.ts"), pairs)
        self.assertTrue(all(c.find("failure") is None for c in pairs.values()))
        self.assertFalse(any(k[0] == "V5" for k in pairs))

    def test_v1_and_multiple_findings_in_one_case(self):
        self.simple()
        self.write("client.ts", "import { send } from './api';\nexport function client() { return missing() + missing(); }\n")
        # A missing member on a known repository class is a definitive V1.
        self.write("class.ts", "class Repo { get(x: string) { return x; } }\nconst r = new Repo();\nr.absent();\nr.absent();\n")
        proc, doc, result = self.report()
        case = self.pairs(doc)["V1", "class.ts"]
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(len(case.findall("failure")), 1)
        self.assertIn("class.ts:3:", case.findtext("failure"))
        self.assertIn("class.ts:4:", case.findtext("failure"))
        findings = [f for f in result["findings"] if f["id"] == "V1" and f["path"] == "class.ts"]
        self.assertEqual(len(findings), 2)
        self.assertEqual(findings[0]["fingerprint"], findings[1]["fingerprint"])

    def test_v2_failure_and_v3_v4_rename(self):
        self.simple()
        self.write("api.ts", "export function send(x: string, required: string) { return x + required; }\n")
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertIsNotNone(self.pairs(doc)["V2", "client.ts"].find("failure"))
        self.write("api.ts", "export function renamed(x: string) { return x; }\n")
        _, doc, _ = self.report()
        for check in ("V3", "V4"):
            self.assertIsNotNone(self.pairs(doc)[check, "client.ts"].find("failure"))

    def test_v3_historical_import_success_and_failure(self):
        self.simple()
        self.write("client.ts", "import { send } from './api';\nexport function client() { return send('ok') + ''; }\n")
        _, doc, _ = self.report()
        self.assertIn(("V3", "client.ts"), self.pairs(doc))
        self.write("api.ts", "export function renamed(x: string) { return x; }\n")
        self.write("client.ts", "import { renamed } from './api';\nexport function client() { return send('ok'); }\n")
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertIsNotNone(self.pairs(doc)["V3", "client.ts"].find("failure"))

    def test_no_checks_for_unsupported_ignored_external_or_unknown_calls(self):
        self.write("api.ts", "export function send(x: string) { return x; }\n")
        self.write(".gitignore", "ignored.ts\n")
        self.commit()
        self.write("README.txt", "not source")
        self.write("ignored.ts", "missing();")
        _, doc, _ = self.report()
        self.assertEqual(doc.get("tests"), "0")
        self.assertIn("No eligible checks completed", doc.findtext("system-out"))
        self.write("new.ts", "import external from 'external-package';\nexternal();\nunknown();\nconsole.log('hello');\n")
        _, doc, _ = self.report()
        self.assertEqual(doc.get("tests"), "0")

    def test_unknown_arity_and_plain_javascript_are_not_v2_passes(self):
        self.simple()
        self.write("spread.ts", "import { send } from './api';\ndeclare const xs: [string];\nsend(...xs);\n")
        self.write("plain.js", "export function f(x) { return x; }\nf();\n")
        _, doc, _ = self.report()
        self.assertNotIn(("V2", "spread.ts"), self.pairs(doc))
        self.assertNotIn(("V2", "plain.js"), self.pairs(doc))

    def test_new_file_missing_history_is_not_a_v3_pass(self):
        self.simple()
        self.write("new.ts", "import { send } from './api';\nsend('ok');\n")
        _, doc, _ = self.report()
        self.assertNotIn(("V3", "new.ts"), self.pairs(doc))
        self.assertIn("V3 new.ts: historical source unavailable", doc.findtext("system-out"))

    def contracts(self):
        self.write("server.ts", "import express from 'express';\nconst app = express();\napp.get('/users', (req, res) => res.json([]));\n")
        self.write("client.ts", "export async function users() { return fetch('/users'); }\n")
        self.commit()

    def test_v5_surviving_removed_moved_and_external_clients(self):
        self.contracts()
        self.write("server.ts", "import express from 'express';\nconst app = express();\napp.get('/users', (req, res) => res.json(['changed']));\n")
        _, doc, _ = self.report()
        self.assertIsNone(self.pairs(doc)["V5", "client.ts"].find("failure"))
        self.write("server.ts", "export const removed = true;\n")
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertIsNotNone(self.pairs(doc)["V5", "client.ts"].find("failure"))
        self.write("moved.ts", "import express from 'express';\nconst app = express();\napp.get('/users', (req, res) => res.json([]));\n")
        _, doc, _ = self.report()
        self.assertIsNone(self.pairs(doc)["V5", "client.ts"].find("failure"))
        self.write("client.ts", "export async function users() { return fetch('https://api.example.com/users'); }\n")
        _, doc, _ = self.report()
        self.assertFalse(any(k[0] == "V5" for k in self.pairs(doc)))

    def test_v5_advisory_and_no_clients(self):
        self.write("server.js", "import express from 'express';\nconst app = express();\napp.get('/users/:id', (req, res) => res.send(req.params.id));\n")
        self.write("client.js", "export function me() { return fetch('/users/me'); }\n")
        self.commit()
        self.write("server.js", "export const removed = true;\n")
        proc, doc, _ = self.report()
        case = self.pairs(doc)["V5", "client.js"]
        self.assertEqual(proc.returncode, 0)
        self.assertIsNone(case.find("failure"))
        self.assertIn("no longer served", case.findtext("system-out"))
        self.write("client.js", "export const noClients = true;\n")
        _, doc, _ = self.report()
        self.assertFalse(any(k[0] == "V5" for k in self.pairs(doc)))
        self.assertIn("index has no contract clients", doc.findtext("system-out"))

    def test_v5_missing_history_has_no_fabricated_pass(self):
        self.write("client.ts", "fetch('/users');\n")
        self.commit()
        self.write("new-server.ts", "import express from 'express';\nconst app = express();\napp.get('/users', (req, res) => res.json([]));\n")
        _, doc, _ = self.report()
        self.assertFalse(any(k[0] == "V5" for k in self.pairs(doc)))
        self.assertIn("V5 new-server.ts: historical source unavailable", doc.findtext("system-out"))

    def test_historical_parse_errors_are_incomplete(self):
        self.simple()
        self.write("client.ts", "import { send } from './api';\nexport function client() { return send('ok'); }\nexport function broken( {\n")
        self.commit()
        self.write("client.ts", "import { send } from './api';\nexport function client() { return send('ok'); }\n")
        _, doc, _ = self.report()
        case = self.pairs(doc)["V3", "client.ts"]
        self.assertIsNotNone(case.find("skipped"))
        self.assertIn("historical source has parse errors", case.find("skipped").get("message"))

    def test_current_target_parse_errors_are_incomplete(self):
        self.simple()
        self.write("api.ts", "export function send(x: string) { return x; }\nexport function broken( {\n")
        _, doc, _ = self.report()
        self.assertIsNotNone(self.pairs(doc)["V2", "client.ts"].find("skipped"))

    def test_v5_historical_parse_errors_are_not_passes(self):
        self.contracts()
        server = (self.root / "server.ts").read_text()
        self.write("server.ts", server + "export function broken( {\n")
        self.commit()
        self.write("server.ts", server)
        _, doc, _ = self.report()
        self.assertNotIn(("V5", "client.ts"), self.pairs(doc))
        self.assertIn("historical source has parse errors", doc.findtext("system-out"))

    def test_standard_tier_unresolved_calls_are_not_fabricated_passes(self):
        self.write("py/util.py", "def helper():\n    return 1\n")
        self.write("py/main.py", "from py.util import helper\n\ndef run():\n    return helper()\n")
        self.commit()
        self.write("py/main.py", "from py.util import helperr\n\ndef run():\n    return helperr()\n")
        proc, doc, result = self.report()
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(result["findings"], [])
        self.assertEqual(doc.get("tests"), "0")

    def test_barrel_unchanged_callers_are_checked(self):
        self.write("api.ts", "export function send(x: string) { return x; }\n")
        self.write("index.ts", "export { send } from './api';\n")
        self.write("client.ts", "import { send } from './index';\nsend('ok');\n")
        self.commit()
        self.write("index.ts", "export { send } from './api'; // unchanged export\n")
        _, doc, _ = self.report()
        self.assertIn(("V3", "client.ts"), self.pairs(doc))
        self.write("api.ts", "export function renamed(x: string) { return x; }\n")
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertIsNotNone(self.pairs(doc)["V3", "client.ts"].find("failure"))
        self.assertIsNotNone(self.pairs(doc)["V4", "index.ts"].find("failure"))

    def test_baseline_accepted_expired_and_no_baseline(self):
        self.simple()
        self.write("api.ts", "export function renamed(x: string) { return x; }\n")
        _, _, result = self.report()
        fingerprints = {f["fingerprint"] for f in result["findings"] if f["blocking"]}
        self.assertTrue(fingerprints)
        self.assertEqual(self.call("baseline").returncode, 0)
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(doc.get("failures"), "0")
        self.assertIn("in the baseline", etree.tostring(doc).decode())
        self.assertEqual(self.report(no_baseline=True)[0].returncode, 1)
        (self.root / ".tessera" / "baseline.json").unlink()
        for fp in fingerprints:
            self.assertEqual(self.call("accept", fp, "--reason", "intended & reviewed", "--expires", "2999-01-01").returncode, 0)
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 0)
        self.assertIn("accepted: intended & reviewed", "".join(doc.itertext()))
        accepted = self.root / ".tessera" / "accepted.toml"
        accepted.write_text(accepted.read_text().replace("2999-01-01", "2000-01-01"))
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertIn("now expired", "".join(doc.itertext()))

    def test_changed_staged_diff_and_deleted_files(self):
        self.simple()
        self.write("api.ts", "export function send(x: string) { return x.trim(); }\n")
        git(self.root, "add", "api.ts")
        for scope in (("--changed",), ("--staged",), ("--diff", "HEAD")):
            self.assertGreater(int(self.report(*scope)[1].get("tests")), 0)
        (self.root / "api.ts").unlink()
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 1)
        self.assertTrue(any(k[1] == "client.ts" for k in self.pairs(doc)))

    def test_empty_index_and_no_change_are_xml(self):
        self.commit()
        proc, doc, _ = self.report()
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(doc.get("tests"), "0")
        self.simple()
        _, doc, _ = self.report()
        self.assertEqual(doc.get("tests"), "0")

    def test_usage_rejected_before_reading_snippet_or_indexing(self):
        for args in (("symbol",), ("--snippet", "missing-file", "--language", "typescript"), ("--snippet", "-", "--language", "typescript")):
            proc = self.call("verify", *args, "--format", "junit")
            self.assertEqual(proc.returncode, 2, proc.stderr.decode())
            self.assertFalse((self.root / ".tessera").exists())
            self.assertEqual(proc.stdout, b"")
        self.simple()
        proc = self.call("--json", "verify", "send", "--format", "junit")
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(set(json.loads(proc.stdout)), {"ok", "findings", "tests"})

    def test_xml_forbidden_characters_in_actual_paths(self):
        if os.name == "nt":
            self.skipTest("Windows prohibits control characters in filenames")
        self.simple()
        path = "control-\x01-\ufffe.ts"
        self.write(path, "import { send } from './api';\nsend('ok');\n")
        self.commit()
        self.write("api.ts", "export function send(x: string) { return x.trim(); }\n")
        proc = self.call("verify", "--changed", "--format", "junit")
        self.assertEqual(proc.returncode, 0, proc.stderr.decode())
        doc = etree.fromstring(proc.stdout)
        SCHEMA.assertValid(doc)
        self.assertIn(("V3", "control-[U+0001]-[U+FFFE].ts"), self.pairs(doc))
        self.assertNotIn(b"\x01", proc.stdout)
        self.assertNotIn("\ufffe", proc.stdout.decode())

    def test_xml_metacharacters_and_attribute_whitespace(self):
        self.simple()
        path = 'odd & <>"\' \t\r\n name.ts'
        self.write(path, "import { send } from './api';\nsend('ok');\n")
        self.commit()
        self.write("api.ts", "export function send(x: string) { return x.trim(); }\n")
        _, doc, _ = self.report()
        self.assertIn(("V3", path), self.pairs(doc))


if __name__ == "__main__":
    unittest.main()
