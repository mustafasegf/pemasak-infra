CREATE TABLE users (
  id          UUID          NOT NULL,
  username    VARCHAR(255)  NOT NULL,
  password    TEXT          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  CONSTRAINT unique_username UNIQUE (username)
);

-- TODO: change this to per project
CREATE TABLE api_token (
  id          UUID          NOT NULL,
  user_id     UUID          NOT NULL,
  name        VARCHAR(255)  NOT NULL,
  token       TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE user_permissions (
  user_id    UUID NOT NULL,
  token      VARCHAR(256) NOT NULL
);

CREATE TABLE sessions (
  id VARCHAR(128) NOT NULL PRIMARY KEY,
  expires INTEGER NULL,
  session TEXT NOT NULL
);

CREATE TABLE owners (
  id          UUID          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id)
);

-- TODO: make a way to owners must have atleast one user
CREATE TABLE users_owners (
  user_id     UUID          NOT NULL,
  owner_id    UUID          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (user_id, owner_id),
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
  FOREIGN KEY (owner_id) REFERENCES owners(id) ON DELETE CASCADE ON UPDATE CASCADE
);


CREATE TABLE repositories (
  id          UUID          NOT NULL,
  owner_id    UUID          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (owner_id) REFERENCES owners(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE domains (
  id          UUID          NOT NULL,
  repo_id     UUID          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  TIMESTAMPTZ   NOT NULL default now(),
  updated_at  TIMESTAMPTZ   NOT NULL default now(),
  deleted_at  TIMESTAMPTZ,

  PRIMARY KEY (id),
  FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE ON UPDATE CASCADE
);

