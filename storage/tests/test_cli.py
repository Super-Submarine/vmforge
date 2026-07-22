import json

import pytest

from vmforge_storage.cli import main


@pytest.fixture()
def home(tmp_path):
    return str(tmp_path / "vmforge-home")


def run(home, *argv):
    return main(["--home", home, *argv])


def run_json(capsys, home, *argv):
    rc = main(["--home", home, "--json", *argv])
    assert rc == 0
    return json.loads(capsys.readouterr().out)


def test_cli_create_info_check(capsys, home):
    out = run_json(capsys, home, "create", "vm1", "root", "32M")
    assert out["path"].endswith("vms/vm1/disks/root.qcow2")
    chain = run_json(capsys, home, "info", "vm1", "root")
    assert chain[0]["format"] == "qcow2"
    assert chain[0]["virtual-size"] == 32 * 1024 * 1024
    check = run_json(capsys, home, "check", "vm1", "root")
    assert check.get("corruptions", 0) == 0
    assert check.get("check-errors", 0) == 0


def test_cli_resize(capsys, home):
    run_json(capsys, home, "create", "vm1", "root", "16M")
    run_json(capsys, home, "resize", "vm1", "root", "32M")
    chain = run_json(capsys, home, "info", "vm1", "root")
    assert chain[0]["virtual-size"] == 32 * 1024 * 1024


def test_cli_import_and_clone(capsys, home, tmp_path):
    raw = tmp_path / "base.raw"
    raw.write_bytes(b"\x00" * (1024 * 1024))
    run_json(capsys, home, "import", str(raw), "--name", "base", "--format", "raw")
    out = run_json(capsys, home, "clone", "base", "vm2", "root")
    assert out["path"].endswith("vms/vm2/disks/root.qcow2")
    chain = run_json(capsys, home, "info", "vm2", "root")
    assert len(chain) == 2


def test_cli_snapshot_flow(capsys, home):
    run_json(capsys, home, "create", "vm1", "root", "32M")
    run_json(capsys, home, "snapshot", "create", "vm1", "root", "s1")
    run_json(capsys, home, "snapshot", "create", "vm1", "root", "s2")
    snaps = run_json(capsys, home, "snapshot", "list", "vm1", "root")
    assert {s["name"] for s in snaps} == {"s1", "s2"}
    run_json(capsys, home, "snapshot", "revert", "vm1", "root", "s1")
    run_json(capsys, home, "snapshot", "delete", "vm1", "root", "s2")
    snaps = run_json(capsys, home, "snapshot", "list", "vm1", "root")
    assert {s["name"] for s in snaps} == {"s1"}
    # human-readable tree output
    rc = run(home, "snapshot", "list", "vm1", "root")
    assert rc == 0
    assert "s1 *" in capsys.readouterr().out


def test_cli_error_exit_code(capsys, home):
    rc = run(home, "info", "vm1", "missing")
    assert rc == 1
    err = json.loads(capsys.readouterr().err)
    assert err["error"]["code"] == "not_found"


def test_cli_contract_version(capsys):
    with pytest.raises(SystemExit) as excinfo:
        main(["--contract-version"])
    assert excinfo.value.code == 0
    assert capsys.readouterr().out.strip() == "1"


def test_cli_tree_alias(capsys, home):
    run_json(capsys, home, "create", "vm1", "root", "16M")
    run_json(capsys, home, "snapshot", "create", "vm1", "root", "s1")
    snaps = run_json(capsys, home, "tree", "vm1", "root")
    assert {s["name"] for s in snaps} == {"s1"}


def test_cli_error_json_shapes(capsys, home):
    # not_found: revert to a snapshot that does not exist
    run_json(capsys, home, "create", "vm1", "root", "16M")
    rc = run(home, "--json", "snapshot", "revert", "vm1", "root", "nope")
    assert rc == 1
    err = json.loads(capsys.readouterr().err)
    assert err["error"]["code"] == "not_found"
    assert err["error"]["message"]

    # invalid_config: name violating the shared regex
    rc = run(home, "create", "vm1", "bad name", "16M")
    assert rc == 1
    err = json.loads(capsys.readouterr().err)
    assert err["error"]["code"] == "invalid_config"

    # already_exists
    rc = run(home, "create", "vm1", "root", "16M")
    assert rc == 1
    err = json.loads(capsys.readouterr().err)
    assert err["error"]["code"] == "already_exists"


def test_cli_backend_error_details(capsys, home):
    # qemu-img failure surfaces as code=backend with details.stderr
    run_json(capsys, home, "create", "vm1", "root", "16M")
    rc = run(home, "resize", "vm1", "root", "8M")  # shrink without --shrink
    assert rc == 1
    err = json.loads(capsys.readouterr().err)
    assert err["error"]["code"] == "backend"
    assert err["error"]["details"]["stderr"]


def test_cli_delete(capsys, home):
    run_json(capsys, home, "create", "vm1", "root", "16M")
    run_json(capsys, home, "delete", "vm1", "root")
    rc = run(home, "info", "vm1", "root")
    assert rc == 1
