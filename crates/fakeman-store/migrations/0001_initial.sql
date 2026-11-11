CREATE TABLE endpoints (
  id          INTEGER PRIMARY KEY,
  name        TEXT NOT NULL,
  method      TEXT NOT NULL DEFAULT 'POST',
  url         TEXT NOT NULL DEFAULT '',
  body_format TEXT NOT NULL DEFAULT 'json' CHECK (body_format IN ('json', 'form')),
  run_count   INTEGER NOT NULL DEFAULT 1 CHECK (run_count >= 1),
  delay_ms    INTEGER NOT NULL DEFAULT 0 CHECK (delay_ms >= 0),
  sort_order  INTEGER NOT NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

CREATE TABLE fields (
  id            INTEGER PRIMARY KEY,
  endpoint_id   INTEGER NOT NULL REFERENCES endpoints(id) ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  name          TEXT NOT NULL,
  generator_key TEXT NOT NULL,
  params_json   TEXT NOT NULL DEFAULT '{}',
  enabled       INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE headers (
  id          INTEGER PRIMARY KEY,
  endpoint_id INTEGER NOT NULL REFERENCES endpoints(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  key         TEXT NOT NULL,
  value       TEXT NOT NULL,
  enabled     INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX fields_endpoint ON fields (endpoint_id, position);
CREATE INDEX headers_endpoint ON headers (endpoint_id, position);
