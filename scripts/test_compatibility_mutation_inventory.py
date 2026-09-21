#!/usr/bin/env python3
"""Tests for the production-source compatibility mutation inventory."""

from __future__ import annotations

import json
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from scripts.rust_source_parser import normalized_token_digest, parse_source

SCRIPT = Path(__file__).with_name("compatibility_mutation_inventory.py")


def evidence_digest(function) -> str:
    base = normalized_token_digest(function)
    return hashlib.sha256((base + "\n" + json.dumps(function.aliases, separators=(",", ":"))).encode()).hexdigest()


def write(root: Path, relative: str, content: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


class CompatibilityMutationInventoryTests(unittest.TestCase):
    def run_script(self, root: Path, *args: str) -> subprocess.CompletedProcess[str]:
        evidence_path = root / "scripts/compatibility_mutation_exception_evidence.json"
        registry_path = root / "backend/src/operations/registry.rs"
        if "--base" not in args and not (root / ".git").exists():
            git(root, "init", "-b", "dev")
            git(root, "config", "user.name", "Compatibility Inventory Test")
            git(root, "config", "user.email", "compatibility-inventory@example.invalid")
            git(root, "add", ".")
            git(root, "commit", "-m", "fixture baseline")
        if evidence_path.is_file() and registry_path.is_file():
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            if not evidence["exceptions"] and "fixture::deferred" in registry_path.read_text(encoding="utf-8"):
                self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(root), *args],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def make_checkout(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / "voidtower"
        write(root, "AGENTS.md", "# VoidTower\n")
        write(root, "backend/src/api/mod.rs", "pub mod fixture;\n")
        write(root, "backend/src/operations/registry.rs", "")
        write(
            root,
            "scripts/compatibility_mutation_exception_evidence.json",
            '{"schema_version":"voidtower.compatibility-mutation-exception-evidence.v1","exceptions":{}}\n',
        )
        return root

    def add_exception_evidence(self, root: Path, source: str, relative: str) -> None:
        path = root / relative
        functions = parse_source(path, path.read_text(encoding="utf-8"), path.stem)
        function = next(item for item in functions if f"{item.module}::{item.name}" == source)
        evidence_path = root / "scripts/compatibility_mutation_exception_evidence.json"
        payload = json.loads(evidence_path.read_text(encoding="utf-8"))
        payload["exceptions"][source] = {"file": relative, "sha256": evidence_digest(function)}
        write(root, "scripts/compatibility_mutation_exception_evidence.json", json.dumps(payload) + "\n")

    def test_unknown_external_callsite_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn dangerous() {
    reqwest::Client::new().post("https://provider.invalid").send().await.unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["unknown"][0]["function"], "dangerous")
        self.assertIn("provider_http_mutation", report["unknown"][0]["marker"])

    def test_rust_mutation_syntax_contract_is_explicit_and_never_silent(self) -> None:
        source = """use std::fs::File;
use std::io::Write;

pub fn qualified(file: &mut File) {
    std::fs::File::set_len(file, 4).unwrap();
}

pub fn ufcs(file: &mut File) {
    <std::fs::File as std::io::Write>::write(file, data).unwrap();
}

pub fn generic_call() {
    std::fs::write::<&[u8]>(path, data).unwrap();
}

pub fn borrowed(mut file: File) {
    let borrowed = &mut file;
    borrowed.write_all(data).unwrap();
}

pub fn function_value() {
    let writer = std::fs::write;
    writer(path, data).unwrap();
}

fn make_file() -> File {
    File::create(path).unwrap()
}

pub fn helper_returned() {
    let file = make_file();
    file.write_all(data).unwrap();
}

pub fn canonical() {
    operation_adoption::submit(state, credential, resource, action, input, headers);
}
"""

        functions = parse_source(Path("syntax-contract.rs"), source, "fixture")
        by_name = {function.name: function for function in functions}
        expected_markers = {
            "qualified": {"unsupported_call_shape"},
            "ufcs": {"unsupported_call_shape"},
            "generic_call": {"unsupported_call_shape"},
            "borrowed": {"filesystem_mutation"},
            "function_value": {"indirect_function_value"},
            "helper_returned": {"filesystem_mutation", "unresolved_receiver_provenance"},
            "canonical": {"canonical"},
        }

        self.assertTrue(set(expected_markers) <= set(by_name))
        for name, markers in expected_markers.items():
            observed = {call.marker for call in by_name[name].calls}
            if name == "canonical":
                observed |= {call.marker for call in by_name[name].canonical_calls}
            self.assertTrue(observed & markers, f"{name} produced no contract marker: {observed}")

    def test_private_and_comment_spoofed_callsites_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """async fn private_dangerous() {
    // operation_adoption::submit is not a delegation.
    reqwest::Client::new().post("https://provider.invalid").send().await.unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["unknown"][0]["function"], "private_dangerous")

    def test_deferred_and_out_of_scope_calls_are_classified_and_mixed_mutation_fails(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn adopted() {
    operation_adoption::submit(state, credential, resource, action, input, headers).await?;
    std::fs::write("/tmp/staged", "fixture").unwrap();
}

pub async fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
    std::fs::write("/tmp/never-reached", "fixture").unwrap();
}

""",
        )
        write(
            root,
            "backend/src/api/webhooks.rs",
            """pub async fn fire_webhooks() {
    reqwest::Client::new().post("https://notification.invalid").send().await.unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        self.add_exception_evidence(root, "webhooks::fire_webhooks", "backend/src/api/webhooks.rs")

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(
            {entry["classification"] for entry in report["classified"]},
            {"deferred_exception", "out_of_scope"},
        )
        self.assertEqual(report["unknown"][0]["function"], "adopted")

    def test_direct_provider_call_is_not_canonicalized_by_later_submit(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn adopted() {
    reqwest::Client::new().post("https://provider.invalid").send().await.unwrap();
    operation_adoption::submit(state, credential, resource, action, input, headers).await?;
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "adopted")

    def test_mutation_before_deferred_error_is_not_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn deferred() {
    std::fs::write("/tmp/side-effect", "fixture").unwrap();
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "deferred")

    def test_raw_strings_cannot_hide_source_after_test_marker(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn raw_text() {
    let _ = r#"contains \" #[cfg(test)] and reqwest::Client::new().post("#;
}

pub async fn dangerous() {
    std::fs::write("/tmp/side-effect", "fixture").unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "dangerous")

    def test_provider_and_filesystem_marker_variants_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn request_variant() {
    client.request(reqwest::Method::POST, url).send().await.unwrap();
}

pub async fn filesystem_variant() {
    fs::remove_file(path).await.unwrap();
}

pub async fn provider_variant() {
    proxy_provider::write_conf(domain, content).unwrap();
}

pub async fn command_alias_variant() {
    Command::new("provider").output().unwrap();
}

pub async fn filesystem_alias_variant() {
    io::remove_file(path).await.unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["unknown"]), 5)

    def test_nonliteral_request_methods_and_ufcs_filesystem_calls_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn request_expression(method: reqwest::Method) {
    client.request(if enabled { reqwest::Method::GET } else { method }, url).send().await.unwrap();
}

pub fn ufcs_mutations(file: std::fs::File, builder: std::fs::DirBuilder, options: std::fs::OpenOptions) {
    std::fs::File::set_permissions(&file, perms).unwrap();
    std::fs::DirBuilder::create(&builder, path).unwrap();
    std::fs::OpenOptions::open(&options, path).unwrap();
}

pub fn unsupported_call_shapes() {
    std::fs::write::<_, _>(path, data).unwrap();
    (std::fs::write)(path, data).unwrap();
    let writer = std::fs::write;
    writer(path, data).unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["unknown"]), 7)

    def test_cfg_test_modules_do_not_hide_later_production_code(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """#[cfg(test)]
mod tests {
    fn hidden() { std::fs::write(path, data).unwrap(); }
}

pub async fn dangerous() {
    std::fs::write("/tmp/side-effect", "fixture").unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual([entry["function"] for entry in report["unknown"]], ["dangerous"])

    def test_nested_module_does_not_inherit_root_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/nested/mods.rs",
            """async fn run_checked() {
    std::process::Command::new("provider").output().unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "run_checked")

    def test_conditional_deferred_error_is_not_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub async fn deferred(flag: bool) {
    if flag {
        return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
    }
    std::fs::write("/tmp/side-effect", "fixture").unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "deferred")

    def test_source_symlinks_fail_closed(self) -> None:
        root = self.make_checkout()
        outside = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(outside))
        write(outside, "evil.rs", "pub fn dangerous() { std::fs::write(path, data).unwrap(); }\n")
        (root / "backend/src/api/linked").symlink_to(outside, target_is_directory=True)

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 2)
        self.assertIn("symlink", result.stderr)

    def test_api_root_symlink_fails_closed(self) -> None:
        root = self.make_checkout()
        api_root = root / "backend/src/api"
        outside = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(outside))
        moved = outside / "api"
        api_root.rename(moved)
        api_root.symlink_to(moved, target_is_directory=True)
        self.addCleanup(lambda: (api_root.unlink(), moved.rename(api_root)))

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 2)
        self.assertIn("source directory is a symlink", result.stderr)

    def test_output_is_sorted_and_does_not_include_source_contents(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/z_fixture.rs",
            """pub async fn zed() {
    std::fs::write("/tmp/not-a-secret", "fixture-secret").unwrap();
}
""",
        )
        write(
            root,
            "backend/src/api/a_fixture.rs",
            """pub async fn alpha() {
    std::fs::write("/tmp/not-a-secret", "fixture-secret").unwrap();
}
""",
        )

        result = self.run_script(root)

        self.assertEqual(result.returncode, 0)
        report = json.loads(result.stdout)
        self.assertEqual(
            [entry["function"] for entry in report["unknown"]],
            ["alpha", "zed"],
        )
        self.assertNotIn("fixture-secret", result.stdout)
        self.assertNotIn("std::fs::write", result.stdout)
    def test_import_aliases_openoptions_and_directory_calls_are_detected(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use reqwest::Client as HttpClient;
use std::{fs::write as grouped_write};
use std::fs::*;
use std::fs::{self, OpenOptions};
use std::process::Command as Spawn;

pub fn aliased() {
    fs::create_dir_all(path).unwrap();
    OpenOptions::new().create_new(true).open(path).unwrap();
    Spawn::new("provider").output().unwrap();
    HttpClient::new().post(url).send().unwrap();
    grouped_write(path, data).unwrap();
    write(path, data).unwrap();
    HttpClient::new().request(reqwest::Method::GET, url).send().unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["unknown"]), 7)
        self.assertEqual({entry["marker"] for entry in report["unknown"]}, {"filesystem_mutation", "process_execution", "provider_http_mutation"})

    def test_canonical_text_spoof_does_not_classify_direct_mutation(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn spoofed() {
    let _text = "operation_adoption::submit(state, credential, resource, action, input, headers)";
    std::fs::write(path, data).unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "spoofed")

    def test_inline_module_impl_identity_and_cfg_item_scope_are_exact(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """struct Thing;
mod nested {
    #[cfg(test)]
    fn hidden() { std::process::Command::new("hidden"); }

    impl Thing {
        fn method() { std::process::Command::new("provider"); }
    }
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual([entry["function"] for entry in report["unknown"]], ["method"])

    def test_malformed_cfg_attribute_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", "#[cfg(test)]\nlet not_an_item = 1;\n")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 2)
        self.assertIn("syntax", result.stderr.lower())

    def test_nested_comments_do_not_create_mutation_findings(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn commented() {
    /* outer /* inner std::fs::write(path, data); */ still comment */
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["unknown"], [])

    def test_unlisted_registry_exception_requires_evidence(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn other() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::other", route: "/other", },\n',
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["reason"], "missing immutable body evidence")

    def test_leading_paths_and_raw_identifier_aliases_are_detected(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::write as r#write;

pub fn leading() {
    ::std::fs::write(path, data).unwrap();
}

pub fn raw_alias() {
    r#write(path, data).unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual([item["function"] for item in json.loads(result.stdout)["unknown"]], ["leading", "raw_alias"])

    def test_unknown_macro_expansion_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """macro_rules! hidden_mutation { () => { std::fs::write(path, data); } }

pub fn invokes_hidden_mutation() {
    hidden_mutation!();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 2)
        self.assertIn("unsupported executable item-level mutation", result.stderr)

    def test_filesystem_api_variants_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn variants() {
    std::fs::set_permissions(path, permissions).unwrap();
    std::fs::File::options().write(true).open(path).unwrap();
    std::fs::DirBuilder::new().create(path).unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(json.loads(result.stdout)["unknown"]), 5)

    def test_typed_filesystem_builders_and_permissions_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;

pub fn typed_builders() {
    let b: std::fs::OpenOptions = std::fs::OpenOptions::new();
    b.write(true).open(path).unwrap();
    let f: std::fs::File = std::fs::File::open(path).unwrap();
    f.set_permissions(perms).unwrap();
    let options = OpenOptions::new();
    options.mode(0o600).open(path).unwrap();
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(json.loads(result.stdout)["unknown"]), 3)

    def test_markerless_registry_exception_still_requires_evidence(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn markerless() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::markerless", route: "/markerless", },\n',
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "exception_evidence")

    def test_registry_comments_cannot_register_an_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn fake() {
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            '// DeferredMutationException { source: "fixture::fake", route: "/fake" }\n',
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["function"], "fake")

    def test_unsupported_macro_is_not_hidden_by_an_evidence_bound_identity(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod proxy;\n")
        write(
            root,
            "backend/src/api/proxy.rs",
            """pub fn write_nginx_conf() {
    hidden!();
}
""",
        )
        self.add_exception_evidence(root, "proxy::write_nginx_conf", "backend/src/api/proxy.rs")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_macro")

    def test_route_registration_is_explicitly_classified(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/mod.rs",
            """pub fn routes() {
    client.route(path, handler).post(handler);
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "http_route_registration")

        write(
            root,
            "backend/src/api/mod.rs",
            """pub fn router() {
    client.route(path, handler).post(handler);
}
""",
        )
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["classified"][0]["classification"], "route_registration")

    def test_evidence_symlink_parent_and_stale_entry_fail_closed(self) -> None:
        root = self.make_checkout()
        outside = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(outside))
        scripts = root / "scripts"
        shutil.rmtree(scripts)
        scripts.symlink_to(outside, target_is_directory=True)
        write(outside, "compatibility_mutation_exception_evidence.json", '{"schema_version":"voidtower.compatibility-mutation-exception-evidence.v1","exceptions":{}}\n')
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 2)
        self.assertIn("escapes checkout", result.stderr)

    def test_changed_exception_body_invalidates_immutable_evidence(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        path = root / "backend/src/api/fixture.rs"
        path.write_text(path.read_text(encoding="utf-8").replace("canonical operation adapter", "changed body"), encoding="utf-8")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["reason"], "immutable body evidence does not match the function")

    def test_changed_exception_body_and_digest_fail_against_approved_git_base(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        git(root, "init", "-b", "dev")
        git(root, "config", "user.name", "Compatibility Inventory Test")
        git(root, "config", "user.email", "compatibility-inventory@example.invalid")
        git(root, "add", ".")
        git(root, "commit", "-m", "approved compatibility baseline")

        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("changed body".into()));
}
""",
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check", "--base", git(root, "rev-parse", "HEAD"))

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["reason"], "exception body differs from approved base")

    def test_check_requires_a_git_approval_base(self) -> None:
        root = self.make_checkout()
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(root), "--check"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        self.assertEqual(result.returncode, 2)
        self.assertIn("requires a Git base", result.stderr)

    def test_new_exception_body_and_digest_have_no_approved_git_base(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        git(root, "init", "-b", "dev")
        git(root, "config", "user.name", "Compatibility Inventory Test")
        git(root, "config", "user.email", "compatibility-inventory@example.invalid")
        git(root, "add", ".")
        git(root, "commit", "-m", "approved compatibility baseline")

        with (root / "backend/src/api/fixture.rs").open("a", encoding="utf-8") as handle:
            handle.write(
                """
pub fn new_deferred() {
    return Err(AppError::FeatureUnavailable("new canonical operation adapter".into()));
}
"""
            )
        write(
            root,
            "backend/src/operations/registry.rs",
            """DeferredMutationException { source: "fixture::deferred", route: "/fixture", }
DeferredMutationException { source: "fixture::new_deferred", route: "/new-fixture", }
""",
        )
        self.add_exception_evidence(root, "fixture::new_deferred", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check", "--base", git(root, "rev-parse", "HEAD"))

        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("exception registry differs from approved base", result.stderr)

    def test_new_registry_entry_for_an_existing_function_requires_base_approval(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}

pub fn existing_unregistered() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        git(root, "init", "-b", "dev")
        git(root, "config", "user.name", "Compatibility Inventory Test")
        git(root, "config", "user.email", "compatibility-inventory@example.invalid")
        git(root, "add", ".")
        git(root, "commit", "-m", "approved compatibility baseline")

        write(
            root,
            "backend/src/operations/registry.rs",
            """DeferredMutationException { source: "fixture::deferred", route: "/fixture", }
DeferredMutationException { source: "fixture::existing_unregistered", route: "/existing", }
""",
        )
        self.add_exception_evidence(root, "fixture::existing_unregistered", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check", "--base", git(root, "rev-parse", "HEAD"))

        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("exception registry differs from approved base", result.stderr)

    def test_symbolic_approval_base_is_rejected(self) -> None:
        root = self.make_checkout()
        git(root, "init", "-b", "dev")
        git(root, "config", "user.name", "Compatibility Inventory Test")
        git(root, "config", "user.email", "compatibility-inventory@example.invalid")
        git(root, "add", ".")
        git(root, "commit", "-m", "approved compatibility baseline")

        result = self.run_script(root, "--check", "--base", "HEAD")

        self.assertEqual(result.returncode, 2)
        self.assertIn("must be a full commit ID", result.stderr)

    def test_registry_record_changes_require_base_approval(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    return Err(AppError::FeatureUnavailable("canonical operation adapter".into()));
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")
        git(root, "init", "-b", "dev")
        git(root, "config", "user.name", "Compatibility Inventory Test")
        git(root, "config", "user.email", "compatibility-inventory@example.invalid")
        git(root, "add", ".")
        git(root, "commit", "-m", "approved compatibility baseline")

        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/changed", },\n',
        )
        result = self.run_script(root, "--check", "--base", git(root, "rev-parse", "HEAD"))

        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("exception registry differs from approved base", result.stderr)

    def test_recursive_aliases_and_function_values_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::write as first;
use first as second;

pub fn recursive_alias() {
    second(path, data).unwrap();
}

pub fn function_value() {
    let writer = std::fs::write;
    writer(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(
            [entry["function"] for entry in report["unknown"]],
            ["recursive_alias", "function_value"],
        )
        self.assertEqual(
            {entry["marker"] for entry in report["unknown"]},
            {"filesystem_mutation", "indirect_function_value"},
        )

    def test_request_builder_function_values_and_receiver_aliases_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn request_values(client: reqwest::Client, builder: reqwest::RequestBuilder) {
    let post = client.post;
    post(url).send().unwrap();
    let forwarded = builder;
    forwarded.send().unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "request_values")
        self.assertIn(
            report["unknown"][0]["marker"],
            {"indirect_function_value", "unresolved_receiver_provenance"},
        )

    def test_returned_receiver_provenance_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """fn returned_file() -> std::fs::File {
    std::fs::File::create(path).unwrap()
}

pub fn use_returned_file() {
    let file = returned_file();
    file.write_all(data).unwrap();
}

fn returned_builder(client: reqwest::Client) -> reqwest::RequestBuilder {
    client.get(url)
}

pub fn use_returned_builder(client: reqwest::Client) {
    let builder = returned_builder(client);
    builder.send().unwrap();
}

pub fn use_parenthesized_file() {
    let file = (returned_file)();
    file.write_all(data).unwrap();
}

pub fn use_destructured_builder(client: reqwest::Client) {
    let (builder,) = (returned_builder(client),);
    builder.send().unwrap();
}

mod helper {
    pub fn make_file() -> std::fs::File {
        std::fs::File::create(path).unwrap()
    }
}

fn generic_file<T>() -> std::fs::File {
    std::fs::File::create(path).unwrap()
}

pub struct Factory;

impl Factory {
    fn make_file(&self) -> std::fs::File {
        std::fs::File::create(path).unwrap()
    }

    pub fn use_qualified_file(&self) {
        let module_file = helper::make_file();
        module_file.write_all(data).unwrap();
        let method_file = self.make_file();
        method_file.write_all(data).unwrap();
        let turbofish_file = self.make_file::<u8>();
        turbofish_file.write_all(data).unwrap();
    }

    pub fn use_generic_file(&self) {
        let file = generic_file::<u8>();
        file.write_all(data).unwrap();
    }
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        expected = {
            "use_returned_file",
            "use_returned_builder",
            "use_parenthesized_file",
            "use_destructured_builder",
            "use_qualified_file",
            "use_generic_file",
        }
        actual = {entry["function"] for entry in report["unknown"]}
        self.assertTrue(expected.issubset(actual), sorted(actual))

    def test_complex_receiver_provenance_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn conditional_receiver(cond: bool) {
    let file = if cond { std::fs::File::open(path).unwrap() } else { std::fs::File::open(path).unwrap() };
    file.set_len(1).unwrap();
}

pub fn parenthesized_receiver() {
    let file = (std::fs::File::open(path).unwrap());
    file.set_len(1).unwrap();
}

pub fn block_receiver() {
    let file = { std::fs::File::open(path).unwrap() };
    file.set_len(1).unwrap();
}

pub fn match_receiver(value: bool) {
    let file = match value {
        true => std::fs::File::open(path).unwrap(),
        false => std::fs::File::open(path).unwrap(),
    };
    file.set_len(1).unwrap();
}

pub fn closure_receiver() {
    let file = (|| std::fs::File::open(path).unwrap())().unwrap();
    file.set_len(1).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        findings = [entry for entry in report["unknown"] if entry["marker"] == "unresolved_receiver_provenance"]
        self.assertEqual(
            {entry["function"] for entry in findings},
            {
                "conditional_receiver",
                "parenthesized_receiver",
                "block_receiver",
                "match_receiver",
                "closure_receiver",
            },
        )

    def test_typed_provider_process_and_vectored_filesystem_receivers_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use reqwest::Client;
use std::fs::File;
use std::io::Write;
use std::process::Command;

pub fn execute_request(client: Client, request: reqwest::Request) {
    client.execute(request).unwrap();
}

pub fn execute_command(mut command: Command) {
    command.output().unwrap();
    command.status().unwrap();
    command.spawn().unwrap();
}

pub fn vectored_file(mut file: File) {
    file.write_vectored(&[]).unwrap();
    file.write_all_vectored(&[]).unwrap();
}

fn make_file() -> std::fs::File {
    std::fs::File::create(path).unwrap()
}

pub fn direct_file_chain() {
    make_file().write_all(data).unwrap();
}

fn make_builder(client: reqwest::Client) -> reqwest::RequestBuilder {
    client.get(url)
}

pub fn direct_builder_chain(client: reqwest::Client) {
    make_builder(client).send().unwrap();
}

""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        by_function = {entry["function"]: entry["marker"] for entry in report["unknown"]}
        self.assertEqual(by_function["execute_request"], "provider_http_mutation")
        self.assertEqual(by_function["execute_command"], "process_execution")
        self.assertEqual(by_function["vectored_file"], "filesystem_mutation")
        self.assertEqual(by_function["direct_file_chain"], "unresolved_receiver_provenance")
        self.assertEqual(by_function["direct_builder_chain"], "provider_http_mutation")


    def test_item_level_executable_mutation_alias_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::write as writer;
const CALLBACK: fn(&str, &[u8]) -> std::io::Result<()> = writer;
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 2)
        self.assertIn("item-level", result.stderr)

    def test_unicode_identifiers_and_typed_function_values_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::write as ш;

pub fn опасная() {
    let writer: fn(&str, &[u8]) -> std::io::Result<()> = ш;
    writer(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "опасная")
        self.assertEqual(report["unknown"][0]["marker"], "indirect_function_value")

    def test_type_alias_and_raw_request_methods_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """type Builder = reqwest::RequestBuilder;

pub fn request_alias(builder: Builder) {
    builder.r#send().unwrap();
}

pub fn request_method(client: reqwest::Client) {
    client.r#post(url).send().unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(
            {entry["function"] for entry in report["unknown"]},
            {"request_alias", "request_method"},
        )
        self.assertTrue(all(entry["marker"] == "provider_http_mutation" for entry in report["unknown"]))

    def test_filesystem_receiver_alias_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn builder_alias() {
    let options = std::fs::OpenOptions::new();
    let alias = options;
    alias.truncate(true).open(path).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "builder_alias")
        self.assertEqual(report["unknown"][0]["marker"], "filesystem_mutation")

    def test_mutating_reexports_fail_closed_before_classification(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub use crate::provider::write as exported;

pub fn invokes_reexport() {
    exported(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 2)
        self.assertIn("re-export", result.stderr)

    def test_ufcs_mutation_call_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn ufcs(file: &mut std::fs::File) {
    <std::fs::File as std::io::Write>::write(file, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "unsupported_call_shape")

    def test_suffix_shaped_prepare_helper_cannot_authorize_direct_mutation(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/proxmox.rs",
            """pub async fn upload_storage_content() {
    std::fs::write(path, data).unwrap();
    prepare_or_submit().await;
}

async fn prepare_or_submit() {}
""",
        )
        write(
            root,
            "scripts/compatibility_mutation_exception_evidence.json",
            '{"schema_version":"voidtower.compatibility-mutation-exception-evidence.v1","exceptions":{}}\n',
        )
        self.add_exception_evidence(root, "proxmox::upload_storage_content", "backend/src/api/proxmox.rs")

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "upload_storage_content")

    def test_closure_scoped_feature_unavailable_cannot_authorize_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    let reject = || return Err::<(), AppError>(AppError::FeatureUnavailable("not yet".into()));
    let _ = reject();
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "deferred")

    def test_async_block_feature_unavailable_cannot_authorize_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    let reject = async { return Err::<(), AppError>(AppError::FeatureUnavailable("not yet".into())); };
    let _ = reject;
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        write(
            root,
            "scripts/compatibility_mutation_exception_evidence.json",
            '{"schema_version":"voidtower.compatibility-mutation-exception-evidence.v1","exceptions":{}}\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["function"], "deferred")

    def test_private_imported_mutation_alias_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::provider::write as exported;

pub fn invokes_import() {
    exported(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "unsupported_call_shape")

    def test_qualified_associated_mutation_call_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn qualified(file: &mut std::fs::File) {
    std::io::Write::write(file, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "unsupported_call_shape")

    def test_same_named_helpers_in_different_modules_do_not_share_canonical_proof(self) -> None:
        source = """mod trusted {
    fn prepare_or_submit() { operation_adoption::submit(state, credential, resource, action, input, headers); }
    fn handler() { std::fs::write(path, data).unwrap(); prepare_or_submit(); }
}
mod untrusted {
    fn prepare_or_submit() {}
    fn handler() { std::fs::write(path, data).unwrap(); prepare_or_submit(); }
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")
        by_identity = {(item.module, item.name): item for item in functions}

        self.assertTrue(by_identity[("proxmox::trusted", "handler")].canonical_calls)
        self.assertFalse(by_identity[("proxmox::untrusted", "handler")].canonical_calls)

    def test_module_qualified_private_mutation_alias_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::provider as private_provider;

pub fn invokes_module_alias() {
    private_provider::write(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_call_shape")

    def test_unknown_wildcard_import_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::provider::*;

pub fn invokes_wildcard() {
    write(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 2)
        self.assertIn("unsupported wildcard use declaration", result.stderr)

    def test_qualified_filesystem_mutators_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn qualified(file: &mut std::fs::File) {
    std::fs::File::set_len(file, 4).unwrap();
    std::fs::File::sync_all(file).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_call_shape")

    def test_local_helper_shadowing_cannot_create_canonical_proof(self) -> None:
        source = """fn prepare_or_submit() { operation_adoption::submit(state, credential, resource, action, input, headers); }
fn handler() {
    let prepare_or_submit = || {};
    std::fs::write(path, data).unwrap();
    prepare_or_submit();
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")
        handler = next(item for item in functions if item.name == "handler")

        self.assertFalse(handler.canonical_calls)

    def test_trusted_alias_name_cannot_hide_an_untrusted_target(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::evil as proxy;
use crate::evil::danger as do_it;
use crate::evil as write;

pub fn invokes_spoofed_alias() {
    proxy::write(path, data).unwrap();
}

pub fn invokes_bare_spoofed_alias() {
    do_it(path);
    write(path, data);
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_call_shape")

    def test_broad_operations_provenance_cannot_authorize_unknown_mutation_alias(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::operations::evil::apply as run;

pub fn invokes_untrusted_alias() {
    run(state);
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["unknown"][0]["marker"], "unsupported_call_shape")

    def test_imported_canonical_binding_shadowed_by_parameter_or_local_is_not_proof(self) -> None:
        source = """use super::operation_adoption::submit;

fn parameter_handler(submit: fn()) {
    std::fs::write(path, data).unwrap();
    submit();
}

fn local_handler() {
    let submit = || {};
    std::fs::write(path, data).unwrap();
    submit();
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")

        self.assertFalse(next(item for item in functions if item.name == "parameter_handler").canonical_calls)
        self.assertFalse(next(item for item in functions if item.name == "local_handler").canonical_calls)

    def test_nested_canonical_call_does_not_authorize_enclosing_mutation(self) -> None:
        source = """fn outer() {
    fn nested() {
        operation_adoption::submit(state, credential, resource, action, input, headers);
    }
    std::fs::write(path, data).unwrap();
    nested();
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")
        outer = next(item for item in functions if item.name == "outer")
        nested = next(item for item in functions if item.name == "nested")

        self.assertFalse(outer.canonical_calls)
        self.assertTrue(nested.canonical_calls)

    def test_parameter_and_mutable_local_shadowing_cannot_create_canonical_proof(self) -> None:
        source = """fn prepare_or_submit() { operation_adoption::submit(state, credential, resource, action, input, headers); }
fn parameter_handler(prepare_or_submit: fn()) {
    std::fs::write(path, data).unwrap();
    prepare_or_submit();
}
fn mutable_handler() {
    let mut prepare_or_submit = || {};
    std::fs::write(path, data).unwrap();
    prepare_or_submit();
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")

        self.assertFalse(next(item for item in functions if item.name == "parameter_handler").canonical_calls)
        self.assertFalse(next(item for item in functions if item.name == "mutable_handler").canonical_calls)

    def test_local_canonical_module_and_nested_exception_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """mod operation_adoption {
    pub fn submit() {}
}

pub fn spoofed() {
    operation_adoption::submit();
    std::fs::write(path, data).unwrap();
}

pub fn nested_exception() {
    fn nested() {
        return Err::<(), AppError>(AppError::FeatureUnavailable("not yet".into()));
    }
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::nested_exception", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::nested_exception", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertIn("unsupported_call_shape", {item["marker"] for item in report["unknown"]})

    def test_external_cfg_test_module_is_not_inventory_production(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod nested;\n")
        write(root, "backend/src/api/nested/mod.rs", "#[cfg(test)]\nmod tests;\n")
        write(
            root,
            "backend/src/api/nested/tests.rs",
            """pub fn hidden() {
    std::fs::write(path, data).unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["unknown"], [])

    def test_broad_cmdb_provenance_cannot_authorize_unknown_mutation_alias(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::cmdb::evil::apply as run;

pub fn invokes_untrusted_alias() {
    run(state);
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_call_shape")

    def test_file_receiver_mutators_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn mutates_file(file: std::fs::File) {
    file.write_all(data).unwrap();
    file.flush().unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "filesystem_mutation")

    def test_pattern_shadowing_cannot_create_canonical_proof(self) -> None:
        source = """use super::operation_adoption::submit;

fn destructured() {
    let (submit,) = (|| {},);
    std::fs::write(path, data).unwrap();
    submit();
}

fn conditional() {
    if let Some(submit) = option {
        std::fs::write(path, data).unwrap();
        submit();
    }
}

fn looped() {
    for (submit,) in values {
        std::fs::write(path, data).unwrap();
        submit();
    }
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")

        self.assertFalse(any(item.canonical_calls for item in functions))

    def test_closure_canonical_call_does_not_authorize_enclosing_mutation(self) -> None:
        source = """fn outer() {
    let closure = || {
        operation_adoption::submit(state, credential, resource, action, input, headers);
    };
    std::fs::write(path, data).unwrap();
}
"""
        functions = parse_source(Path("fixture.rs"), source, "proxmox")
        outer = next(item for item in functions if item.name == "outer")

        self.assertFalse(outer.canonical_calls)

    def test_cfg_test_external_module_from_file_module_is_not_inventory_production(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod fixture;\n")
        write(root, "backend/src/api/fixture.rs", "#[cfg(test)]\nmod hidden;\n")
        write(
            root,
            "backend/src/api/fixture/hidden.rs",
            """pub fn hidden() {
    std::process::Command::new("hidden");
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["unknown"], [])

    def test_single_parameter_closure_feature_error_cannot_authorize_exception(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """pub fn deferred() {
    let closure = |value: u8| {
        return Err::<(), AppError>(AppError::FeatureUnavailable(value.to_string()));
    };
    std::fs::write(path, data).unwrap();
}
""",
        )
        write(
            root,
            "backend/src/operations/registry.rs",
            'DeferredMutationException { source: "fixture::deferred", route: "/fixture", },\n',
        )
        self.add_exception_evidence(root, "fixture::deferred", "backend/src/api/fixture.rs")

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["function"], "deferred")

    def test_imported_and_type_aliased_file_receivers_fail_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use std::fs::File as ImportedFile;
type AliasedFile = ImportedFile;

pub fn mutates(file: AliasedFile) {
    file.write_all(data).unwrap();
    file.flush().unwrap();
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(json.loads(result.stdout)["unknown"]), 2)

    def test_canonical_shaped_untrusted_alias_fails_closed(self) -> None:
        root = self.make_checkout()
        write(
            root,
            "backend/src/api/fixture.rs",
            """use crate::support::submit as submit;

pub fn spoofed() {
    submit(state);
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["unknown"][0]["marker"], "unsupported_call_shape")

    def test_cfg_test_path_attribute_and_pub_crate_module_are_not_production(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod fixture;\n")
        write(root, "backend/src/api/fixture.rs", "#[path = \"hidden.rs\"]\n#[cfg(test)]\npub(crate) mod hidden;\n")
        write(
            root,
            "backend/src/api/fixture/hidden.rs",
            """pub fn hidden() {
    std::process::Command::new("hidden");
}
""",
        )

        result = self.run_script(root, "--check")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["unknown"], [])

    def test_match_braced_pattern_shadowing_cannot_create_canonical_proof(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use crate::operation_adoption::submit;

pub fn dangerous(state: State) {
    match state {
        State { submit } => submit(),
        _ => (),
    }
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_typed_return_closure_cannot_authorize_enclosing_mutation(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """pub fn dangerous() {
    let closure = |value: u8| -> Result<(), Error> {
        operation_adoption::submit(value);
        Ok(())
    };
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_reference_type_alias_file_receiver_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use std::fs::File as FileAlias;
type FileRef<'a> = &'a mut FileAlias;

pub fn dangerous(file: FileRef<'_>) {
    file.write_all(data).unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filesystem_mutation", result.stdout)

    def test_cfg_test_path_shared_with_production_is_not_discarded(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", """pub mod fixture;
#[cfg(test)]
#[path = "fixture.rs"]
mod fixture_test;
""")
        write(root, "backend/src/api/fixture.rs", """pub fn dangerous() {
    reqwest::Client::new().post("https://provider.invalid").send().await.unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_imported_request_builder_receiver_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use reqwest::RequestBuilder as Builder;

pub async fn dangerous(builder: Builder) {
    builder.send().await.unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("provider_http_mutation", result.stdout)

    def test_chained_temporary_file_receiver_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """pub fn dangerous(path: Path) {
    std::fs::File::open(path).unwrap().set_len(1).unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filesystem_mutation", result.stdout)

    def test_trusted_helper_shadowed_by_match_pattern_cannot_prove_canonical(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """fn helper() {
    operation_adoption::submit(state);
}

pub fn dangerous(state: State) {
    match state {
        State { helper } => helper(),
        _ => helper(),
    }
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_qualified_mutation_paths_fail_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """pub fn dangerous() {
    crate::provider::write(state);
    crate::provider::apply(state);
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["unknown"]), 2)

    def test_local_external_module_cannot_spoof_canonical_adapter(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """mod operation_adoption;

pub fn dangerous() {
    operation_adoption::submit(state);
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_extern_crate_alias_cannot_spoof_canonical_adapter(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """extern crate fake_adapter as operation_adoption;

pub fn dangerous() {
    operation_adoption::submit(state);
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_destructured_function_parameter_cannot_create_canonical_proof(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use crate::operation_adoption::submit;

pub fn dangerous((submit,): (FnType,)) {
    submit();
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_parent_external_module_cannot_spoof_canonical_adapter(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod parent;\n")
        write(root, "backend/src/api/parent.rs", "mod operation_adoption;\npub mod child;\n")
        write(root, "backend/src/api/parent/child.rs", """pub fn dangerous() {
    operation_adoption::submit(state);
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_raw_path_attribute_target_is_scanned(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "#[path = r\"other.rs\"]\npub mod fixture;\n")
        write(root, "backend/src/api/other.rs", """pub fn dangerous() {
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_ancestor_use_alias_cannot_spoof_canonical_adapter(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "pub mod parent;\n")
        write(root, "backend/src/api/parent.rs", "use crate::evil::operation_adoption;\npub mod child;\n")
        write(root, "backend/src/api/parent/child.rs", """pub fn dangerous() {
    super::operation_adoption::submit(state);
    std::fs::write("/tmp/staged", "fixture").unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("dangerous", result.stdout)

    def test_filesystem_receiver_assignment_propagates(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use std::fs::File;

pub fn dangerous(file: File) {
    let forwarded;
    forwarded = file;
    forwarded.write_all(data).unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filesystem_mutation", result.stdout)

    def test_reference_filesystem_receiver_assignment_propagates(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use std::fs::File;

pub fn dangerous(mut file: File) {
    let forwarded = &mut file;
    forwarded.write_all(data).unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filesystem_mutation", result.stdout)

    def test_parenthesized_reference_filesystem_receiver_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/fixture.rs", """use std::fs::File;
use std::io::Write;

pub fn dangerous(mut file: File) {
    (&mut file).write_all(&[]).unwrap();
}
""")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filesystem_mutation", result.stdout)

    def test_unsafe_external_module_path_fails_closed(self) -> None:
        root = self.make_checkout()
        write(root, "backend/src/api/mod.rs", "#[path = \"../../outside.rs\"]\npub mod hidden;\n")
        write(root, "backend/outside.rs", "pub fn hidden() {}\n")
        result = self.run_script(root, "--check")
        self.assertEqual(result.returncode, 2)
        self.assertIn("unsupported external module path", result.stderr)


if __name__ == "__main__":
    unittest.main()
