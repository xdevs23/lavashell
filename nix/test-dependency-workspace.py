#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import copy
import importlib.util
import json
import tempfile
import tomllib
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


projection = module("dependency_workspace", "dependency-workspace.py")
identities = module("cargo_plan", "cargo-plan.py")
refresh = module("refresh_cargo_projections", "refresh-cargo-projections.py")
boundaries = module("dependency_boundaries", "test-dependency-boundaries.py")
ROOT = "/fixture"


def package(name, local=False, dependencies=None, version="1.0.0"):
    source = None if local else projection.CRATES_IO
    identity = f"path+file://{ROOT}/{name}#{name}@{version}" if local else f"{source}#{name}@{version}"
    return {"id": identity, "name": name, "version": version, "edition": "2024", "source": source,
            "manifest_path": f"{ROOT}/{name}/Cargo.toml", "dependencies": dependencies or [], "links": None}


def declaration(name, rename=None, kind=None, target=None, source=projection.CRATES_IO):
    return {"name": name, "rename": rename, "kind": kind, "target": target, "source": source}


def unit(package_, dependencies=None, features=None, host=False, kind="lib", mode="build"):
    return {"pkg_id": package_["id"], "features": features or [], "dependencies": dependencies or [],
            "mode": mode, "platform": None if host else "x86_64-unknown-linux-gnu",
            "profile": {"name": "packaged", "opt_level": "0" if host else "2"},
            "target": {"name": package_["name"].replace("-", "_"), "kind": [kind],
                       "crate_types": ["proc-macro" if kind == "proc-macro" else "lib"]}}


def edge(index, name):
    return {"index": index, "extern_crate_name": name.replace("-", "_"),
            "noprelude": False, "nounused": False, "public": False}


def fixture():
    app = package("app", True, [declaration("dep")])
    dep = package("dep")
    graph = {"roots": [0], "units": [unit(app, [edge(1, "dep")]), unit(dep, features=["std", "default"])]}
    metadata = {"workspace_root": ROOT, "packages": [app, dep]}
    seed = {"workspace_manifest": {"workspace": {"resolver": "3", "members": ["app"]},
                                   "profile": {"packaged": {"inherits": "dev", "opt-level": 2}}}}
    selection = {"target": "x86_64-unknown-linux-gnu", "profile": "packaged", "packages": ["app"], "features": []}
    return seed, graph, metadata, selection


class DependencyWorkspace(unittest.TestCase):
    def test_metadata_request_uses_the_selected_target_and_retains_frozen_dependencies(self):
        seed, graph, metadata, selection = fixture()
        app_manifest = projection.manifest_text({"package": {"name": "app"}})
        data = {"manifest": projection.manifest_text(seed["workspace_manifest"]), "seedHash": "fixture",
                "crates": [{"manifest": app_manifest}]}
        lock = {"version": 4, "package": [{"name": "app", "version": "0.0.0"}]}
        args = SimpleNamespace(cargo="synthetic-cargo", rustc="synthetic-rustc", nixpkgs="unused",
                               target=selection["target"])
        result = SimpleNamespace(returncode=0, stdout=json.dumps(metadata))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "app").mkdir()
            (root / "app/Cargo.toml").write_text(app_manifest)
            with patch.object(refresh, "workspace", return_value=data), \
                    patch.object(refresh, "graph", return_value=(graph, lock)), \
                    patch.object(refresh.subprocess, "run", return_value=result) as run:
                refresh.dependency_plan(args, root, "app", False, root / "result")
            command = run.call_args.args[0]
            self.assertEqual(command[:2], [args.cargo, "metadata"])
            self.assertEqual(command[command.index("--filter-platform") + 1], args.target)
            self.assertIn("--offline", command)
            self.assertIn("--frozen", command)
            self.assertNotIn("--no-deps", command)

    def test_native_closure_fixture_changes_lavashell_manifest_and_lock_edge_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            members = {"crates/lavashell": "lavashell", "examples/engine-app": "engine-app"}
            (root / "Cargo.toml").write_text(projection.manifest_text({"workspace": {"members": list(members)}}))
            lock = {"version": 4, "package": []}
            for member, name in members.items():
                directory = root / member
                directory.mkdir(parents=True)
                (directory / "Cargo.toml").write_text(projection.manifest_text({
                    "package": {"name": name, "version": "0.1.0"},
                    "target": {'cfg(target_os = "android")': {"dependencies": {}}},
                }))
                lock["package"].append({"name": name, "version": "0.1.0", "dependencies": []})
            (root / "Cargo.lock").write_text(projection.lock_text(lock))
            application_manifest = (root / "examples/engine-app/Cargo.toml").read_text()
            boundaries.inactive_native_local(root, "examples/engine-app")
            self.assertEqual((root / "examples/engine-app/Cargo.toml").read_text(), application_manifest)
            manifest = tomllib.loads((root / "crates/lavashell/Cargo.toml").read_text())
            name, declaration = next(iter(manifest["target"]['cfg(target_os = "android")']["dependencies"].items()))
            member = root / "crates/lavashell" / declaration["path"]
            self.assertEqual(tomllib.loads((member / "Cargo.toml").read_text())["package"]["name"], name)
            workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
            self.assertIn(member.resolve().relative_to(root).as_posix(), workspace["members"])
            updated_lock = tomllib.loads((root / "Cargo.lock").read_text())
            packages = {package["name"]: package for package in updated_lock["package"]}
            self.assertEqual(packages["lavashell"]["dependencies"], [name])
            self.assertEqual(packages["engine-app"]["dependencies"], [])
            self.assertIn(name, packages)

    def test_engine_and_native_transitions_remove_obsolete_records_across_targets(self):
        previous = {"normalLock": {"version": 4}, "engineLock": {}, "features": {}, "externalUnits": [],
                    "dependencyPlans": {"current": {"engine": "old-engine", "registry": "old-registry"},
                                        "other": {"engine": "other-engine", "registry": "other-registry"}}}
        native = refresh.prepare_metadata(previous, False, "current")
        self.assertEqual(native, {"normalLock": {"version": 4},
                                  "dependencyPlans": {"other": {"registry": "other-registry"}}})
        engine = refresh.prepare_metadata(previous, True, "current")
        self.assertEqual(engine["dependencyPlans"], {"other": {"engine": "other-engine"}})
        self.assertIn("engineLock", engine)
        self.assertEqual(previous["dependencyPlans"]["current"]["engine"], "old-engine")

    def test_original_engine_contract_is_checked_after_cargo_selects_active_members(self):
        seed, graph, metadata, selection = fixture()
        seed["originalContracts"] = [{"member": "inactive", "links": "native", "build": "build.rs"}]
        projection.canonical_plan(seed, graph, metadata, selection)
        for links, build in (("native", False), (None, "build.rs")):
            seed["originalContracts"] = [{"member": "app", "links": links, "build": build}]
            with self.assertRaises(ValueError):
                projection.canonical_plan(seed, graph, metadata, selection)

    def test_a_build_script_and_its_local_build_time_crate_enter_the_plan(self):
        seed, graph, metadata, selection = fixture()
        app, dep = metadata["packages"]
        keys = package("keys", True, [declaration("dep")])
        app["dependencies"].append(declaration("keys", kind="build", source=None))
        metadata["packages"].append(keys)
        graph["units"] += [unit(dep, features=["host"], host=True), unit(keys, [edge(2, "dep")], host=True),
                           unit(app, [edge(3, "keys")], host=True, kind="custom-build"),
                           unit(app, [edge(4, "build_script_build")], mode="run-custom-build")]
        graph["units"][0]["dependencies"].append(edge(5, "build_script_build"))
        seed["originalContracts"] = [{"member": "app", "links": None, "build": "build.rs"},
                                     {"member": "keys", "links": None, "build": False}]
        plan = projection.canonical_plan(seed, graph, metadata, selection)
        crates = {crate["member"]: crate for crate in plan["crates"]}
        self.assertEqual(crates["app"]["manifest"]["package"]["build"], "build.rs")
        self.assertIn({"path": "build.rs", "main": True}, crates["app"]["targets"])
        self.assertEqual(crates["app"]["manifest"]["build-dependencies"]["keys"]["path"], "../keys")
        self.assertEqual(crates["keys"]["manifest"]["dependencies"]["dep"]["features"], ["host"])
        self.assertFalse(crates["keys"]["manifest"]["package"]["build"])

    def test_engine_seed_refreshes_local_closure_and_keeps_registry_jni_edges(self):
        registry = {"name": "jni", "version": "0.22.4", "source": projection.CRATES_IO}
        original = {"version": 4, "package": [registry,
                    {"name": "app", "version": "0.1.0", "dependencies": ["native", "new-local", "jni"]},
                    {"name": "new-local", "version": "0.1.0"}, {"name": "native", "version": "0.1.0"}]}
        baseline = {"version": 4, "package": [registry | {"dependencies": ["registry-child"]},
                    {"name": "app", "version": "0.0.0"}, {"name": "removed-local", "version": "0.0.0"}]}
        result = refresh.engine_seed_lock(original, baseline, ["native"])
        self.assertEqual(result["package"][0], baseline["package"][0])
        app = next(package for package in result["package"] if package["name"] == "app")
        self.assertEqual(app["dependencies"], ["new-local", "jni"])
        self.assertIn("new-local", [package["name"] for package in result["package"]])
        self.assertNotIn("removed-local", [package["name"] for package in result["package"]])

    def test_unused_target_declarations_and_local_closure_do_not_enter_plan(self):
        seed, graph, metadata, selection = fixture()
        before = projection.canonical_plan(seed, graph, metadata, selection)
        metadata["packages"][0]["dependencies"].append(
            declaration("android-only", target='cfg(target_os="android")', source=None))
        metadata["packages"].append(package("android-only", True))
        seed["workspace_manifest"]["workspace"]["members"].append("android-only")
        after = projection.canonical_plan(seed, graph, metadata, selection)
        self.assertEqual(before, after)
        self.assertEqual([crate["member"] for crate in after["crates"]], ["app"])

    def test_equivalent_common_and_active_target_requests_have_one_canonical_dependency(self):
        seed, graph, metadata, selection = fixture()
        before = projection.canonical_plan(seed, graph, metadata, selection)
        metadata["packages"][0]["dependencies"].append(declaration("dep", target='cfg(target_os="linux")'))
        self.assertEqual(before, projection.canonical_plan(seed, graph, metadata, selection))
        dependency = before["crates"][0]["manifest"]["dependencies"]["dep"]
        self.assertEqual(dependency, {"package": "dep", "version": "=1.0.0", "default-features": False,
                                      "features": ["default", "std"]})

    def test_registry_version_feature_profile_and_patch_changes_are_retained(self):
        seed, graph, metadata, selection = fixture()
        baseline = projection.canonical_plan(seed, graph, metadata, selection)
        graph["units"][1]["features"].append("changed")
        self.assertNotEqual(baseline, projection.canonical_plan(seed, graph, metadata, selection))
        graph["units"][1]["features"].remove("changed")
        metadata["packages"][1]["version"] = "2.0.0"
        self.assertNotEqual(baseline, projection.canonical_plan(seed, graph, metadata, selection))
        metadata["packages"][1]["version"] = "1.0.0"
        seed["workspace_manifest"]["profile"]["packaged"]["opt-level"] = 3
        self.assertNotEqual(baseline, projection.canonical_plan(seed, graph, metadata, selection))
        seed["workspace_manifest"]["patch"] = {"crates-io": {"patched": {"git": "https://example.invalid/repo"}}}
        self.assertEqual(projection.canonical_plan(seed, graph, metadata, selection)["manifest"]["patch"],
                         seed["workspace_manifest"]["patch"])

    def test_aliases_and_two_exact_versions_stay_distinct(self):
        seed, graph, metadata, selection = fixture()
        app = metadata["packages"][0]
        app["dependencies"] = [declaration("dep", "old"), declaration("dep", "new")]
        new = package("dep", version="2.0.0")
        metadata["packages"].append(new)
        graph["units"].append(unit(new))
        graph["units"][0]["dependencies"] = [edge(1, "old"), edge(2, "new")]
        declarations = projection.canonical_plan(seed, graph, metadata, selection)["crates"][0]["manifest"]
        self.assertEqual(declarations["dependencies"]["old"]["version"], "=1.0.0")
        self.assertEqual(declarations["dependencies"]["new"]["version"], "=2.0.0")

    def test_host_build_dependencies_and_normal_proc_macros_keep_edge_roles(self):
        seed, graph, metadata, selection = fixture()
        app = metadata["packages"][0]
        app["dependencies"] += [declaration("dep", kind="build"), declaration("derive")]
        derive = package("derive")
        metadata["packages"].append(derive)
        graph["units"] += [unit(metadata["packages"][1], features=["host"], host=True),
                            unit(derive, host=True, kind="proc-macro"),
                            unit(app, [edge(2, "dep")], host=True, kind="custom-build")]
        graph["units"][0]["dependencies"] += [edge(3, "derive"), edge(4, "build_script_build")]
        manifest = projection.canonical_plan(seed, graph, metadata, selection)["crates"][0]["manifest"]
        self.assertIn("derive", manifest["dependencies"])
        self.assertEqual(manifest["build-dependencies"]["dep"]["features"], ["host"])
        self.assertEqual(manifest["dependencies"]["dep"]["features"], ["default", "std"])
        self.assertEqual(manifest["package"]["build"], "build.rs")

    def test_conflicting_requests_in_one_host_target_manifest_slot_fail(self):
        seed, graph, metadata, selection = fixture()
        graph["units"].append(unit(metadata["packages"][1], features=["host"], host=True))
        graph["units"].append(unit(metadata["packages"][0], [edge(2, "dep")], host=True))
        graph["roots"].append(3)
        with self.assertRaisesRegex(ValueError, "distinct host/target requests"):
            projection.canonical_plan(seed, graph, metadata, selection)

    def test_no_temporary_absolute_paths_are_stored(self):
        seed, graph, metadata, selection = fixture()
        child = package("child", True)
        metadata["packages"].append(child)
        metadata["packages"][0]["dependencies"].append(declaration("child", source=None))
        graph["units"].append(unit(child))
        graph["units"][0]["dependencies"].append(edge(2, "child"))
        plan = projection.canonical_plan(seed, graph, metadata, selection)
        self.assertNotIn(ROOT, json.dumps(plan))
        app = next(crate for crate in plan["crates"] if crate["member"] == "app")
        self.assertEqual(app["manifest"]["dependencies"]["child"]["path"], "../child")

    def test_unreachable_units_are_excluded(self):
        seed, graph, metadata, selection = fixture()
        foreign = package("foreign", True)
        metadata["packages"].append(foreign)
        graph["units"].append(unit(foreign))
        self.assertEqual(len(projection.canonical_plan(seed, graph, metadata, selection)["crates"]), 1)

    def test_a_crates_io_dependency_patched_to_git_is_requested_by_its_exact_version(self):
        seed, graph, metadata, selection = fixture()
        git = "git+https://example.invalid/repo?rev=0123abc#0123abc"
        seed["workspace_manifest"]["patch"] = {
            "crates-io": {"dep": {"git": "https://example.invalid/repo", "rev": "0123abc"}},
        }
        metadata["packages"][1].update(source=git, id=f"{git}#dep@1.0.0")
        graph["units"][1]["pkg_id"] = metadata["packages"][1]["id"]
        plan = projection.canonical_plan(seed, graph, metadata, selection)
        self.assertEqual(plan["crates"][0]["manifest"]["dependencies"]["dep"]["version"], "=1.0.0")
        self.assertEqual(plan["manifest"]["patch"], seed["workspace_manifest"]["patch"])
        self.assertEqual(len(identities.external_units(graph)), 1)

    def test_unsupported_source_and_build_contracts_fail_explicitly(self):
        git = "git+https://example.invalid/repo"
        for change in (
            lambda metadata: (metadata["packages"][1].update(source=git),
                              metadata["packages"][0]["dependencies"][0].update(source=git)),
            lambda metadata: metadata["packages"][0].update(links="native-library"),
            lambda metadata: metadata["packages"][0].update(manifest_path="/outside/Cargo.toml"),
        ):
            seed, graph, metadata, selection = fixture()
            change(metadata)
            with self.assertRaises(ValueError):
                projection.canonical_plan(seed, graph, metadata, selection)

    def test_canonical_toml_and_lock_round_trip(self):
        seed, graph, metadata, selection = fixture()
        plan = projection.canonical_plan(seed, graph, metadata, selection)
        for manifest in [plan["manifest"], *(crate["manifest"] for crate in plan["crates"])]:
            self.assertEqual(tomllib.loads(projection.manifest_text(manifest)), manifest)
        lock = {"version": 4, "package": [{"name": "app", "version": "0.0.0", "dependencies": ["dep"]}]}
        self.assertEqual(tomllib.loads(projection.lock_text(lock)), lock)

    def test_supported_toml_strings_nested_tables_and_arrays_round_trip(self):
        special = 'quote" slash\\ tab\t newline\n carriage\r backspace\b formfeed\f DEL\x7f zero\0 café 🦀'
        manifest = {"package": {"name": special, "edition": "2024", "autobins": False},
                    "profile": {"packaged": {"opt-level": 2, "debug": True}},
                    "dependencies": {special: {"version": "=1.0.0", "features": [special, ""]}},
                    "bin": [{"name": "first", "path": "src/first.rs"}, {"name": "second"}]}
        self.assertEqual(tomllib.loads(projection.manifest_text(manifest)), manifest)
        for unsupported in (None, 1.5, "\ud800"):
            with self.assertRaises(ValueError):
                projection.manifest_text({"package": {"name": unsupported}})

    def test_native_common_keeps_explicit_renderer_selection_and_active_closure(self):
        seed, graph, metadata, selection = fixture()
        renderer = package("lavashell-renderer", True)
        metadata["packages"] += [renderer, package("unused-platform", True)]
        metadata["packages"][0]["dependencies"] += [
            declaration("lavashell-renderer", source=None),
            declaration("unused-platform", target='cfg(target_os="other")', source=None),
        ]
        graph["units"].append(unit(renderer, features=["selected"]))
        graph["units"][0]["dependencies"].append(edge(2, "lavashell-renderer"))
        graph["roots"].append(2)
        selection["packages"].append("lavashell-renderer")
        selection["features"].append("lavashell-renderer/selected")
        plan = projection.canonical_plan(seed, graph, metadata, selection)
        self.assertEqual(plan["selection"], selection)
        self.assertEqual(plan["manifest"]["workspace"]["members"], ["app", "lavashell-renderer"])
        self.assertEqual(plan["crates"][1]["manifest"]["features"], {"selected": []})

    def test_path_patch_and_dependency_edge_flags_are_explicit_failures(self):
        seed, graph, metadata, selection = fixture()
        seed["workspace_manifest"]["patch"] = {"crates-io": {"dep": {"path": "patched"}}}
        with self.assertRaisesRegex(ValueError, "path patches"):
            projection.canonical_plan(seed, graph, metadata, selection)
        del seed["workspace_manifest"]["patch"]
        graph["units"][0]["dependencies"][0]["public"] = True
        with self.assertRaisesRegex(ValueError, "dependency-edge flags"):
            projection.canonical_plan(seed, graph, metadata, selection)

    def test_frozen_context_verification_compares_full_records_not_counts(self):
        _, graph, _, _ = fixture()
        changed = copy.deepcopy(graph)
        changed["units"][1]["features"] = ["other"]
        with self.assertRaises(ValueError):
            projection.verify_units(graph, changed, identities.external_units)
        projection.verify_units(graph, copy.deepcopy(graph), identities.external_units)


if __name__ == "__main__":
    unittest.main()
