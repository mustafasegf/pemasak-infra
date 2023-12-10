CREATE TYPE role AS ENUM ('admin', 'asdos', 'user');
CREATE TYPE build_state AS ENUM ('pending', 'building', 'successful', 'failed');
CREATE TYPE project_state AS ENUM ('empty', 'running', 'stopped', 'idle');

CREATE TABLE users (
  id          UUID          NOT NULL,
  username    VARCHAR(255)  NOT NULL,
  password    TEXT          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,
  role        role          NOT NULL default 'user',

  PRIMARY KEY (id),
  CONSTRAINT unique_username UNIQUE (username)
);

CREATE TABLE project_owners (
  id          UUID          NOT NULL,
  -- TODO: make this unique
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id)
);

-- TODO: make a way to owners must have atleast one user. posibly with trigger or better constraint
-- for many to many relationship
CREATE TABLE users_owners (
  user_id     UUID          NOT NULL,
  owner_id    UUID          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (user_id, owner_id),
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
  FOREIGN KEY (owner_id) REFERENCES project_owners(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE projects (
  id          UUID          NOT NULL,
  owner_id    UUID          NOT NULL,
  name        TEXT          NOT NULL,
  envs        JSONB         NOT NULL default '{}',
  state       project_state NOT NULL default 'empty',
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (owner_id) REFERENCES project_owners(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE domains (
  id          UUID          NOT NULL,
  project_id  UUID          NOT NULL,
  name        TEXT          NOT NULL,
  port        INTEGER       NOT NULL,
  docker_ip   TEXT          NOT NULL,
  -- TODO: rethink if we need this on a seperate table
  db_url      TEXT,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE api_token (
  id          UUID          NOT NULL,
  project_id  UUID          NOT NULL,
  token       TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- for axum_auth_sessions library
CREATE TABLE user_permissions (
  user_id    UUID NOT NULL,
  token      VARCHAR(256) NOT NULL
);

-- for axum_auth_sessions library
CREATE TABLE sessions (
  id VARCHAR(128) NOT NULL PRIMARY KEY,
  expires INTEGER NULL,
  session TEXT NOT NULL
);

-- for tracking build state for each project
CREATE TABLE builds (
  id UUID NOT NULL PRIMARY KEY,
  project_id UUID NOT NULL,
  
  status build_state NOT NULL DEFAULT 'pending',
  log TEXT NOT NULL DEFAULT '',

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,

  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE ON UPDATE CASCADE
);
