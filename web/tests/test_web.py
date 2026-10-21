import re

import pytest
from fastapi.testclient import TestClient

from fafnirx_web import db, integrations
from fafnirx_web.app import create_app


@pytest.fixture()
def client(tmp_path, monkeypatch, database_url):
    monkeypatch.setenv("FAFNIRX_DATA", str(tmp_path))
    monkeypatch.setenv("FAFNIRX_DATABASE_URL", database_url)
    monkeypatch.setenv("FAFNIRX_ADMIN_PASSWORD", "hunter2")
    with TestClient(create_app(), follow_redirects=False) as c:
        yield c


def login(c):
    r = c.post("/login", data={"password": "hunter2"})
    assert r.status_code == 303


def scalar(conn, sql, params=()):
    """The first column of the first row."""
    return next(iter(conn.execute(sql, params).fetchone().values()))


def test_requires_login(client):
    for path in ("/", "/settings", "/api/devices", "/devices/x", "/ca.pem", "/sources", "/mirrors"):
        r = client.get(path)
        assert r.status_code == 303 and r.headers["location"] == "/login", path


def test_wrong_password_and_rate_limit(client):
    for _ in range(5):
        assert "Wrong password" in client.post("/login", data={"password": "nope"}).text
    assert "Too many attempts" in client.post("/login", data={"password": "hunter2"}).text


def test_device_lifecycle(client, dbc):
    login(client)
    r = client.post("/devices", data={"name": "Laptop"})
    assert r.status_code == 200
    token = re.search(r'<pre class="code">([\w-]{40,})</pre>', r.text).group(1)

    # Only the hash of the token is stored.
    stored = scalar(dbc, "SELECT token_hash FROM devices WHERE name = 'laptop'")
    assert stored == db.hash_token(token) and token not in stored

    assert "already exists" in client.post("/devices", data={"name": "laptop"}).text
    assert "Name:" in client.post("/devices", data={"name": "Bad Name!"}).text

    r = client.get("/devices/laptop")
    assert r.status_code == 200 and token not in r.text  # token is shown once

    live = client.get("/api/devices").json()
    assert live[0]["name"] == "laptop" and live[0]["online"] is False

    assert client.post("/devices/laptop/revoke").status_code == 303
    assert scalar(dbc, "SELECT revoked_at FROM devices WHERE name = 'laptop'") is not None

    r = client.post("/devices/laptop/token")
    new = re.search(r'<pre class="code">([\w-]{40,})</pre>', r.text).group(1)
    assert new != token
    assert scalar(dbc, "SELECT revoked_at FROM devices WHERE name = 'laptop'") is None


def test_online_state_and_totals(client, dbc):
    login(client)
    client.post("/devices", data={"name": "desk"})
    dbc.execute("UPDATE devices SET last_seen = %s, last_collected = %s", (db.now(), db.now()))
    dbc.execute("INSERT INTO files (device_id, path) VALUES (1, 'docs/a.txt')")
    dbc.execute(
        "INSERT INTO versions (file_id, root_hash, size, mtime, manifest, created_at)"
        " VALUES (1, decode('00', 'hex'), 2048, 0, ''::bytea, 0)"
    )
    dbc.execute("UPDATE files SET current_version_id = 1")
    d = client.get("/api/devices").json()[0]
    assert d["online"] is True and d["collected"] == "2.0 KB" and d["files"] == 1
    assert "docs/a.txt" not in client.get("/").text  # no per-file listing on the home page


def test_alerts_and_settings(client, dbc):
    login(client)
    dbc.execute("INSERT INTO alerts (kind, key, message, created_at) VALUES ('disk_usage', '', 'disk 90% full', 0)")
    assert "disk 90% full" in client.get("/").text
    assert client.post("/alerts/1/dismiss").status_code == 303
    assert "disk 90% full" not in client.get("/").text

    assert client.post("/settings", data={"version_count": 9, "retention_days": 45}).status_code == 303
    assert db.setting(dbc, "version_count", 0) == 9 and db.setting(dbc, "retention_days", 0) == 45
    assert client.post("/settings", data={"version_count": 0, "retention_days": 45}).status_code == 400


def test_password_required_at_startup(tmp_path, monkeypatch, database_url):
    monkeypatch.setenv("FAFNIRX_DATA", str(tmp_path))
    monkeypatch.setenv("FAFNIRX_DATABASE_URL", database_url)
    monkeypatch.delenv("FAFNIRX_ADMIN_PASSWORD", raising=False)
    with pytest.raises(RuntimeError):
        create_app()


def test_database_url_required_at_startup(tmp_path, monkeypatch):
    monkeypatch.setenv("FAFNIRX_DATA", str(tmp_path))
    monkeypatch.setenv("FAFNIRX_ADMIN_PASSWORD", "hunter2")
    monkeypatch.delenv("FAFNIRX_DATABASE_URL", raising=False)
    with pytest.raises(RuntimeError, match="FAFNIRX_DATABASE_URL"):
        create_app()


SA = r'{"client_email": "bot@proj.iam.gserviceaccount.com", "private_key": "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n", "ignored": 1}'
FOLDER = "1AbCdEfGhIjKlMnOpQrStUvWxYz"


def test_parsers():
    urls = f"{FOLDER} https://drive.google.com/drive/folders/{FOLDER}?usp=sharing, 0123456789abc"
    assert integrations.parse_folder_ids(urls) == [FOLDER, "0123456789abc"]
    for bad in ("", "short", "a b/c"):
        with pytest.raises(integrations.FormError):
            integrations.parse_folder_ids(bad)
    assert integrations.parse_options("# c\nprovider = Other\nSecret-Key=a=b\n") == {
        "provider": "Other",
        "secret_key": "a=b",
    }
    with pytest.raises(integrations.FormError):
        integrations.parse_options("RCLONE_CONFIG_X = 1\nno equals")
    assert set(integrations.parse_service_account(SA)) == {"client_email", "private_key"}
    for bad in ("nope", "{}", '{"client_email": "a", "private_key": "x"}'):
        with pytest.raises(integrations.FormError):
            integrations.parse_service_account(bad)


def test_source_lifecycle_and_secrecy(client, dbc):
    login(client)
    r = client.post("/sources", data={"name": "Drive", "folders": FOLDER, "service_account": SA, "interval_hours": 6})
    assert r.status_code == 303
    src = dbc.execute("SELECT * FROM sources").fetchone()
    assert src["name"] == "drive" and src["interval_secs"] == 6 * 3600 and src["last_run_at"] is None
    hidden = dbc.execute("SELECT * FROM devices WHERE id = %s", (src["device_id"],)).fetchone()
    assert hidden["name"] == "source:drive" and hidden["kind"] == "source" and hidden["token_hash"] == "!"

    page = client.get("/sources").text
    assert "drive" in page and "PRIVATE KEY" not in page and "bot@proj" not in page
    assert client.get("/api/devices").json() == [] and "source:drive" not in client.get("/").text, (
        "sources are not devices"
    )
    assert client.post("/devices/source:drive/token").status_code == 404, "a source can never get a token"

    bad = client.post("/sources", data={"name": "drive", "folders": FOLDER, "service_account": SA})
    assert bad.status_code == 400 and "already exists" in bad.text
    bad = client.post("/sources", data={"name": "two", "folders": FOLDER, "service_account": "{}"})
    assert bad.status_code == 400 and "client_email" in bad.text

    client.post("/sources/drive/run")
    assert scalar(dbc, "SELECT run_requested FROM sources") == 1
    client.post("/sources/drive/toggle")
    assert scalar(dbc, "SELECT enabled FROM sources") == 0

    dbc.execute("INSERT INTO files (device_id, path) VALUES (%s, 'x/a.txt')", (src["device_id"],))
    client.post("/sources/drive/delete")
    assert scalar(dbc, "SELECT COUNT(*) FROM sources") == 0
    assert scalar(dbc, "SELECT deleted_at FROM files") is not None, "collected files go through normal retention"
    assert (
        client.post("/sources", data={"name": "drive", "folders": FOLDER, "service_account": SA}).status_code == 303
    ), "name can be reused"
    assert client.post("/sources/nope/run").status_code == 404


def test_mirror_lifecycle_and_secrecy(client, dbc):
    login(client)
    form = {
        "name": "Offsite",
        "remote_type": "S3",
        "options": "provider = Other\nsecret_access_key = s3cr3t",
        "remote_path": "bucket/fx",
    }
    assert client.post("/mirrors", data=form).status_code == 303
    m = dbc.execute("SELECT * FROM mirrors").fetchone()
    assert (m["name"], m["remote_type"], m["interval_secs"], m["keep_runs"]) == ("offsite", "s3", 72 * 3600, 3)
    assert '"secret_access_key": "s3cr3t"' in m["options"]

    page = client.get("/mirrors").text
    assert "offsite" in page and "secret_access_key" in page and "s3cr3t" not in page, (
        "option names are listed, values never"
    )

    r = client.post("/mirrors", data={**form, "name": "x", "options": "bad line"})
    assert r.status_code == 400 and "key=value" in r.text and "s3cr3t" not in r.text
    assert client.post("/mirrors", data={**form, "keep_runs": 0, "name": "y"}).status_code == 400
    assert client.post("/mirrors", data=form).status_code == 400

    dbc.execute(
        "INSERT INTO mirror_items (mirror_id, device_id, path, root_hash) VALUES (%s, 1, 'a', decode('00', 'hex'))",
        (m["id"],),
    )
    client.post("/mirrors/offsite/resync")
    assert scalar(dbc, "SELECT COUNT(*) FROM mirror_items") == 0
    assert scalar(dbc, "SELECT run_requested FROM mirrors") == 1
    client.post("/mirrors/offsite/delete")
    assert scalar(dbc, "SELECT COUNT(*) FROM mirrors") == 0
